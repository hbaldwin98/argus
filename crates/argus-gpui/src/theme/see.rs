//! What shows through the window: the background-opacity setting as the alpha of the three large
//! fills and of nothing else.
//!
//! The page's `--see` is the alpha of its rail, strip and stage fills, so text, borders, controls,
//! dialogs and menus stay solid while the desktop (or Mica, or Acrylic) shows through. Here the
//! same three are tokens of their own (`see_chrome`, `see_surface`, `see_sunken`), and a region
//! paints its fill with one of them; the `bg_*` roles stay opaque for everything that must not
//! be see-through.

use super::Tokens;

impl Tokens {
    /// These tokens with the three large fills at `percent` of their alpha (SHELL-LOOK-3, 5).
    pub fn seen(mut self, percent: u8) -> Self {
        let alpha = f32::from(percent.min(100)) / 100.0;
        self.see_chrome = self.bg_chrome.opacity(alpha);
        self.see_surface = self.bg_surface.opacity(alpha);
        self.see_sunken = self.bg_sunken.opacity(alpha);
        self
    }
}

#[cfg(test)]
mod tests {
    use crate::theme::themes::ledger;

    /// SETTINGS-6c: only the large fills fade; text, borders and the dialogs' own fills do not.
    #[test]
    fn opacity_is_the_alpha_of_the_three_large_fills_and_of_nothing_else() {
        let solid = ledger::tokens();
        let seen = solid.seen(40);
        for fill in [seen.see_chrome, seen.see_surface, seen.see_sunken] {
            assert!((fill.a - 0.4).abs() < 1e-6);
        }
        assert_eq!((seen.see_chrome.h, seen.see_chrome.l), (solid.bg_chrome.h, solid.bg_chrome.l), "same colour, less of it");
        for kept in [
            (seen.bg_chrome, solid.bg_chrome),
            (seen.bg_surface, solid.bg_surface),
            (seen.bg_sunken, solid.bg_sunken),
            (seen.bg_raised, solid.bg_raised),
            (seen.line, solid.line),
            (seen.line_strong, solid.line_strong),
            (seen.fg, solid.fg),
            (seen.fg_muted, solid.fg_muted),
            (seen.accent, solid.accent),
        ] {
            assert_eq!(kept.0, kept.1);
            assert_eq!(kept.0.a, 1.0);
        }
    }

    /// SHELL-LOOK-3, style.css "The see-through fills": each large region paints its own see token,
    /// and the kit root paints nothing beneath them. A region painted with a `bg_*` role stays solid
    /// however far the slider moves, so the source of each is held to its token.
    #[test]
    fn each_large_region_paints_its_see_token_and_the_root_paints_none() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        for (file, token) in [
            ("chrome/rail/mod.rs", "see_chrome"),
            ("chrome/header/mod.rs", "see_surface"),
            ("chrome/stage/empty.rs", "see_surface"),
            ("layout/float.rs", "see_surface"),
            ("chrome/shell.rs", "see_sunken"),
            ("layout/dock.rs", "see_sunken"),
            ("panes/console/mod.rs", "see_sunken"),
            ("panes/terminal/mod.rs", "see_sunken"),
        ] {
            let text = std::fs::read_to_string(src.join(file)).unwrap();
            assert!(text.contains(&format!(".bg(tokens.{token})")), "{file} paints {token}");
        }
        assert_eq!(crate::theme::kit::colors(&ledger::tokens()).background, gpui_kit::Hsla::transparent_black());
        let shell = std::fs::read_to_string(src.join("chrome/shell.rs")).unwrap();
        assert!(shell.contains(".bg(tokens.scrim)"), "a dialog that is up dims the window with the page's scrim");
        let host = std::fs::read_to_string(src.join("kit/overlay/host.rs")).unwrap();
        assert!(host.contains("dialog.bg(surface)"), "a dialog paints its own solid surface over the transparent root");
    }

    #[test]
    fn a_hundred_percent_is_the_solid_fills() {
        let t = ledger::tokens().seen(100);
        assert_eq!((t.see_chrome, t.see_surface, t.see_sunken), (t.bg_chrome, t.bg_surface, t.bg_sunken));
    }

    #[test]
    fn more_than_a_hundred_is_solid() {
        assert_eq!(ledger::tokens().seen(250).see_sunken.a, 1.0);
    }
}
