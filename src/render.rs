use crate::palette::{Color, Palette};
use crate::state::{
    unix_now, unix_now_ms, Activity, ClickRegion, FlashMode, MenuAction, MenuClickRegion,
    NotifyMode, SessionInfo, SettingKey, State, ViewMode,
};
use std::fmt::Write;
use std::io::Write as IoWrite;
use zellij_tile::prelude::{InputMode, TabInfo};

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

/// The glyph for an activity. Its color comes from the palette (see `activity_color`).
fn activity_symbol(activity: &Activity) -> &'static str {
    match activity {
        Activity::Init => "◆",
        Activity::Thinking => "●",
        Activity::Tool(name) => match name.as_str() {
            "Bash" => "⚡",
            "Read" | "Glob" | "Grep" => "◉",
            "Edit" | "Write" => "✎",
            "Task" => "⊜",
            "WebSearch" | "WebFetch" => "◈",
            _ => "⚙",
        },
        Activity::Prompting => "▶",
        Activity::Waiting => "⚠",
        Activity::Notification => "◇",
        Activity::Done => "✓",
        Activity::AgentDone => "✓",
        Activity::Idle => "○",
    }
}

/// The palette color for an activity glyph.
fn activity_color(p: &Palette, activity: &Activity) -> Color {
    match activity {
        Activity::Init => p.neutral,
        Activity::Thinking => p.thinking,
        Activity::Tool(_) => p.tool,
        Activity::Prompting => p.success,
        Activity::Waiting => p.waiting,
        Activity::Notification => p.notification,
        Activity::Done => p.success,
        Activity::AgentDone => p.success,
        Activity::Idle => p.neutral,
    }
}

fn fg(r: u8, g: u8, b: u8) -> String {
    format!("\x1b[38;2;{r};{g};{b}m")
}

fn bg(r: u8, g: u8, b: u8) -> String {
    format!("\x1b[48;2;{r};{g};{b}m")
}

fn fgc(c: Color) -> String {
    fg(c.0, c.1, c.2)
}

fn bgc(c: Color) -> String {
    bg(c.0, c.1, c.2)
}

fn display_width(s: &str) -> usize {
    s.chars().count()
}

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const ELAPSED_THRESHOLD: u64 = 30;
const SEPARATOR: &str = "\u{e0b0}";

/// Write a powerline arrow: fg=from_bg, bg=to_bg, then separator char.
fn arrow(buf: &mut String, col: &mut usize, from: Color, to: Color) {
    let _ = write!(buf, "{}{}{SEPARATOR}", fgc(from), bgc(to));
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

/// The pill label for an input mode. Its color comes from the palette (see `mode_color`).
fn mode_label(mode: InputMode) -> &'static str {
    match mode {
        InputMode::Normal => "NORMAL",
        InputMode::Locked => "LOCKED",
        InputMode::Pane => "PANE",
        InputMode::Tab => "TAB",
        InputMode::Resize => "RESIZE",
        InputMode::Move => "MOVE",
        InputMode::Scroll => "SCROLL",
        InputMode::EnterSearch => "SEARCH",
        InputMode::Search => "SEARCH",
        InputMode::RenameTab => "RENAME",
        InputMode::RenamePane => "RENAME",
        InputMode::Session => "SESSION",
        InputMode::Prompt => "PROMPT",
        InputMode::Tmux => "TMUX",
    }
}

/// The palette color for an input-mode pill background.
fn mode_color(p: &Palette, mode: InputMode) -> Color {
    match mode {
        InputMode::Normal | InputMode::Prompt | InputMode::Tmux => p.success,
        InputMode::Locked => p.waiting,
        InputMode::Pane => p.accent_blue,
        InputMode::Tab | InputMode::Session => p.thinking,
        InputMode::Resize | InputMode::Move => p.tool,
        InputMode::Scroll
        | InputMode::EnterSearch
        | InputMode::Search
        | InputMode::RenameTab
        | InputMode::RenamePane => p.notification,
    }
}

pub fn render_status_bar(state: &mut State, _rows: usize, cols: usize) {
    state.click_regions.clear();
    state.menu_click_regions.clear();

    let pal = state.palette;

    let mut buf = String::with_capacity(cols * 4);
    // Terminal setup for a 1-row status bar:
    //  \x1b[H     — cursor home (prevent scroll from cursor at end-of-line)
    //  \x1b[?7l   — disable auto-wrap (clip overflow instead of scroll)
    //  \x1b[?25l  — hide cursor
    buf.push_str("\x1b[H\x1b[?7l\x1b[?25l");
    let bar_bg_str = bgc(pal.bar_bg);

    // Bail early if terminal is too narrow
    if cols < 5 {
        let _ = write!(buf, "{bar_bg_str}{:width$}{RESET}", "", width = cols);
        print!("{buf}");
        let _ = std::io::stdout().flush();
        return;
    }

    let prefix_bg = if state.view_mode == ViewMode::Settings {
        pal.prefix_bg_active
    } else {
        pal.prefix_bg
    };

    // Build prefix: " Zellaude (session) MODE "
    let mode_text = mode_label(state.input_mode);
    let mode_bg = mode_color(&pal, state.input_mode);
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
            bgc(prefix_bg),
            fgc(pal.text),
        );
        if show_mode {
            let _ = write!(
                buf,
                "{}{}{BOLD} {mode_text} {RESET}",
                bgc(mode_bg),
                fgc(pal.bar_bg),
            );
        }
        col = total_prefix_width;
    } else if prefix_width <= cols {
        // Fit the name part but skip mode pill
        let _ = write!(
            buf,
            "{}{}{BOLD}{prefix_text}{RESET}",
            bgc(prefix_bg),
            fgc(pal.text),
        );
        col = prefix_width;
    } else {
        // Even name doesn't fit — just show what we can
        let avail = cols.saturating_sub(2); // leave room for fill
        let short: String = prefix_text.chars().take(avail).collect();
        let _ = write!(
            buf,
            "{}{}{BOLD}{short}{RESET}",
            bgc(prefix_bg),
            fgc(pal.text),
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
            }
            ViewMode::Settings => {
                arrow(&mut buf, &mut col, last_prefix_bg, pal.bar_bg);
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
    let pal = state.palette;
    let now_s = unix_now();
    let now_ms = unix_now_ms();

    // Sort tabs by position
    let mut tabs: Vec<&TabInfo> = state.tabs.iter().collect();
    tabs.sort_by_key(|t| t.position);

    let count = tabs.len();
    if count == 0 {
        arrow(buf, col, prefix_bg, pal.bar_bg);
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
            pal.flash_bg
        } else if is_active {
            pal.tab_active_bg
        } else {
            pal.tab_inactive_bg
        };

        // Arrow: close previous segment, then open this tab
        if prev_bg == prefix_bg {
            arrow(buf, col, prev_bg, tab_bg);
        } else {
            arrow(buf, col, prev_bg, pal.bar_bg);
            arrow(buf, col, pal.bar_bg, tab_bg);
        }

        let tab_bg_str = bgc(tab_bg);
        let region_start = *col;

        if is_claude {
            let s = session.unwrap();
            let symbol = activity_symbol(&s.activity);
            let sym_color = activity_color(&pal, &s.activity);

            let (sym_fg, name_fg, name_bold) = if is_flash_bright {
                (fgc(pal.flash_text), fgc(pal.flash_text), true)
            } else if is_active {
                (fgc(sym_color), fgc(pal.text), true)
            } else {
                (fgc(sym_color), fgc(pal.text_dim), false)
            };

            // Leading space
            let _ = write!(buf, "{tab_bg_str} ");
            *col += 1;

            // Symbol
            let _ = write!(buf, "{sym_fg}{}", symbol);
            *col += display_width(symbol);

            // Space + name
            if !truncated.is_empty() {
                let bold_str = if name_bold { BOLD } else { "" };
                let _ = write!(buf, " {bold_str}{name_fg}{truncated}{RESET}{tab_bg_str}");
                *col += 1 + display_width(&truncated);
            }

            // Elapsed suffix
            if let Some(ref es) = elapsed_strs[i] {
                if *col + 1 + es.len() + 1 < cols {
                    let _ = write!(buf, " {}{es}", fgc(pal.elapsed));
                    *col += 1 + es.len();
                }
            }

            // Fullscreen indicator
            if tab.is_fullscreen_active && *col + 3 < cols {
                let _ = write!(buf, " {}F{RESET}{tab_bg_str}", fgc(pal.fullscreen));
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
                fgc(pal.text)
            } else {
                fgc(pal.text_muted)
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
                let _ = write!(buf, " {}F{RESET}{tab_bg_str}", fgc(pal.fullscreen));
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
        arrow(buf, col, prev_bg, pal.bar_bg);
    }
}

fn notify_mode_label(
    p: &Palette,
    mode: NotifyMode,
) -> (&'static str, &'static str, String, String) {
    match mode {
        NotifyMode::Always => ("●", "Notify: always", fgc(p.success), fgc(p.text)),
        NotifyMode::Unfocused => (
            "◐",
            "Notify: unfocused",
            fgc(p.fullscreen),
            fgc(p.fullscreen),
        ),
        NotifyMode::Never => ("○", "Notify: off", fgc(p.disabled), fgc(p.disabled)),
    }
}

fn flash_mode_label(p: &Palette, mode: FlashMode) -> (&'static str, &'static str, String, String) {
    match mode {
        FlashMode::Persist => ("●", "Flash: persist", fgc(p.success), fgc(p.text)),
        FlashMode::Once => ("◐", "Flash: brief", fgc(p.fullscreen), fgc(p.fullscreen)),
        FlashMode::Off => ("○", "Flash: off", fgc(p.disabled), fgc(p.disabled)),
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
    let pal = state.palette;

    // Leading space after arrow
    let _ = write!(buf, " ");
    *col += 1;

    // --- Notifications (three-state) ---
    {
        let (symbol, label, sym_color, label_color) =
            notify_mode_label(&pal, state.settings.notifications);
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
        let (symbol, label, sym_color, label_color) = flash_mode_label(&pal, state.settings.flash);
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
            ("●", fgc(pal.success), fgc(pal.text))
        } else {
            ("○", fgc(pal.disabled), fgc(pal.disabled))
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
            ("●", fgc(pal.success), fgc(pal.text))
        } else {
            ("○", fgc(pal.disabled), fgc(pal.disabled))
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

    // Close button
    let _ = write!(buf, "  ");
    *col += 2;
    let close_start = *col;
    let _ = write!(buf, "{}×", fgc(pal.waiting));
    *col += 1;

    state.menu_click_regions.push(MenuClickRegion {
        start_col: close_start,
        end_col: *col,
        action: MenuAction::CloseMenu,
    });
}
