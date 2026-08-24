//! Every color the UI draws, keyed by the *role* it plays rather than
//! hardcoded at the call site (DESIGN.md §6).
//!
//! The presets are the four [Catppuccin](https://catppuccin.com) flavors,
//! in truecolor. Indexed-palette approximations were tried first and are
//! the reason the UI read as a 1980s terminal: ANSI cyan/green/red are
//! fully saturated, and no amount of layout fixes a palette that shouts.
//! A terminal without truecolor degrades to its nearest 256 match, which is
//! muddier but still legible.

use ratatui::style::Color;

const fn rgb(hex: u32) -> Color {
    Color::Rgb(
        ((hex >> 16) & 0xff) as u8,
        ((hex >> 8) & 0xff) as u8,
        (hex & 0xff) as u8,
    )
}

/// Preset names, in the order a settings toggle cycles them: darkest first,
/// light last. `by_name` matches case-insensitively and falls back to the
/// first.
pub const THEMES: &[&str] = &["mocha", "macchiato", "frappe", "latte"];

/// Semantic color roles for the whole client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    /// The page behind the panels. Deliberately darker than `surface` so a
    /// panel reads as a card sitting on it rather than a box drawn in it.
    pub bg: Color,
    /// An unfocused panel's fill.
    pub surface: Color,
    /// A focused panel's fill: one step nearer the viewer than `surface`.
    pub surface_focus: Color,
    /// Focus borders, focused titles, the selection marker, the cursor.
    pub accent: Color,
    /// Text drawn on top of an `accent` fill.
    pub on_accent: Color,
    /// Primary text.
    pub text: Color,
    /// Secondary text: unfocused titles, paths, counts.
    pub muted: Color,
    /// Hints, dividers, placeholder text, idle glyphs.
    pub dim: Color,
    /// Idle-but-healthy, a clean exit, "clean" in git terms.
    pub ok: Color,
    /// Working — an agent mid-turn — and uncommitted changes.
    pub warn: Color,
    /// Needs you, and destructive confirmations.
    pub err: Color,
    /// Structural chrome: borders of unfocused panels.
    pub edge: Color,
    /// Selected-row fill in the focused column.
    pub sel_bg: Color,
    /// Selected-row fill in unfocused columns; barely raised, just enough
    /// to remember where you were.
    pub sel_bg_dim: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Theme::mocha()
    }
}

impl Theme {
    /// The preset named by `ARGUS_THEME`, or the default.
    pub fn from_env() -> Self {
        let Ok(name) = std::env::var("ARGUS_THEME") else {
            return Theme::default();
        };
        let known = THEMES.iter().any(|t| t.eq_ignore_ascii_case(name.trim()));
        if !known {
            // A typo should be inert, not a silently different palette.
            return Theme::default();
        }
        Theme::by_name(&name)
    }

    pub fn by_name(name: &str) -> Self {
        match name.trim().to_ascii_lowercase().as_str() {
            "macchiato" => Theme::macchiato(),
            "frappe" => Theme::frappe(),
            "latte" => Theme::latte(),
            _ => Theme::mocha(),
        }
    }

    /// The name this theme was built from, for the settings display.
    pub fn name(&self) -> &'static str {
        THEMES
            .iter()
            .copied()
            .find(|n| Theme::by_name(n) == *self)
            .unwrap_or("mocha")
    }

    pub fn mocha() -> Self {
        Theme {
            bg: rgb(0x11111b),          // crust
            surface: rgb(0x181825),     // mantle
            surface_focus: rgb(0x1e1e2e), // base
            accent: rgb(0xcba6f7),      // mauve
            on_accent: rgb(0x11111b),
            text: rgb(0xcdd6f4),
            muted: rgb(0xa6adc8),       // subtext0
            dim: rgb(0x6c7086),         // overlay0
            ok: rgb(0xa6e3a1),          // green
            warn: rgb(0xf9e2af),        // yellow
            err: rgb(0xf38ba8),         // red
            edge: rgb(0x313244),        // surface0
            sel_bg: rgb(0x45475a),      // surface1
            sel_bg_dim: rgb(0x313244),
        }
    }

    pub fn macchiato() -> Self {
        Theme {
            bg: rgb(0x181926),
            surface: rgb(0x1e2030),
            surface_focus: rgb(0x24273a),
            accent: rgb(0xc6a0f6),
            on_accent: rgb(0x181926),
            text: rgb(0xcad3f5),
            muted: rgb(0xa5adcb),
            dim: rgb(0x6e738d),
            ok: rgb(0xa6da95),
            warn: rgb(0xeed49f),
            err: rgb(0xed8796),
            edge: rgb(0x363a4f),
            sel_bg: rgb(0x494d64),
            sel_bg_dim: rgb(0x363a4f),
        }
    }

    pub fn frappe() -> Self {
        Theme {
            bg: rgb(0x232634),
            surface: rgb(0x292c3c),
            surface_focus: rgb(0x303446),
            accent: rgb(0xca9ee6),
            on_accent: rgb(0x232634),
            text: rgb(0xc6d0f5),
            muted: rgb(0xa5adce),
            dim: rgb(0x737994),
            ok: rgb(0xa6d189),
            warn: rgb(0xe5c890),
            err: rgb(0xe78284),
            edge: rgb(0x414559),
            sel_bg: rgb(0x51576d),
            sel_bg_dim: rgb(0x414559),
        }
    }

    /// The one light flavor. `bg` is darker than `surface` here too — on a
    /// light theme "nearer the viewer" means lighter, so the relationship
    /// inverts while the roles stay the same.
    pub fn latte() -> Self {
        Theme {
            bg: rgb(0xdce0e8),          // crust
            surface: rgb(0xe6e9ef),     // mantle
            surface_focus: rgb(0xeff1f5), // base
            accent: rgb(0x8839ef),      // mauve
            on_accent: rgb(0xeff1f5),
            text: rgb(0x4c4f69),
            muted: rgb(0x5c5f77),
            dim: rgb(0x8c8fa1),
            ok: rgb(0x40a02b),
            warn: rgb(0xdf8e1d),
            err: rgb(0xd20f39),
            edge: rgb(0xbcc0cc),
            sel_bg: rgb(0xbcc0cc),
            sel_bg_dim: rgb(0xccd0da),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> Vec<Theme> {
        THEMES.iter().map(|n| Theme::by_name(n)).collect()
    }

    #[test]
    fn every_listed_preset_is_a_distinct_palette() {
        let themes = all();
        for (i, a) in themes.iter().enumerate() {
            for b in &themes[i + 1..] {
                assert_ne!(a, b, "two presets resolved to the same palette");
            }
        }
    }

    #[test]
    fn an_unknown_name_falls_back_rather_than_failing() {
        assert_eq!(Theme::by_name("no-such-theme"), Theme::default());
        assert_eq!(Theme::by_name(""), Theme::default());
    }

    #[test]
    fn names_match_case_and_whitespace_insensitively() {
        assert_eq!(Theme::by_name("  Frappe "), Theme::by_name("frappe"));
        assert_eq!(Theme::by_name("LATTE"), Theme::by_name("latte"));
    }

    #[test]
    fn a_theme_can_say_which_preset_it_is() {
        for name in THEMES {
            assert_eq!(Theme::by_name(name).name(), *name);
        }
    }

    #[test]
    fn the_status_roles_stay_visually_distinct() {
        // ok/warn/err carry the whole agent-state signal (§8b); if two of
        // them ever collapse the UI stops communicating.
        for t in all() {
            assert_ne!(t.ok, t.warn);
            assert_ne!(t.warn, t.err);
            assert_ne!(t.ok, t.err);
        }
    }

    #[test]
    fn the_three_elevations_are_all_different() {
        // Page behind unfocused panel behind focused panel is the whole
        // reason the UI reads as cards rather than boxes.
        for t in all() {
            assert_ne!(t.bg, t.surface);
            assert_ne!(t.surface, t.surface_focus);
            assert_ne!(t.bg, t.surface_focus);
        }
    }

    #[test]
    fn selection_fills_are_distinguishable_and_never_the_panel_itself() {
        for t in all() {
            assert_ne!(t.sel_bg, t.sel_bg_dim, "focused selection must read stronger");
            assert_ne!(t.sel_bg, t.surface_focus);
            assert_ne!(t.sel_bg_dim, t.surface);
        }
    }

    #[test]
    fn every_preset_is_truecolor() {
        // An indexed value here would be an ANSI approximation, which is
        // the look this palette exists to get away from.
        for t in all() {
            for c in [
                t.bg, t.surface, t.surface_focus, t.accent, t.on_accent, t.text, t.muted, t.dim,
                t.ok, t.warn, t.err, t.edge, t.sel_bg, t.sel_bg_dim,
            ] {
                assert!(matches!(c, Color::Rgb(..)), "{c:?} is not truecolor");
            }
        }
    }

    #[test]
    fn text_and_background_never_collapse_into_each_other() {
        for t in all() {
            assert_ne!(t.text, t.surface_focus);
            assert_ne!(t.text, t.sel_bg);
            assert_ne!(t.on_accent, t.accent);
        }
    }
}
