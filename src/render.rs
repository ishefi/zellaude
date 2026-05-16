use crate::state::{
    unix_now, unix_now_ms, Activity, BeepMode, ClickRegion, FlashMode, LogLevel, MenuAction,
    MenuClickRegion, NotifyMode, RemoteTagClickRegion, RemoteTagKind, SessionInfo, SettingKey,
    State, ViewMode,
};
use std::fmt::Write;
use std::io::Write as IoWrite;
use zellij_tile::prelude::{InputMode, TabInfo};

struct Style {
    symbol: &'static str,
    r: u8,
    g: u8,
    b: u8,
}

fn activity_priority(activity: &Activity) -> u8 {
    match activity {
        Activity::Waiting => 8,
        Activity::Tool(_) => 7,
        Activity::Thinking => 6,
        Activity::Prompting => 5,
        Activity::Notification => 4,
        Activity::Init => 3,
        Activity::Done => 2,
        Activity::AgentDone => 1,
        Activity::Idle => 0,
    }
}

fn activity_style(activity: &Activity) -> Style {
    match activity {
        Activity::Init => Style {
            symbol: "◆",
            r: 180,
            g: 175,
            b: 195,
        },
        Activity::Thinking => Style {
            symbol: "●",
            r: 180,
            g: 140,
            b: 255,
        },
        Activity::Tool(name) => {
            let symbol = match name.as_str() {
                "Bash" => "⚡",
                "Read" | "Glob" | "Grep" => "◉",
                "Edit" | "Write" => "✎",
                "Task" => "⊜",
                "WebSearch" | "WebFetch" => "◈",
                _ => "⚙",
            };
            Style {
                symbol,
                r: 255,
                g: 170,
                b: 50,
            }
        }
        Activity::Prompting => Style {
            symbol: "▶",
            r: 80,
            g: 200,
            b: 120,
        },
        Activity::Waiting => Style {
            symbol: "⚠",
            r: 255,
            g: 60,
            b: 60,
        },
        Activity::Notification => Style {
            symbol: "◇",
            r: 200,
            g: 200,
            b: 100,
        },
        Activity::Done => Style {
            symbol: "✓",
            r: 80,
            g: 200,
            b: 120,
        },
        Activity::AgentDone => Style {
            symbol: "✓",
            r: 80,
            g: 180,
            b: 100,
        },
        Activity::Idle => Style {
            symbol: "○",
            r: 180,
            g: 175,
            b: 195,
        },
    }
}

fn fg(r: u8, g: u8, b: u8) -> String {
    format!("\x1b[38;2;{r};{g};{b}m")
}

fn bg(r: u8, g: u8, b: u8) -> String {
    format!("\x1b[48;2;{r};{g};{b}m")
}

fn display_width(s: &str) -> usize {
    s.chars().count()
}

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const ELAPSED_THRESHOLD: u64 = 30;
const SEPARATOR: &str = "\u{e0b0}";

type Color = (u8, u8, u8);
const BAR_BG: Color = (30, 30, 46);
const PREFIX_BG: Color = (60, 50, 80);
const PREFIX_BG_SETTINGS: Color = (100, 70, 140);
const TAB_BG_ACTIVE: Color = (140, 100, 200);
const TAB_BG_INACTIVE: Color = (80, 75, 110);
const FLASH_BG_BRIGHT: Color = (80, 80, 30);

/// Write a powerline arrow: fg=from_bg, bg=to_bg, then separator char.
fn arrow(buf: &mut String, col: &mut usize, from: Color, to: Color) {
    let _ = write!(
        buf,
        "{}{}{SEPARATOR}",
        fg(from.0, from.1, from.2),
        bg(to.0, to.1, to.2),
    );
    *col += 1;
}

fn format_elapsed(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h", secs / 3600)
    }
}

fn mode_style(mode: InputMode) -> (Color, &'static str) {
    match mode {
        InputMode::Normal => ((80, 200, 120), "NORMAL"),
        InputMode::Locked => ((255, 80, 80), "LOCKED"),
        InputMode::Pane => ((80, 180, 255), "PANE"),
        InputMode::Tab => ((180, 140, 255), "TAB"),
        InputMode::Resize => ((255, 170, 50), "RESIZE"),
        InputMode::Move => ((255, 170, 50), "MOVE"),
        InputMode::Scroll => ((200, 200, 100), "SCROLL"),
        InputMode::EnterSearch => ((200, 200, 100), "SEARCH"),
        InputMode::Search => ((200, 200, 100), "SEARCH"),
        InputMode::RenameTab => ((200, 200, 100), "RENAME"),
        InputMode::RenamePane => ((200, 200, 100), "RENAME"),
        InputMode::Session => ((180, 140, 255), "SESSION"),
        InputMode::Prompt => ((80, 200, 120), "PROMPT"),
        InputMode::Tmux => ((80, 200, 120), "TMUX"),
    }
}

pub fn render_status_bar(state: &mut State, _rows: usize, cols: usize) {
    state.click_regions.clear();
    state.menu_click_regions.clear();
    state.remote_tag_click_regions.clear();

    let mut buf = String::with_capacity(cols * 4);
    // Pending notification beeps. Every plugin instance queues its own beep
    // when a hook arrives, but only the instance whose tab is active should
    // actually emit — otherwise a queued beep would fire later when the user
    // switches to that tab, long after the event. Always drain the set so
    // entries don't accumulate across renders.
    let local_beep_candidate = !state.beep_pending.is_empty();
    let local_beep = state.settings.beep.beeps_local()
        && state.beep_pending.iter().any(|pane_id| {
            state
                .pane_to_tab
                .get(pane_id)
                .is_some_and(|(tab_idx, _)| Some(*tab_idx) == state.active_tab_index)
        });
    let local_pending_count = state.beep_pending.len();
    state.beep_pending.clear();
    // Cross-session beep: fires when reconcile_remote_tags actually enqueued
    // a new tag this cycle. A remote-session's tag is presented at most once
    // per lifetime (until the remote's state file is evicted from
    // remote_sessions), so a `/loop` cycling Done→Working→Done won't repeat
    // the bell. Not gated on active tab — the remote isn't tied to any local
    // tab.
    let remote_pending_raw = state.beep_remote_pending;
    let remote_beep = state.settings.beep.beeps_remote() && state.beep_remote_pending;
    state.beep_remote_pending = false;
    if local_beep || remote_beep {
        // Don't push '\x07' into the status-bar buffer — Zellij's grid parser
        // would consume it. Shell out instead so the bell reaches the host
        // pty (and through SSH to the user's terminal emulator).
        state.ring_terminal_bell();
        state.log(
            LogLevel::Info,
            &format!(
                "render: BEL emitted (local={} remote={} beep_setting={:?})",
                local_beep, remote_beep, state.settings.beep
            ),
        );
    } else if remote_pending_raw {
        state.log(
            LogLevel::Debug,
            &format!(
                "render: cross-session beep pending but suppressed by beep_setting={:?}",
                state.settings.beep
            ),
        );
    } else if local_beep_candidate {
        state.log(
            LogLevel::Trace,
            &format!(
                "render: {} local beep_pending entries but none on active tab (idx={:?})",
                local_pending_count, state.active_tab_index
            ),
        );
    }
    // Terminal setup for a 1-row status bar:
    //  \x1b[H     — cursor home (prevent scroll from cursor at end-of-line)
    //  \x1b[?7l   — disable auto-wrap (clip overflow instead of scroll)
    //  \x1b[?25l  — hide cursor
    buf.push_str("\x1b[H\x1b[?7l\x1b[?25l");
    let bar_bg_str = bg(BAR_BG.0, BAR_BG.1, BAR_BG.2);

    // Bail early if terminal is too narrow
    if cols < 5 {
        let _ = write!(buf, "{bar_bg_str}{:width$}{RESET}", "", width = cols);
        print!("{buf}");
        let _ = std::io::stdout().flush();
        return;
    }

    let prefix_bg = if state.view_mode == ViewMode::Settings {
        PREFIX_BG_SETTINGS
    } else {
        PREFIX_BG
    };

    // Build prefix: " Zellaude (session) MODE "
    let (mode_bg, mode_text) = mode_style(state.input_mode);
    let show_mode = state.settings.mode_indicator;
    let session_part = match state.zellij_session_name.as_deref() {
        Some(name) => format!(" ({name})"),
        None => String::new(),
    };
    let prefix_text = format!(" Zellaude{session_part} ");
    let prefix_width = display_width(&prefix_text);
    let mode_pill_width = if show_mode {
        1 + mode_text.len() + 1
    } else {
        0
    };
    let total_prefix_width = prefix_width + mode_pill_width;

    // Render prefix segment (truncate if wider than cols)
    let mut col;
    if total_prefix_width <= cols {
        let _ = write!(
            buf,
            "{}{}{BOLD}{prefix_text}{RESET}",
            bg(prefix_bg.0, prefix_bg.1, prefix_bg.2),
            fg(255, 255, 255),
        );
        if show_mode {
            let _ = write!(
                buf,
                "{}{}{BOLD} {mode_text} {RESET}",
                bg(mode_bg.0, mode_bg.1, mode_bg.2),
                fg(30, 30, 46),
            );
        }
        col = total_prefix_width;
    } else if prefix_width <= cols {
        // Fit the name part but skip mode pill
        let _ = write!(
            buf,
            "{}{}{BOLD}{prefix_text}{RESET}",
            bg(prefix_bg.0, prefix_bg.1, prefix_bg.2),
            fg(255, 255, 255),
        );
        col = prefix_width;
    } else {
        // Even name doesn't fit — just show what we can
        let avail = cols.saturating_sub(2); // leave room for fill
        let short: String = prefix_text.chars().take(avail).collect();
        let _ = write!(
            buf,
            "{}{}{BOLD}{short}{RESET}",
            bg(prefix_bg.0, prefix_bg.1, prefix_bg.2),
            fg(255, 255, 255),
        );
        col = display_width(&short);
    }
    state.prefix_click_region = Some((0, col));

    let last_prefix_bg = if show_mode && total_prefix_width <= cols {
        mode_bg
    } else {
        prefix_bg
    };
    let prefix_used = col;

    if col < cols {
        match state.view_mode {
            ViewMode::Normal => {
                render_tabs(state, &mut buf, &mut col, cols, last_prefix_bg, prefix_used);
                render_remote_cluster(state, &mut buf, &mut col, cols);
            }
            ViewMode::Settings => {
                arrow(&mut buf, &mut col, last_prefix_bg, BAR_BG);
                let _ = write!(buf, "{bar_bg_str}");
                render_settings_menu(state, &mut buf, &mut col);
            }
        }
    }

    // Fill remaining width with bar background — never exceed cols
    if col < cols {
        let remaining = cols - col;
        let _ = write!(buf, "{bar_bg_str}{:width$}", "", width = remaining);
    }
    let _ = write!(buf, "{RESET}");

    print!("{buf}");
    let _ = std::io::stdout().flush();
}

fn render_tabs(
    state: &mut State,
    buf: &mut String,
    col: &mut usize,
    cols: usize,
    prefix_bg: Color,
    prefix_width: usize,
) {
    let now_s = unix_now();
    let now_ms = unix_now_ms();

    // Sort tabs by position
    let mut tabs: Vec<&TabInfo> = state.tabs.iter().collect();
    tabs.sort_by_key(|t| t.position);

    let count = tabs.len();
    if count == 0 {
        arrow(buf, col, prefix_bg, BAR_BG);
        return;
    }

    // For each tab, find the best (highest-priority) Claude session
    let best_sessions: Vec<Option<&SessionInfo>> = tabs
        .iter()
        .map(|tab| {
            state
                .sessions
                .values()
                .filter(|s| s.tab_index == Some(tab.position))
                .max_by_key(|s| activity_priority(&s.activity))
        })
        .collect();

    // Pre-compute elapsed strings (only for Claude tabs)
    let elapsed_strs: Vec<Option<String>> = best_sessions
        .iter()
        .map(|session: &Option<&SessionInfo>| {
            if !state.settings.elapsed_time {
                return None;
            }
            session.and_then(|s| {
                let elapsed = now_s.saturating_sub(s.last_event_ts);
                if elapsed >= ELAPSED_THRESHOLD {
                    Some(format_elapsed(elapsed))
                } else {
                    None
                }
            })
        })
        .collect();

    // Compute overhead: varies per tab type
    let total_elapsed_width: usize = elapsed_strs
        .iter()
        .map(|e: &Option<String>| e.as_ref().map_or(0, |s| s.len() + 1))
        .sum();
    let per_tab_overhead: usize = best_sessions
        .iter()
        .map(|s: &Option<&SessionInfo>| if s.is_some() { 4 } else { 2 })
        .sum();
    let overhead = prefix_width + 2 * count + per_tab_overhead + total_elapsed_width;
    let max_name_len = if overhead < cols {
        ((cols - overhead) / count).min(20)
    } else {
        0
    };

    let mut prev_bg = prefix_bg;

    for (i, tab) in tabs.iter().enumerate() {
        // Stop if we'd overflow — need room for at least arrow + closing arrow
        let arrows_needed = if prev_bg == prefix_bg { 1 } else { 2 };
        if *col + arrows_needed + 3 > cols {
            break;
        }

        let session = best_sessions[i];
        let is_claude = session.is_some();
        let tab_name = &tab.name;

        // Truncate name
        let char_count = tab_name.chars().count();
        let truncated = if max_name_len == 0 {
            String::new()
        } else if char_count > max_name_len {
            let s: String = tab_name
                .chars()
                .take(max_name_len.saturating_sub(1))
                .collect();
            format!("{s}…")
        } else {
            tab_name.to_string()
        };

        // Check flash for any session in this tab
        let is_flash_bright = state
            .sessions
            .values()
            .filter(|s| s.tab_index == Some(tab.position))
            .any(|s| {
                state
                    .flash_deadlines
                    .get(&s.pane_id)
                    .map(|&deadline| now_ms < deadline && (now_ms / 250) % 2 == 0)
                    .unwrap_or(false)
            });

        let is_active = tab.active;

        // Pick tab background color
        let tab_bg = if is_flash_bright {
            FLASH_BG_BRIGHT
        } else if is_active {
            TAB_BG_ACTIVE
        } else {
            TAB_BG_INACTIVE
        };

        // Arrow: close previous segment, then open this tab
        if prev_bg == prefix_bg {
            arrow(buf, col, prev_bg, tab_bg);
        } else {
            arrow(buf, col, prev_bg, BAR_BG);
            arrow(buf, col, BAR_BG, tab_bg);
        }

        let tab_bg_str = bg(tab_bg.0, tab_bg.1, tab_bg.2);
        let region_start = *col;

        if is_claude {
            let s = session.unwrap();
            let style = activity_style(&s.activity);

            let (sym_fg, name_fg, name_bold) = if is_flash_bright {
                (fg(255, 255, 80), fg(255, 255, 80), true)
            } else if is_active {
                (fg(style.r, style.g, style.b), fg(255, 255, 255), true)
            } else {
                (fg(style.r, style.g, style.b), fg(120, 220, 220), false)
            };

            // Leading space
            let _ = write!(buf, "{tab_bg_str} ");
            *col += 1;

            // Symbol
            let _ = write!(buf, "{sym_fg}{}", style.symbol);
            *col += display_width(style.symbol);

            // Space + name
            if !truncated.is_empty() {
                let bold_str = if name_bold { BOLD } else { "" };
                let _ = write!(buf, " {bold_str}{name_fg}{truncated}{RESET}{tab_bg_str}");
                *col += 1 + display_width(&truncated);
            }

            // Elapsed suffix
            if let Some(ref es) = elapsed_strs[i] {
                if *col + 1 + es.len() + 1 < cols {
                    let _ = write!(buf, " {}{es}", fg(165, 160, 180));
                    *col += 1 + es.len();
                }
            }

            // Fullscreen indicator
            if tab.is_fullscreen_active && *col + 3 < cols {
                let _ = write!(buf, " {}F{RESET}{tab_bg_str}", fg(255, 200, 60));
                *col += 2;
            }

            // Trailing space
            let _ = write!(buf, " ");
            *col += 1;

            // Click region: if any session is waiting, use its pane_id for focus
            let waiting_session = state
                .sessions
                .values()
                .filter(|s| s.tab_index == Some(tab.position))
                .find(|s| matches!(s.activity, Activity::Waiting));

            state.click_regions.push(ClickRegion {
                start_col: region_start,
                end_col: *col,
                tab_index: tab.position,
                pane_id: waiting_session.map_or(0, |s| s.pane_id),
                is_waiting: waiting_session.is_some(),
            });
        } else {
            // Non-Claude tab: dimmer, no symbol
            let name_fg = if is_active {
                fg(220, 215, 230)
            } else {
                fg(170, 165, 185)
            };
            let name_bold = is_active;

            // Leading space
            let _ = write!(buf, "{tab_bg_str} ");
            *col += 1;

            // Name only (no symbol)
            if !truncated.is_empty() {
                let bold_str = if name_bold { BOLD } else { "" };
                let _ = write!(buf, "{bold_str}{name_fg}{truncated}{RESET}{tab_bg_str}");
                *col += display_width(&truncated);
            }

            // Fullscreen indicator
            if tab.is_fullscreen_active && *col + 3 < cols {
                let _ = write!(buf, " {}F{RESET}{tab_bg_str}", fg(255, 200, 60));
                *col += 2;
            }

            // Trailing space
            let _ = write!(buf, " ");
            *col += 1;

            state.click_regions.push(ClickRegion {
                start_col: region_start,
                end_col: *col,
                tab_index: tab.position,
                pane_id: 0,
                is_waiting: false,
            });
        }

        prev_bg = tab_bg;
    }

    // Arrow from last tab → bar background (only if we rendered any tabs)
    if prev_bg != prefix_bg || count > 0 {
        arrow(buf, col, prev_bg, BAR_BG);
    }
}

fn render_remote_cluster(state: &mut State, buf: &mut String, col: &mut usize, cols: usize) {
    let bar_bg_str = bg(BAR_BG.0, BAR_BG.1, BAR_BG.2);
    let dim_red = fg(200, 100, 100);
    let dim_green = fg(120, 200, 130);
    let max_len = state.settings.cross_session_tag_max_len.max(1);
    let cap = state.settings.max_cross_session_tags.max(1);

    let total = state.remote_tag_order.len();
    // Reserve room for the worst-case overflow chip up front so a narrow
    // terminal doesn't drop both the tags AND the indicator. Since the actual
    // chip text is `+{total - shown}` and shown ≤ cap, `total - shown` is at
    // most `total` — using `format!(" +{total} ")` is an upper bound.
    let chip_reserve = if total > cap {
        format!(" +{total} ").len()
    } else {
        0
    };
    let tag_budget = cols.saturating_sub(chip_reserve);

    let mut shown = 0usize;
    let mut overflow_start = state.remote_tag_order.len();
    for (idx, (session_name, kind)) in state.remote_tag_order.iter().enumerate() {
        if shown >= cap {
            overflow_start = idx;
            break;
        }
        let Some(remote) = state.remote_sessions.get(session_name) else {
            continue;
        };
        let name: String = remote.session_name.chars().take(max_len).collect();
        // Layout: " ↗ <name> <icon> " — 6 fixed cols + name width.
        let needed = 6 + display_width(&name);
        if *col + needed >= tag_budget {
            // Stop, but fall through so the overflow chip can still render
            // within the reserved budget.
            overflow_start = idx;
            break;
        }
        let (chip_fg, icon) = match kind {
            RemoteTagKind::Waiting => (&dim_red, "\u{26A0}"),
            RemoteTagKind::Done => (&dim_green, "\u{2713}"),
        };
        let region_start = *col;
        let _ = write!(buf, "{bar_bg_str}{chip_fg} \u{2197} {name} {icon} {RESET}");
        *col += needed;
        state.remote_tag_click_regions.push(RemoteTagClickRegion {
            start_col: region_start,
            end_col: *col,
            session_name: session_name.clone(),
            kind: *kind,
        });
        shown += 1;
    }

    if total > shown {
        let overflow = total - shown;
        let chip = format!(" +{overflow} ");
        let needed = chip.len();
        if *col + needed >= cols {
            return;
        }
        // Escalate to the most urgent hidden kind: red if any Waiting is
        // hidden, green otherwise. Avoids falsely implying hidden Waiting
        // tags when only Done tags overflow.
        let any_hidden_waiting = state
            .remote_tag_order
            .iter()
            .skip(overflow_start)
            .any(|(_, kind)| matches!(kind, RemoteTagKind::Waiting));
        let chip_fg = if any_hidden_waiting {
            &dim_red
        } else {
            &dim_green
        };
        let _ = write!(buf, "{bar_bg_str}{chip_fg}{chip}{RESET}");
        *col += needed;
    }
}

fn notify_mode_label(mode: NotifyMode) -> (&'static str, &'static str, String, String) {
    match mode {
        NotifyMode::Always => ("●", "Notify: always", fg(80, 200, 120), fg(255, 255, 255)),
        NotifyMode::Unfocused => ("◐", "Notify: unfocused", fg(255, 200, 60), fg(255, 200, 60)),
        NotifyMode::Never => ("○", "Notify: off", fg(100, 100, 100), fg(100, 100, 100)),
    }
}

fn flash_mode_label(mode: FlashMode) -> (&'static str, &'static str, String, String) {
    match mode {
        FlashMode::Persist => ("●", "Flash: persist", fg(80, 200, 120), fg(255, 255, 255)),
        FlashMode::Once => ("◐", "Flash: brief", fg(255, 200, 60), fg(255, 200, 60)),
        FlashMode::Off => ("○", "Flash: off", fg(100, 100, 100), fg(100, 100, 100)),
    }
}

/// Render a three-state toggle and register its click region.
/// Assumes the caller has already set the desired background color.
fn render_tristate(
    buf: &mut String,
    col: &mut usize,
    state_regions: &mut Vec<MenuClickRegion>,
    key: SettingKey,
    symbol: &str,
    label: &str,
    sym_color: &str,
    label_color: &str,
) {
    let region_start = *col;
    let width = display_width(symbol) + 1 + label.len();
    *col += width;

    state_regions.push(MenuClickRegion {
        start_col: region_start,
        end_col: *col,
        action: MenuAction::ToggleSetting(key),
    });

    let _ = write!(buf, "{sym_color}{symbol} {label_color}{label}");
}

fn render_settings_menu(state: &mut State, buf: &mut String, col: &mut usize) {
    // Leading space after arrow
    let _ = write!(buf, " ");
    *col += 1;

    // --- Notifications (three-state) ---
    {
        let (symbol, label, sym_color, label_color) =
            notify_mode_label(state.settings.notifications);
        render_tristate(
            buf,
            col,
            &mut state.menu_click_regions,
            SettingKey::Notifications,
            symbol,
            label,
            &sym_color,
            &label_color,
        );
    }

    // --- Flash (three-state) ---
    {
        let _ = write!(buf, "  ");
        *col += 2;
        let (symbol, label, sym_color, label_color) = flash_mode_label(state.settings.flash);
        render_tristate(
            buf,
            col,
            &mut state.menu_click_regions,
            SettingKey::Flash,
            symbol,
            label,
            &sym_color,
            &label_color,
        );
    }

    // --- Elapsed time (bool) ---
    {
        let _ = write!(buf, "  ");
        *col += 2;
        let enabled = state.settings.elapsed_time;
        let (symbol, sym_color, label_color) = if enabled {
            ("●", fg(80, 200, 120), fg(255, 255, 255))
        } else {
            ("○", fg(100, 100, 100), fg(100, 100, 100))
        };
        let label = if enabled {
            "Elapsed time: on"
        } else {
            "Elapsed time: off"
        };
        render_tristate(
            buf,
            col,
            &mut state.menu_click_regions,
            SettingKey::ElapsedTime,
            symbol,
            label,
            &sym_color,
            &label_color,
        );
    }

    // --- Mode indicator (bool) ---
    {
        let _ = write!(buf, "  ");
        *col += 2;
        let enabled = state.settings.mode_indicator;
        let (symbol, sym_color, label_color) = if enabled {
            ("●", fg(80, 200, 120), fg(255, 255, 255))
        } else {
            ("○", fg(100, 100, 100), fg(100, 100, 100))
        };
        let label = if enabled {
            "Mode indicator: on"
        } else {
            "Mode indicator: off"
        };
        render_tristate(
            buf,
            col,
            &mut state.menu_click_regions,
            SettingKey::ModeIndicator,
            symbol,
            label,
            &sym_color,
            &label_color,
        );
    }

    // --- Beep (tristate: On / CrossSession / Off) ---
    {
        let _ = write!(buf, "  ");
        *col += 2;
        let (symbol, sym_color, label_color, label) = match state.settings.beep {
            BeepMode::On => ("●", fg(80, 200, 120), fg(255, 255, 255), "Beep: on"),
            BeepMode::CrossSession => (
                "◐",
                fg(80, 200, 120),
                fg(255, 255, 255),
                "Beep: cross-session",
            ),
            BeepMode::Off => ("○", fg(100, 100, 100), fg(100, 100, 100), "Beep: off"),
        };
        render_tristate(
            buf,
            col,
            &mut state.menu_click_regions,
            SettingKey::Beep,
            symbol,
            label,
            &sym_color,
            &label_color,
        );
    }

    // --- Persist cross-session tags (bool) ---
    {
        let _ = write!(buf, "  ");
        *col += 2;
        let enabled = state.settings.persist_cross_session_tags;
        let (symbol, sym_color, label_color) = if enabled {
            ("●", fg(80, 200, 120), fg(255, 255, 255))
        } else {
            ("○", fg(100, 100, 100), fg(100, 100, 100))
        };
        let label = if enabled {
            "Persist tags: on"
        } else {
            "Persist tags: off"
        };
        render_tristate(
            buf,
            col,
            &mut state.menu_click_regions,
            SettingKey::PersistCrossSessionTags,
            symbol,
            label,
            &sym_color,
            &label_color,
        );
    }

    // --- Max cross-session tags (cycler 1→2→3→4→1) ---
    {
        let _ = write!(buf, "  ");
        *col += 2;
        let n = state.settings.max_cross_session_tags.max(1);
        let symbol = "◆";
        let sym_color = fg(80, 200, 120);
        let label_color = fg(255, 255, 255);
        let label = format!("Max tags: {n}");
        render_tristate(
            buf,
            col,
            &mut state.menu_click_regions,
            SettingKey::MaxCrossSessionTags,
            symbol,
            &label,
            &sym_color,
            &label_color,
        );
    }

    // --- Log level (cycler off→error→warn→info→debug→trace→off) ---
    {
        let _ = write!(buf, "  ");
        *col += 2;
        let level = state.settings.log_level;
        let symbol = if level == LogLevel::Off { "○" } else { "◆" };
        let sym_color = if level == LogLevel::Off {
            fg(100, 100, 100)
        } else {
            fg(255, 180, 60)
        };
        let label_color = if level == LogLevel::Off {
            fg(100, 100, 100)
        } else {
            fg(255, 255, 255)
        };
        let label = format!("Log: {}", level.label());
        render_tristate(
            buf,
            col,
            &mut state.menu_click_regions,
            SettingKey::LogLevel,
            symbol,
            &label,
            &sym_color,
            &label_color,
        );
    }

    // Close button
    let _ = write!(buf, "  ");
    *col += 2;
    let close_start = *col;
    let _ = write!(buf, "{}×", fg(255, 60, 60));
    *col += 1;

    state.menu_click_regions.push(MenuClickRegion {
        start_col: close_start,
        end_col: *col,
        action: MenuAction::CloseMenu,
    });
}
