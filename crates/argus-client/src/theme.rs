//! Every color the UI draws, keyed by the *role* it plays rather than
//! hardcoded at the call site (DESIGN.md §6, M6 "a real UI theme pass").
//!
//! Presets stay on ANSI-16 and 256-color indexed values so they render on
//! anything. The one exception is `focus_tint`, which needs a roughly
//! 5%-opacity accent shade to wash the focused column — the 256 palette's
//! darkest chromatic steps start far brighter than that — so it is
//! truecolor RGB. A terminal that can't do truecolor degrades to a slightly
//! off background there, which is the least load-bearing cue in the set.

use ratatui::style::Color;

/// Preset names, in the order a future settings toggle would cycle them.
/// `by_name` matches case-insensitively and falls back to the first.
pub const THEMES: &[&str] = &["default", "ocean", "forest", "amber"];

/// Semantic color roles for the whole client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    /// Focus borders, focused titles, the selection marker, the cursor.
    pub accent: Color,
    /// Text drawn on top of an `accent` fill (the focused title chip).
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
    /// Structural chrome: borders of unfocused panels. Darker than `dim`
    /// so the frame recedes behind the content it holds.
    pub edge: Color,
    /// Selected-row fill in the focused column — a subtly raised surface,
    /// never a reverse-video slab.
    pub sel_bg: Color,
    /// Selected-row fill in unfocused columns; barely raised, just enough
    /// to remember where you were.
    pub sel_bg_dim: Color,
    /// The focused column's background wash. Truecolor by necessity.
    pub focus_tint: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            accent: Color::Cyan,
            on_accent: Color::Black,
            text: Color::White,
            muted: Color::Gray,
            dim: Color::DarkGray,
            ok: Color::Green,
            warn: Color::Yellow,
            err: Color::Red,
            edge: Color::Indexed(238),
            sel_bg: Color::Indexed(237),
            sel_bg_dim: Color::Indexed(235),
            focus_tint: Color::Rgb(4, 15, 16),
        }
    }
}

impl Theme {
    /// The preset named by `ARGUS_THEME`, or the default. An env var rather
    /// than a config key for now: the client has no config file of its own
    /// yet, and this keeps the palette switchable while the roles above are
    /// still being tuned. A settings toggle is the M6 shape (`THEMES` is
    /// the cycle order it would use).
    pub fn from_env() -> Self {
        let Ok(name) = std::env::var("ARGUS_THEME") else {
            return Theme::default();
        };
        let known = THEMES
            .iter()
            .any(|t| t.eq_ignore_ascii_case(name.trim()));
        if !known {
            // A typo should be inert, not a silently different palette.
            return Theme::default();
        }
        Theme::by_name(&name)
    }

    pub fn by_name(name: &str) -> Self {
        let base = Theme::default();
        match name.trim().to_ascii_lowercase().as_str() {
            "ocean" => Theme {
                accent: Color::Indexed(39),
                focus_tint: Color::Rgb(3, 13, 20),
                ..base
            },
            "forest" => Theme {
                accent: Color::Indexed(114),
                focus_tint: Color::Rgb(8, 16, 9),
                ..base
            },
            "amber" => Theme {
                accent: Color::Indexed(214),
                focus_tint: Color::Rgb(19, 14, 4),
                ..base
            },
            _ => base,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_preset_actually_differs_from_the_default() {
        for name in THEMES {
            let theme = Theme::by_name(name);
            if *name != "default" {
                assert_ne!(theme, Theme::default(), "{name} silently fell back");
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
        assert_eq!(Theme::by_name("  Ocean "), Theme::by_name("ocean"));
        assert_eq!(Theme::by_name("AMBER"), Theme::by_name("amber"));
    }

    #[test]
    fn the_status_roles_stay_visually_distinct() {
        // ok/warn/err carry the whole agent-state signal (§8b); if two of
        // them ever collapse to one color the UI stops communicating.
        let t = Theme::default();
        assert_ne!(t.ok, t.warn);
        assert_ne!(t.warn, t.err);
        assert_ne!(t.ok, t.err);
    }

    #[test]
    fn selection_fills_are_distinguishable_and_not_the_plain_background() {
        let t = Theme::default();
        assert_ne!(t.sel_bg, t.sel_bg_dim, "focused selection must read stronger");
        assert_ne!(t.sel_bg, Color::Reset);
    }
}
