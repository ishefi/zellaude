use crate::state::{
    unix_now_ms, Activity, ClickAction, ClickRegion, FlashMode, MenuAction, MenuClickRegion,
    NotifyMode, SessionInfo, SettingKey, State, ViewMode,
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

fn activity_style(activity: &Activity) -> Style {
    match activity {
        Activity::Init => Style { symbol: "◆", r: 180, g: 175, b: 195 },
        Activity::Thinking => Style { symbol: "●", r: 180, g: 140, b: 255 },
        Activity::Tool(name) => {
            let symbol = match name.as_str() {
                "Bash" => "⚡",
                "Read" | "Glob" | "Grep" => "◉",
                "Edit" | "Write" => "✎",
                "Task" => "⊜",
                "WebSearch" | "WebFetch" => "◈",
                _ => "⚙",
            };
            Style { symbol, r: 255, g: 170, b: 50 }
        }
        Activity::Prompting => Style { symbol: "▶", r: 80, g: 200, b: 120 },
        Activity::Waiting => Style { symbol: "⚠", r: 255, g: 60, b: 60 },
        Activity::Notification => Style { symbol: "◇", r: 200, g: 200, b: 100 },
        Activity::Done => Style { symbol: "✓", r: 80, g: 200, b: 120 },
        Activity::AgentDone => Style { symbol: "✓", r: 80, g: 180, b: 100 },
        Activity::Idle => Style { symbol: "○", r: 180, g: 175, b: 195 },
    }
}

fn fg(r: u8, g: u8, b: u8) -> String {
    format!("\x1b[38;2;{r};{g};{b}m")
}

fn bg(r: u8, g: u8, b: u8) -> String {
    format!("\x1b[48;2;{r};{g};{b}m")
}

fn display_width(s: &str) -> usize {
    s.chars()
        .filter(|&c| c != '\u{FE0E}' && c != '\u{FE0F}')
        .count()
}

const SYMBOL_CELL: usize = 2;

fn symbol_visual_cols(s: &str) -> usize {
    s.chars()
        .map(|c| match c {
            '\u{FE0E}' | '\u{FE0F}' => 0,
            '⚡' | '⚙' | '▶' | '⚠' => 2,
            _ => 1,
        })
        .sum()
}

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const SEPARATOR: &str = "\u{e0b0}";

type Color = (u8, u8, u8);
const BAR_BG: Color = (30, 30, 46);
const PREFIX_BG: Color = (60, 50, 80);
const PREFIX_BG_SETTINGS: Color = (100, 70, 140);
const TAB_BG_ACTIVE: Color = (140, 100, 200);
const TAB_BG_INACTIVE: Color = (80, 75, 110);
const FLASH_FG_BRIGHT: Color = (255, 255, 80);

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

    let mut buf = String::with_capacity(cols * 4);
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
    let mode_pill_width = if show_mode { 1 + mode_text.len() + 1 } else { 0 };
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

    let last_prefix_bg = if show_mode && total_prefix_width <= cols { mode_bg } else { prefix_bg };
    let prefix_used = col;

    if col < cols {
        match state.view_mode {
            ViewMode::Normal => {
                render_tabs(state, &mut buf, &mut col, cols, last_prefix_bg, prefix_used);
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
    let now_ms = unix_now_ms();

    let mut tabs: Vec<&TabInfo> = state.tabs.iter().collect();
    tabs.sort_by_key(|t| t.position);

    let count = tabs.len();
    if count == 0 {
        arrow(buf, col, prefix_bg, BAR_BG);
        return;
    }

    let tab_sessions: Vec<Vec<&SessionInfo>> = tabs
        .iter()
        .map(|tab| {
            let mut sessions: Vec<&SessionInfo> = state
                .sessions
                .values()
                .filter(|s| s.tab_index == Some(tab.position))
                .collect();
            sessions.sort_by_key(|s| s.pane_id);
            sessions
        })
        .collect();

    let per_tab_fixed: Vec<usize> = tab_sessions
        .iter()
        .map(|sessions| {
            if sessions.is_empty() {
                2
            } else {
                let n = sessions.len();
                SYMBOL_CELL * n + (n - 1) + 2
            }
        })
        .collect();

    let fixed_total: usize =
        prefix_width + 2 * count + per_tab_fixed.iter().sum::<usize>();
    let remaining = cols.saturating_sub(fixed_total);
    let name_budget = (remaining / count).min(21);
    let max_name_len = if name_budget >= 2 { name_budget - 1 } else { 0 };

    let mut prev_bg = prefix_bg;

    for (i, tab) in tabs.iter().enumerate() {
        let arrows_needed = if prev_bg == prefix_bg { 1 } else { 2 };
        if *col + arrows_needed + per_tab_fixed[i] + 1 > cols {
            break;
        }

        let sessions = &tab_sessions[i];
        let is_claude = !sessions.is_empty();
        let tab_name = &tab.name;

        let char_count = tab_name.chars().count();
        let truncated = if max_name_len == 0 {
            String::new()
        } else if char_count > max_name_len {
            let s: String = tab_name.chars().take(max_name_len.saturating_sub(1)).collect();
            format!("{s}…")
        } else {
            tab_name.to_string()
        };

        let is_active = tab.active;
        let tab_bg = if is_active { TAB_BG_ACTIVE } else { TAB_BG_INACTIVE };

        if prev_bg == prefix_bg {
            arrow(buf, col, prev_bg, tab_bg);
        } else {
            arrow(buf, col, prev_bg, BAR_BG);
            arrow(buf, col, BAR_BG, tab_bg);
        }

        let tab_bg_str = bg(tab_bg.0, tab_bg.1, tab_bg.2);

        if is_claude {
            let _ = write!(buf, "{tab_bg_str} ");
            *col += 1;

            for (sym_idx, session) in sessions.iter().enumerate() {
                let style = activity_style(&session.activity);

                let is_flashing = state
                    .flash_deadlines
                    .get(&session.pane_id)
                    .map(|&deadline| now_ms < deadline && (now_ms / 250) % 2 == 0)
                    .unwrap_or(false);

                let sym_fg = if is_flashing {
                    fg(FLASH_FG_BRIGHT.0, FLASH_FG_BRIGHT.1, FLASH_FG_BRIGHT.2)
                } else {
                    fg(style.r, style.g, style.b)
                };

                let region_start = *col;
                let visual = symbol_visual_cols(style.symbol);
                let pad = SYMBOL_CELL.saturating_sub(visual);
                let _ = write!(buf, "{tab_bg_str}{sym_fg}{}", style.symbol);
                for _ in 0..pad {
                    let _ = write!(buf, " ");
                }
                *col += SYMBOL_CELL;

                state.click_regions.push(ClickRegion {
                    start_col: region_start,
                    end_col: *col,
                    action: ClickAction::FocusPane(session.pane_id),
                });

                if sym_idx + 1 < sessions.len() {
                    let _ = write!(buf, "{tab_bg_str} ");
                    *col += 1;
                }
            }

            let name_start = *col;

            if !truncated.is_empty() {
                let needed = 1 + display_width(&truncated);
                if cols.saturating_sub(*col) >= needed + 2 {
                    let (name_fg_str, name_bold) = if is_active {
                        (fg(255, 255, 255), true)
                    } else {
                        (fg(120, 220, 220), false)
                    };
                    let bold_str = if name_bold { BOLD } else { "" };
                    let _ = write!(
                        buf,
                        "{tab_bg_str} {bold_str}{name_fg_str}{truncated}{RESET}{tab_bg_str}"
                    );
                    *col += needed;
                }
            }

            if tab.is_fullscreen_active && *col + 3 < cols {
                let _ = write!(buf, " {}F{RESET}{tab_bg_str}", fg(255, 200, 60));
                *col += 2;
            }

            let _ = write!(buf, " ");
            *col += 1;

            if *col > name_start {
                state.click_regions.push(ClickRegion {
                    start_col: name_start,
                    end_col: *col,
                    action: ClickAction::SwitchTab(tab.position),
                });
            }
        } else {
            let region_start = *col;

            let name_fg_str = if is_active {
                fg(220, 215, 230)
            } else {
                fg(170, 165, 185)
            };
            let name_bold = is_active;

            let _ = write!(buf, "{tab_bg_str} ");
            *col += 1;

            if !truncated.is_empty() {
                let needed = display_width(&truncated);
                if cols.saturating_sub(*col) >= needed + 2 {
                    let bold_str = if name_bold { BOLD } else { "" };
                    let _ = write!(buf, "{bold_str}{name_fg_str}{truncated}{RESET}{tab_bg_str}");
                    *col += needed;
                }
            }

            if tab.is_fullscreen_active && *col + 3 < cols {
                let _ = write!(buf, " {}F{RESET}{tab_bg_str}", fg(255, 200, 60));
                *col += 2;
            }

            let _ = write!(buf, " ");
            *col += 1;

            state.click_regions.push(ClickRegion {
                start_col: region_start,
                end_col: *col,
                action: ClickAction::SwitchTab(tab.position),
            });
        }

        prev_bg = tab_bg;
    }

    if prev_bg != prefix_bg || count > 0 {
        arrow(buf, col, prev_bg, BAR_BG);
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
            buf, col, &mut state.menu_click_regions,
            SettingKey::Notifications, symbol, label, &sym_color, &label_color,
        );
    }

    // --- Flash (three-state) ---
    {
        let _ = write!(buf, "  ");
        *col += 2;
        let (symbol, label, sym_color, label_color) =
            flash_mode_label(state.settings.flash);
        render_tristate(
            buf, col, &mut state.menu_click_regions,
            SettingKey::Flash, symbol, label, &sym_color, &label_color,
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
        let label = if enabled { "Mode indicator: on" } else { "Mode indicator: off" };
        render_tristate(
            buf, col, &mut state.menu_click_regions,
            SettingKey::ModeIndicator, symbol, label, &sym_color, &label_color,
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
