//! Color palette for the Zellaude status bar.
//!
//! A `Palette` holds one [`Color`] per semantic role. The defaults reproduce
//! Zellaude's original hardcoded colors; see
//! `docs/superpowers/specs/2026-06-09-color-palettes-design.md` for which UI
//! elements each role drives. The effective palette is resolved in three
//! layers: built-in defaults, an optional Zellij-theme overlay, then explicit
//! per-role overrides.

use std::collections::BTreeMap;
use zellij_tile::prelude::{PaletteColor, Styling};

/// An RGB color, matching the `(r, g, b)` tuples used throughout rendering.
pub type Color = (u8, u8, u8);

/// The resolved set of colors the status bar renders with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    // Status hues — activity glyphs and the matching mode pills.
    pub thinking: Color,
    pub tool: Color,
    pub waiting: Color,
    pub success: Color,
    pub notification: Color,
    pub accent_blue: Color,
    pub neutral: Color,
    // Surfaces.
    pub bar_bg: Color,
    pub prefix_bg: Color,
    pub prefix_bg_active: Color,
    pub tab_active_bg: Color,
    pub tab_inactive_bg: Color,
    pub flash_bg: Color,
    // Text and detail.
    pub text: Color,
    pub text_dim: Color,
    pub text_muted: Color,
    pub disabled: Color,
    pub elapsed: Color,
    pub flash_text: Color,
    pub fullscreen: Color,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            thinking: (180, 140, 255),
            tool: (255, 170, 50),
            waiting: (255, 60, 60),
            success: (80, 200, 120),
            notification: (200, 200, 100),
            accent_blue: (80, 180, 255),
            neutral: (180, 175, 195),
            bar_bg: (30, 30, 46),
            prefix_bg: (60, 50, 80),
            prefix_bg_active: (100, 70, 140),
            tab_active_bg: (140, 100, 200),
            tab_inactive_bg: (80, 75, 110),
            flash_bg: (80, 80, 30),
            text: (255, 255, 255),
            text_dim: (120, 220, 220),
            text_muted: (170, 165, 185),
            disabled: (100, 100, 100),
            elapsed: (165, 160, 180),
            flash_text: (255, 255, 80),
            fullscreen: (255, 200, 60),
        }
    }
}

/// Parse a color string. Accepts `#rrggbb`, `#rgb` (shorthand), and `r,g,b` or
/// `r g b` triplets (components `0..=255`). Returns `None` on any other input.
pub fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        return parse_hex(hex);
    }
    let parts: Vec<&str> = if s.contains(',') {
        s.split(',').collect()
    } else {
        s.split_whitespace().collect()
    };
    if parts.len() != 3 {
        return None;
    }
    let r = parts[0].trim().parse::<u8>().ok()?;
    let g = parts[1].trim().parse::<u8>().ok()?;
    let b = parts[2].trim().parse::<u8>().ok()?;
    Some((r, g, b))
}

fn parse_hex(hex: &str) -> Option<Color> {
    match hex.len() {
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some((r, g, b))
        }
        3 => {
            // Expand each nibble: "f" -> 0xff, "a" -> 0xaa.
            let expand = |c: &str| u8::from_str_radix(c, 16).map(|v| v * 17).ok();
            let r = expand(&hex[0..1])?;
            let g = expand(&hex[1..2])?;
            let b = expand(&hex[2..3])?;
            Some((r, g, b))
        }
        _ => None,
    }
}

/// Standard xterm base-16 ANSI colors (the terminal's real values aren't
/// knowable from a plugin, so a conventional table is used).
const ANSI16: [Color; 16] = [
    (0, 0, 0),
    (128, 0, 0),
    (0, 128, 0),
    (128, 128, 0),
    (0, 0, 128),
    (128, 0, 128),
    (0, 128, 128),
    (192, 192, 192),
    (128, 128, 128),
    (255, 0, 0),
    (0, 255, 0),
    (255, 255, 0),
    (0, 0, 255),
    (255, 0, 255),
    (0, 255, 255),
    (255, 255, 255),
];

/// Convert an xterm-256 palette index to RGB.
fn eightbit_to_rgb(idx: u8) -> Color {
    match idx {
        0..=15 => ANSI16[idx as usize],
        16..=231 => {
            let i = idx - 16;
            let r = i / 36;
            let g = (i % 36) / 6;
            let b = i % 6;
            let scale = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            (scale(r), scale(g), scale(b))
        }
        232..=255 => {
            let level = 8 + (idx - 232) * 10;
            (level, level, level)
        }
    }
}

/// Resolve a Zellij `PaletteColor` to RGB.
fn palette_color_to_rgb(c: PaletteColor) -> Color {
    match c {
        PaletteColor::Rgb((r, g, b)) => (r, g, b),
        PaletteColor::EightBit(idx) => eightbit_to_rgb(idx),
    }
}

/// Apply per-role overrides on top of an existing palette.
pub fn apply_overrides(palette: &mut Palette, overrides: &[(PaletteRole, Color)]) {
    for &(role, color) in overrides {
        palette.set(role, color);
    }
}

/// Overlay the roles that can be derived from the active Zellij theme.
///
/// Only surfaces, text shades, and the two semantic exit-code colors are
/// mapped. Decorative status hues are left untouched so red still means
/// "waiting" and green still means "done" on any theme.
pub fn apply_theme(palette: &mut Palette, styling: &Styling) {
    let text = &styling.text_unselected;
    palette.bar_bg = palette_color_to_rgb(text.background);
    palette.text = palette_color_to_rgb(text.base);
    palette.text_dim = palette_color_to_rgb(text.emphasis_0);
    palette.text_muted = palette_color_to_rgb(text.emphasis_2);
    palette.disabled = palette_color_to_rgb(text.emphasis_3);
    palette.elapsed = palette_color_to_rgb(text.emphasis_2);
    palette.tab_active_bg = palette_color_to_rgb(styling.ribbon_selected.background);
    palette.tab_inactive_bg = palette_color_to_rgb(styling.ribbon_unselected.background);
    palette.prefix_bg = palette_color_to_rgb(styling.ribbon_unselected.background);
    palette.prefix_bg_active = palette_color_to_rgb(styling.ribbon_selected.background);
    palette.success = palette_color_to_rgb(styling.exit_code_success.base);
    palette.waiting = palette_color_to_rgb(styling.exit_code_error.base);
}

/// A single overridable color role, identified in config by its snake_case key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteRole {
    Thinking,
    Tool,
    Waiting,
    Success,
    Notification,
    AccentBlue,
    Neutral,
    BarBg,
    PrefixBg,
    PrefixBgActive,
    TabActiveBg,
    TabInactiveBg,
    FlashBg,
    Text,
    TextDim,
    TextMuted,
    Disabled,
    Elapsed,
    FlashText,
    Fullscreen,
}

impl PaletteRole {
    /// Map a config key to its role, or `None` if it is not a palette role.
    pub fn from_key(key: &str) -> Option<Self> {
        Some(match key {
            "thinking" => Self::Thinking,
            "tool" => Self::Tool,
            "waiting" => Self::Waiting,
            "success" => Self::Success,
            "notification" => Self::Notification,
            "accent_blue" => Self::AccentBlue,
            "neutral" => Self::Neutral,
            "bar_bg" => Self::BarBg,
            "prefix_bg" => Self::PrefixBg,
            "prefix_bg_active" => Self::PrefixBgActive,
            "tab_active_bg" => Self::TabActiveBg,
            "tab_inactive_bg" => Self::TabInactiveBg,
            "flash_bg" => Self::FlashBg,
            "text" => Self::Text,
            "text_dim" => Self::TextDim,
            "text_muted" => Self::TextMuted,
            "disabled" => Self::Disabled,
            "elapsed" => Self::Elapsed,
            "flash_text" => Self::FlashText,
            "fullscreen" => Self::Fullscreen,
            _ => return None,
        })
    }
}

impl Palette {
    /// Set the color for a single role.
    pub fn set(&mut self, role: PaletteRole, c: Color) {
        use PaletteRole::*;
        match role {
            Thinking => self.thinking = c,
            Tool => self.tool = c,
            Waiting => self.waiting = c,
            Success => self.success = c,
            Notification => self.notification = c,
            AccentBlue => self.accent_blue = c,
            Neutral => self.neutral = c,
            BarBg => self.bar_bg = c,
            PrefixBg => self.prefix_bg = c,
            PrefixBgActive => self.prefix_bg_active = c,
            TabActiveBg => self.tab_active_bg = c,
            TabInactiveBg => self.tab_inactive_bg = c,
            FlashBg => self.flash_bg = c,
            Text => self.text = c,
            TextDim => self.text_dim = c,
            TextMuted => self.text_muted = c,
            Disabled => self.disabled = c,
            Elapsed => self.elapsed = c,
            FlashText => self.flash_text = c,
            Fullscreen => self.fullscreen = c,
        }
    }
}

/// Where the base palette comes from before overrides are applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeSource {
    /// Zellaude's built-in default colors.
    #[default]
    Builtin,
    /// Derive surfaces/text/exit-code colors from the active Zellij theme.
    Zellij,
}

/// Parse the plugin's KDL configuration into a theme source and a list of
/// per-role color overrides.
///
/// Unknown keys are ignored (Zellij passes the entire `plugin {}` block).
/// Known roles with an unparseable color are skipped with a diagnostic so the
/// bar always renders.
pub fn parse_config(config: &BTreeMap<String, String>) -> (ThemeSource, Vec<(PaletteRole, Color)>) {
    let mut theme_source = ThemeSource::Builtin;
    let mut overrides = Vec::new();

    for (key, value) in config {
        if key == "theme_source" {
            theme_source = match value.trim() {
                "zellij" => ThemeSource::Zellij,
                "builtin" => ThemeSource::Builtin,
                other => {
                    eprintln!("zellaude: unknown theme_source {other:?}, using \"builtin\"");
                    ThemeSource::Builtin
                }
            };
            continue;
        }
        if let Some(role) = PaletteRole::from_key(key) {
            match parse_color(value) {
                Some(color) => overrides.push((role, color)),
                None => eprintln!(
                    "zellaude: invalid color for {key:?}: {value:?} (keeping default)"
                ),
            }
        }
    }

    (theme_source, overrides)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use zellij_tile::prelude::{PaletteColor, StyleDeclaration, Styling};

    #[test]
    fn parse_config_reads_theme_overrides_and_ignores_unknown() {
        let mut config = BTreeMap::new();
        config.insert("theme_source".to_string(), "zellij".to_string());
        config.insert("thinking".to_string(), "#010203".to_string());
        config.insert("waiting".to_string(), "bad-color".to_string()); // skipped
        config.insert("not_a_role".to_string(), "whatever".to_string()); // ignored

        let (source, overrides) = parse_config(&config);
        assert_eq!(source, ThemeSource::Zellij);
        assert!(overrides.contains(&(PaletteRole::Thinking, (1, 2, 3))));
        assert!(!overrides.iter().any(|(r, _)| *r == PaletteRole::Waiting));
        assert_eq!(overrides.len(), 1);
    }

    #[test]
    fn parse_config_defaults_to_builtin_theme() {
        let (source, overrides) = parse_config(&BTreeMap::new());
        assert_eq!(source, ThemeSource::Builtin);
        assert!(overrides.is_empty());
    }

    fn pc(r: u8, g: u8, b: u8) -> PaletteColor {
        PaletteColor::Rgb((r, g, b))
    }

    #[test]
    fn overrides_apply_in_order() {
        let mut p = Palette::default();
        apply_overrides(
            &mut p,
            &[(PaletteRole::Thinking, (1, 2, 3)), (PaletteRole::BarBg, (4, 5, 6))],
        );
        assert_eq!(p.thinking, (1, 2, 3));
        assert_eq!(p.bar_bg, (4, 5, 6));
        assert_eq!(p.tool, Palette::default().tool);
    }

    #[test]
    fn theme_maps_surfaces_text_and_exit_codes_only() {
        let styling = Styling {
            text_unselected: StyleDeclaration {
                base: pc(1, 1, 1),
                background: pc(2, 2, 2),
                emphasis_0: pc(3, 3, 3),
                emphasis_1: pc(4, 4, 4),
                emphasis_2: pc(5, 5, 5),
                emphasis_3: pc(6, 6, 6),
            },
            ribbon_selected: StyleDeclaration { background: pc(7, 7, 7), ..Default::default() },
            ribbon_unselected: StyleDeclaration { background: pc(8, 8, 8), ..Default::default() },
            exit_code_success: StyleDeclaration { base: pc(9, 9, 9), ..Default::default() },
            exit_code_error: StyleDeclaration { base: pc(10, 10, 10), ..Default::default() },
            ..Default::default()
        };
        let mut p = Palette::default();
        apply_theme(&mut p, &styling);

        assert_eq!(p.bar_bg, (2, 2, 2));
        assert_eq!(p.text, (1, 1, 1));
        assert_eq!(p.text_dim, (3, 3, 3));
        assert_eq!(p.text_muted, (5, 5, 5));
        assert_eq!(p.disabled, (6, 6, 6));
        assert_eq!(p.elapsed, (5, 5, 5));
        assert_eq!(p.tab_active_bg, (7, 7, 7));
        assert_eq!(p.tab_inactive_bg, (8, 8, 8));
        assert_eq!(p.prefix_bg, (8, 8, 8));
        assert_eq!(p.prefix_bg_active, (7, 7, 7));
        assert_eq!(p.success, (9, 9, 9));
        assert_eq!(p.waiting, (10, 10, 10));

        // Decorative status hues are untouched by the theme.
        assert_eq!(p.thinking, Palette::default().thinking);
        assert_eq!(p.tool, Palette::default().tool);
        assert_eq!(p.accent_blue, Palette::default().accent_blue);
    }

    #[test]
    fn eightbit_cube_and_grayscale() {
        assert_eq!(eightbit_to_rgb(16), (0, 0, 0));
        assert_eq!(eightbit_to_rgb(231), (255, 255, 255));
        assert_eq!(eightbit_to_rgb(196), (255, 0, 0));
        assert_eq!(eightbit_to_rgb(232), (8, 8, 8));
        assert_eq!(eightbit_to_rgb(255), (238, 238, 238));
        assert_eq!(eightbit_to_rgb(9), (255, 0, 0)); // ANSI bright red
    }

    #[test]
    fn parse_hex_long_and_short() {
        assert_eq!(parse_color("#b48cff"), Some((180, 140, 255)));
        assert_eq!(parse_color("#B48CFF"), Some((180, 140, 255)));
        assert_eq!(parse_color("#fff"), Some((255, 255, 255)));
        assert_eq!(parse_color("#0a0"), Some((0, 170, 0)));
    }

    #[test]
    fn parse_triplet_comma_and_space() {
        assert_eq!(parse_color("180,140,255"), Some((180, 140, 255)));
        assert_eq!(parse_color("180, 140, 255"), Some((180, 140, 255)));
        assert_eq!(parse_color("80 180 255"), Some((80, 180, 255)));
        assert_eq!(parse_color("  255,60,60  "), Some((255, 60, 60)));
    }

    #[test]
    fn parse_color_rejects_invalid() {
        assert_eq!(parse_color("#12"), None);
        assert_eq!(parse_color("#zzzzzz"), None);
        assert_eq!(parse_color("256,0,0"), None);
        assert_eq!(parse_color("1,2"), None);
        assert_eq!(parse_color("1,2,3,4"), None);
        assert_eq!(parse_color("b48cff"), None); // hex requires '#'
        assert_eq!(parse_color("garbage"), None);
    }

    #[test]
    fn role_from_key_known_and_unknown() {
        assert_eq!(PaletteRole::from_key("thinking"), Some(PaletteRole::Thinking));
        assert_eq!(PaletteRole::from_key("tab_active_bg"), Some(PaletteRole::TabActiveBg));
        assert_eq!(PaletteRole::from_key("fullscreen"), Some(PaletteRole::Fullscreen));
        assert_eq!(PaletteRole::from_key("nope"), None);
        assert_eq!(PaletteRole::from_key("theme_source"), None);
    }

    #[test]
    fn set_changes_only_the_named_role() {
        let mut p = Palette::default();
        p.set(PaletteRole::Thinking, (1, 2, 3));
        assert_eq!(p.thinking, (1, 2, 3));
        assert_eq!(p.tool, Palette::default().tool);
    }

    #[test]
    fn default_palette_matches_original_values() {
        // Exhaustive struct-literal compare: a typo in any default, or a new
        // field added without a default, fails this test (or fails to compile).
        // These values must reproduce Zellaude's original hardcoded palette.
        assert_eq!(
            Palette::default(),
            Palette {
                thinking: (180, 140, 255),
                tool: (255, 170, 50),
                waiting: (255, 60, 60),
                success: (80, 200, 120),
                notification: (200, 200, 100),
                accent_blue: (80, 180, 255),
                neutral: (180, 175, 195),
                bar_bg: (30, 30, 46),
                prefix_bg: (60, 50, 80),
                prefix_bg_active: (100, 70, 140),
                tab_active_bg: (140, 100, 200),
                tab_inactive_bg: (80, 75, 110),
                flash_bg: (80, 80, 30),
                text: (255, 255, 255),
                text_dim: (120, 220, 220),
                text_muted: (170, 165, 185),
                disabled: (100, 100, 100),
                elapsed: (165, 160, 180),
                flash_text: (255, 255, 80),
                fullscreen: (255, 200, 60),
            }
        );
    }
}
