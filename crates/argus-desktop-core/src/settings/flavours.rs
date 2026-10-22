//! The themes the Settings dialog's cards offer, in the order they are laid out and by the name each
//! carries, and what "follow the system" draws.
//!
//! The order and the names were the page's own list (`settings/preferences.ts`, `FLAVOURS`); they
//! are here so a second window does not copy them, and so the one rule that gives the cards a name
//! is checked once. Which colours a theme has is the window's own (a stylesheet, a token space).

use crate::settings::remembered::Theme;

/// The twelve cards: every theme but `System`, which is the switch beside them.
pub const FLAVOURS: [(Theme, &str); 12] = [
    (Theme::Night, "Argus Night"),
    (Theme::Ledger, "Ledger"),
    (Theme::Carbon, "Carbon"),
    (Theme::Flexoki, "Flexoki"),
    (Theme::Kanagawa, "Kanagawa"),
    (Theme::RosePine, "Ros\u{e9} Pine"),
    (Theme::Nord, "Nord"),
    (Theme::Radix, "Radix"),
    (Theme::Mocha, "Mocha"),
    (Theme::Macchiato, "Macchiato"),
    (Theme::Frappe, "Frapp\u{e9}"),
    (Theme::Latte, "Latte"),
];

/// The card's name; `System` has none of its own and is named for what it draws now.
pub fn name(theme: Theme, system_dark: bool) -> &'static str {
    let drawn = theme.resolve(system_dark);
    FLAVOURS.iter().find(|(flavour, _)| *flavour == drawn).map_or("", |(_, name)| name)
}

/// The theme the "Follow the system" switch asks for when pressed: turning it on follows the
/// system, turning it off keeps the flavour that was being drawn.
pub fn toggled_follow(chosen: Theme, system_dark: bool) -> Theme {
    match chosen {
        Theme::System => chosen.resolve(system_dark),
        _ => Theme::System,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_theme_but_system_has_one_card() {
        for theme in [
            Theme::Mocha, Theme::Macchiato, Theme::Frappe, Theme::Latte, Theme::Night, Theme::Ledger, Theme::Carbon,
            Theme::Flexoki, Theme::Kanagawa, Theme::RosePine, Theme::Nord, Theme::Radix,
        ] {
            assert_eq!(FLAVOURS.iter().filter(|(card, _)| *card == theme).count(), 1, "{theme:?}");
        }
        assert!(FLAVOURS.iter().all(|(card, _)| *card != Theme::System), "the switch is not a card");
    }

    #[test]
    fn the_system_is_named_for_what_it_draws() {
        assert_eq!(name(Theme::System, true), "Mocha");
        assert_eq!(name(Theme::System, false), "Latte");
        assert_eq!(name(Theme::RosePine, true), "Ros\u{e9} Pine");
    }

    #[test]
    fn leaving_the_system_keeps_the_flavour_it_drew() {
        assert_eq!(toggled_follow(Theme::System, true), Theme::Mocha);
        assert_eq!(toggled_follow(Theme::System, false), Theme::Latte);
        assert_eq!(toggled_follow(Theme::Nord, true), Theme::System);
    }
}
