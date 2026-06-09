//! Color palette for the Zellaude status bar.
//!
//! A `Palette` holds one [`Color`] per semantic role. The defaults reproduce
//! Zellaude's original hardcoded colors; see
//! `docs/superpowers/specs/2026-06-09-color-palettes-design.md` for which UI
//! elements each role drives. The effective palette is resolved in three
//! layers: built-in defaults, an optional Zellij-theme overlay, then explicit
//! per-role overrides.

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

#[cfg(test)]
mod tests {
    use super::*;

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
