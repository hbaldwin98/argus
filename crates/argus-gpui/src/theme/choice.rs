//! Which theme a choice draws: the person's pick, or, following the system, Mocha when it is dark
//! and Latte when it is light.
//!
//! The choice and its rule are core's (`settings::remembered::Theme::resolve`, which the Tauri
//! page's `drawn()` is the copy of); this is the table from each to its data.

use argus_desktop_core::settings::remembered::Theme as Chosen;

use super::define::Theme;
use super::themes::{carbon, catppuccin, flexoki, kanagawa, ledger, night, nord, radix, rose_pine};
use super::Tokens;

/// The data of the theme `chosen` draws now.
pub fn theme(chosen: Chosen, system_dark: bool) -> &'static Theme {
    match chosen.resolve(system_dark) {
        Chosen::Mocha | Chosen::System => &catppuccin::MOCHA,
        Chosen::Macchiato => &catppuccin::MACCHIATO,
        Chosen::Frappe => &catppuccin::FRAPPE,
        Chosen::Latte => &catppuccin::LATTE,
        Chosen::Night => &night::NIGHT,
        Chosen::Ledger => &ledger::THEME,
        Chosen::Carbon => &carbon::CARBON,
        Chosen::Flexoki => &flexoki::FLEXOKI,
        Chosen::Kanagawa => &kanagawa::KANAGAWA,
        Chosen::RosePine => &rose_pine::ROSE_PINE,
        Chosen::Nord => &nord::NORD,
        Chosen::Radix => &radix::RADIX,
    }
}

/// The token space of what is chosen, its large fills at `opacity` percent.
pub fn tokens(chosen: Chosen, system_dark: bool, opacity: u8) -> Tokens {
    super::derive::tokens(theme(chosen, system_dark)).seen(opacity)
}

#[cfg(test)]
mod tests;
