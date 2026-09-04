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

/// What a token *is*, coloured. The daemon sends review diffs tagged with
/// these roles and never with colours, so this is the only place a language
/// turns into a palette.
///
/// Nine roles for ten span kinds: constants and numbers share one, which is
/// what Catppuccin itself does. Identifiers have no role at all — the daemon
/// does not tag them, because a diff where every name is coloured hides the
/// thing a diff is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Syntax {
    pub keyword: Color,
    pub string: Color,
    /// Drawn italic as well, so prose reads as prose at a glance.
    pub comment: Color,
    /// Numbers and constants both.
    pub number: Color,
    pub type_name: Color,
    pub function: Color,
    pub property: Color,
    pub operator: Color,
    /// Brackets and delimiters, kept quiet: they are structure, not content.
    pub punctuation: Color,
}

/// Semantic color roles for the whole client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    /// The page behind the panels. Deliberately darker than `surface` so a
    /// panel reads as a card sitting on it rather than a box drawn in it.
    ///
    /// These three are the elevation scale, and the whole layout rests on
    /// them being *separable*, not merely unequal. They were first assigned
    /// Catppuccin's crust/mantle/base, which are that flavour's background
    /// trio — three shades of the same ground, 13/255 apart in mocha. The
    /// scale was therefore invisible, and the panel borders ended up
    /// carrying all of the structure, which is what made the UI read as a
    /// grid of dialog boxes. Every surface role now sits one rung higher,
    /// on the layers Catppuccin publishes for raised elements.
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
    /// The wash behind an added or removed diff line. Background, because
    /// syntax highlighting owns the foreground: a `+` line has to stay
    /// readable as code and still read as added from across the screen.
    pub add_bg: Color,
    pub del_bg: Color,
    /// The same wash under the review's selection. A selected range covers
    /// whole lines, so selection cannot simply take the background over
    /// without erasing which side of the diff each line was on.
    pub add_bg_sel: Color,
    pub del_bg_sel: Color,
    pub syntax: Syntax,
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
            bg: rgb(0x11111b),            // crust
            surface: rgb(0x1e1e2e),       // base
            surface_focus: rgb(0x313244), // surface0
            accent: rgb(0xcba6f7),        // mauve
            on_accent: rgb(0x11111b),
            text: rgb(0xcdd6f4),
            muted: rgb(0xa6adc8),  // subtext0
            dim: rgb(0x6c7086),    // overlay0
            ok: rgb(0xa6e3a1),     // green
            warn: rgb(0xf9e2af),   // yellow
            err: rgb(0xf38ba8),    // red
            edge: rgb(0x45475a),   // surface1
            sel_bg: rgb(0x585b70), // surface2
            sel_bg_dim: rgb(0x45475a),
            add_bg: rgb(0x2a4433),
            del_bg: rgb(0x4d2c3e),
            add_bg_sel: rgb(0x365c43),
            del_bg_sel: rgb(0x633a4e),
            syntax: Syntax {
                keyword: rgb(0xcba6f7),
                string: rgb(0xa6e3a1),
                comment: rgb(0x7f849c),
                number: rgb(0xfab387),
                type_name: rgb(0xf9e2af),
                function: rgb(0x89b4fa),
                property: rgb(0x94e2d5),
                operator: rgb(0x89dceb),
                punctuation: rgb(0x9399b2),
            },
        }
    }

    pub fn macchiato() -> Self {
        Theme {
            bg: rgb(0x181926),
            surface: rgb(0x24273a),
            surface_focus: rgb(0x363a4f),
            accent: rgb(0xc6a0f6),
            on_accent: rgb(0x181926),
            text: rgb(0xcad3f5),
            muted: rgb(0xa5adcb),
            dim: rgb(0x6e738d),
            ok: rgb(0xa6da95),
            warn: rgb(0xeed49f),
            err: rgb(0xed8796),
            edge: rgb(0x494d64),
            sel_bg: rgb(0x5b6078),
            sel_bg_dim: rgb(0x494d64),
            add_bg: rgb(0x2e4a3c),
            del_bg: rgb(0x533341),
            add_bg_sel: rgb(0x3a5c48),
            del_bg_sel: rgb(0x66404f),
            syntax: Syntax {
                keyword: rgb(0xc6a0f6),
                string: rgb(0xa6da95),
                comment: rgb(0x8087a2),
                number: rgb(0xf5a97f),
                type_name: rgb(0xeed49f),
                function: rgb(0x8aadf4),
                property: rgb(0x8bd5ca),
                operator: rgb(0x91d7e3),
                punctuation: rgb(0x939ab7),
            },
        }
    }

    pub fn frappe() -> Self {
        Theme {
            bg: rgb(0x232634),
            surface: rgb(0x303446),
            surface_focus: rgb(0x414559),
            accent: rgb(0xca9ee6),
            on_accent: rgb(0x232634),
            text: rgb(0xc6d0f5),
            muted: rgb(0xa5adce),
            dim: rgb(0x737994),
            ok: rgb(0xa6d189),
            warn: rgb(0xe5c890),
            err: rgb(0xe78284),
            edge: rgb(0x51576d),
            sel_bg: rgb(0x626880),
            sel_bg_dim: rgb(0x51576d),
            add_bg: rgb(0x385440),
            del_bg: rgb(0x573c4a),
            add_bg_sel: rgb(0x456650),
            del_bg_sel: rgb(0x6a4a5b),
            syntax: Syntax {
                keyword: rgb(0xca9ee6),
                string: rgb(0xa6d189),
                comment: rgb(0x838ba7),
                number: rgb(0xef9f76),
                type_name: rgb(0xe5c890),
                function: rgb(0x8caaee),
                property: rgb(0x81c8be),
                operator: rgb(0x99d5ce),
                punctuation: rgb(0x949cbb),
            },
        }
    }

    /// The one light flavor. `bg` is darker than `surface` here too — on a
    /// light theme "nearer the viewer" means lighter, so the relationship
    /// inverts while the roles stay the same.
    pub fn latte() -> Self {
        Theme {
            bg: rgb(0xccd0da),            // surface0
            surface: rgb(0xdce0e8),       // crust
            surface_focus: rgb(0xeff1f5), // base
            accent: rgb(0x8839ef),        // mauve
            on_accent: rgb(0xeff1f5),
            text: rgb(0x4c4f69),
            muted: rgb(0x5c5f77),
            dim: rgb(0x8c8fa1),
            ok: rgb(0x40a02b),
            warn: rgb(0xdf8e1d),
            err: rgb(0xd20f39),
            edge: rgb(0xacb0be),
            sel_bg: rgb(0xbcc0cc),
            sel_bg_dim: rgb(0xccd0da),
            add_bg: rgb(0xd8f0d0),
            del_bg: rgb(0xf7d8dd),
            add_bg_sel: rgb(0xc3e6b8),
            del_bg_sel: rgb(0xefc2ca),
            syntax: Syntax {
                keyword: rgb(0x8839ef),
                string: rgb(0x40a02b),
                comment: rgb(0x8c8fa1),
                number: rgb(0xfe640b),
                type_name: rgb(0xdf8e1d),
                function: rgb(0x1e66f5),
                property: rgb(0x179299),
                operator: rgb(0x04a5e5),
                punctuation: rgb(0x7c7f93),
            },
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

    /// Relative luminance on a 0..255 scale, sRGB coefficients without the
    /// gamma step: enough to say whether two greys of the same family read
    /// as different surfaces, which is all this file asks of it.
    fn luminance(c: Color) -> f32 {
        let Color::Rgb(r, g, b) = c else {
            panic!("{c:?} is not truecolor");
        };
        0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32
    }

    /// How far apart two neighbouring elevations have to be. The scale this
    /// replaced was `assert_ne!`-different and still invisible: its widest
    /// step was 7, and the eye cannot separate two dark greys that close.
    const ELEVATION_STEP: f32 = 10.0;

    #[test]
    fn each_elevation_is_visibly_above_the_one_below() {
        // Page behind unfocused panel behind focused panel is the whole
        // reason the UI reads as cards rather than boxes. Asserting they
        // merely differ is what let a 3%-wide scale ship: the roles were
        // all distinct and none of them was distinguishable.
        for t in all() {
            let steps = [
                ("bg -> surface", t.bg, t.surface),
                ("surface -> surface_focus", t.surface, t.surface_focus),
            ];
            for (what, lower, upper) in steps {
                let step = (luminance(upper) - luminance(lower)).abs();
                assert!(
                    step >= ELEVATION_STEP,
                    "{} in {}: {step:.1} apart, needs {ELEVATION_STEP}",
                    what,
                    t.name(),
                );
            }
            // Latte raises by getting lighter and the dark flavours by
            // getting lighter too, so the ladder only has to be monotone,
            // not signed one particular way.
            let ordered = (luminance(t.surface) - luminance(t.bg)).signum()
                == (luminance(t.surface_focus) - luminance(t.surface)).signum();
            assert!(ordered, "{}'s elevations are not monotone", t.name());
        }
    }

    #[test]
    fn a_selected_row_reads_against_the_card_it_sits_on() {
        // The selection fill is the only thing marking where you are, and
        // it is drawn over whichever card has focus.
        for t in all() {
            let on_focused = (luminance(t.sel_bg) - luminance(t.surface_focus)).abs();
            let on_unfocused = (luminance(t.sel_bg_dim) - luminance(t.surface)).abs();
            assert!(on_focused >= ELEVATION_STEP, "{}: {on_focused:.1}", t.name());
            assert!(
                on_unfocused >= ELEVATION_STEP,
                "{}: {on_unfocused:.1}",
                t.name()
            );
        }
    }

    #[test]
    fn selection_fills_are_distinguishable_and_never_the_panel_itself() {
        for t in all() {
            assert_ne!(
                t.sel_bg, t.sel_bg_dim,
                "focused selection must read stronger"
            );
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
                t.bg,
                t.surface,
                t.surface_focus,
                t.accent,
                t.on_accent,
                t.text,
                t.muted,
                t.dim,
                t.ok,
                t.warn,
                t.err,
                t.edge,
                t.sel_bg,
                t.sel_bg_dim,
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
