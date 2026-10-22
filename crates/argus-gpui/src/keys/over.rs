//! Opening the palette, the shortcuts sheet or the settings, for a key the page hears and for one
//! the session heard in a pane (`Event::Run`, SHELL-KEYS-9): only one of the three is ever open,
//! `?` while the sheet is up closes it, and the other two open once (SHELL-KEYS-13).
//!
//! The host (`kit/overlay`) closes the others; what is here is which overlay an action is and what
//! it opens. The palette, the sheet and the settings are surfaces the shell builds and puts here
//! (`Surfaces`); the settings chord opens the sheet and, while it is up, puts it away (SHELL-SETTINGS-1).

use argus_desktop_core::keys::keymap::Action;
use gpui_kit::{App, Entity, Global, Window};

use super::palette::PaletteView;
use super::sheet::SheetView;
use crate::kit::overlay::kind::Overlay;
use crate::settings::preferences::PreferencesView;

/// The surfaces an action opens over the stage, built once by the shell.
#[derive(Clone)]
pub struct Surfaces {
    pub palette: Entity<PaletteView>,
    pub sheet: Entity<SheetView>,
    pub settings: Entity<PreferencesView>,
}

impl Global for Surfaces {}

impl Surfaces {
    pub fn install(self, cx: &mut App) {
        cx.set_global(self);
    }

    pub fn get(cx: &App) -> Option<Self> {
        cx.try_global::<Surfaces>().cloned()
    }
}

/// The overlay an action opens.
pub fn overlay_of(action: Action) -> Option<Overlay> {
    match action {
        Action::Palette => Some(Overlay::Palette),
        Action::Shortcuts => Some(Overlay::Sheet),
        Action::Settings => Some(Overlay::Settings),
        _ => None,
    }
}

/// Opens what `action` names over the window; whether it names one.
pub fn run_over(action: Action, window: &mut Window, cx: &mut App) -> bool {
    let Some(overlay) = overlay_of(action) else { return false };
    match (overlay, Surfaces::get(cx)) {
        (Overlay::Palette, Some(surfaces)) => surfaces.palette.update(cx, |palette, cx| palette.open(window, cx)),
        (Overlay::Sheet, Some(surfaces)) => surfaces.sheet.update(cx, |sheet, cx| sheet.toggle(window, cx)),
        (Overlay::Settings, Some(surfaces)) => surfaces.settings.update(cx, |settings, cx| settings.toggle(window, cx)),
        _ => {}
    }
    true
}

#[cfg(test)]
mod tests;
