//! The colour of the one-pixel border Windows 11 draws round a frameless window, per theme: the
//! sidebar's rule, `--line` of the stylesheet, opaque (Argus Night's rule is translucent, so its
//! border is the opaque step it sits on).
//!
//! A table of its own because the system wants a solid colour and each theme's `--line` is a
//! derived one; both windows put it on the live window when the theme, or the system's light or
//! dark, changes.

use crate::settings::remembered::Theme;

/// `0xrrggbb` for the theme as drawn. `System` is whichever of Mocha and Latte `system_dark` says.
pub fn frame_border(theme: Theme, system_dark: bool) -> u32 {
    match theme.resolve(system_dark) {
        Theme::Latte => 0xccd0da,
        Theme::Macchiato => 0x363a4f,
        Theme::Frappe => 0x414559,
        Theme::Night => 0x161a2e,
        Theme::Ledger => 0x23252a,
        Theme::Carbon => 0x393939,
        Theme::Flexoki => 0x282726,
        Theme::Kanagawa => 0x2a2a37,
        Theme::RosePine => 0x26233a,
        Theme::Nord => 0x434c5e,
        Theme::Radix => 0x363a3f,
        Theme::Mocha | Theme::System => 0x313244,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_border_takes_the_line_colour_the_stylesheet_names() {
        let css = include_str!("../../../argus-desktop/assets/style.css");
        for theme in crate::settings::flavours::FLAVOURS.map(|(theme, _)| theme) {
            assert!(css.contains(&format!("--surface0: #{:06x};", frame_border(theme, true))), "{theme:?}");
        }
    }

    #[test]
    fn the_system_follows_mocha_and_latte() {
        assert_eq!(frame_border(Theme::System, true), frame_border(Theme::Mocha, false));
        assert_eq!(frame_border(Theme::System, false), frame_border(Theme::Latte, true));
    }

    #[test]
    fn a_flavour_chosen_ignores_the_systems_light_or_dark() {
        assert_eq!(frame_border(Theme::Nord, true), 0x434c5e);
        assert_eq!(frame_border(Theme::Nord, false), 0x434c5e);
    }
}
