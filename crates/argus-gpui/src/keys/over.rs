//! Opening the palette, the shortcuts sheet or the settings, for a key the page hears and for one
//! the session heard in a pane (`Event::Run`, SHELL-KEYS-9): only one of the three is ever open,
//! `?` while the sheet is up closes it, and the other two open once (SHELL-KEYS-13).
//!
//! The host (`kit/overlay`) closes the others; what is here is which overlay an action is and
//! what it opens as. None of the three surfaces is in the GPUI window yet, so each opens as a
//! dialog that says so, a stand-in that is a real overlay with the real rules, replaced where its
//! surface lands (audit items 33 to 35).

use argus_desktop_core::keys::keymap::Action;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::{prelude::*, px, App, Window};

use crate::kit::overlay::host::{Overlays, Spec};
use crate::kit::overlay::kind::Overlay;
use crate::layout::panel::NOT_YET;
use crate::theme::Tokens;

/// The overlay an action opens.
pub fn overlay_of(action: Action) -> Option<Overlay> {
    match action {
        Action::Palette => Some(Overlay::Palette),
        Action::Shortcuts => Some(Overlay::Sheet),
        Action::Settings => Some(Overlay::Settings),
        _ => None,
    }
}

fn title(overlay: Overlay) -> &'static str {
    match overlay {
        Overlay::Palette => "Search everything",
        Overlay::Sheet => "Shortcuts",
        _ => "Settings",
    }
}

/// Opens what `action` names over the window; whether it names one.
pub fn run_over(action: Action, window: &mut Window, cx: &mut App) -> bool {
    let Some(overlay) = overlay_of(action) else { return false };
    let host = Overlays::get(cx);
    let build = move |dialog: Dialog, _: &mut Window, cx: &mut App, _: bool| dialog.title(title(overlay)).w(px(Tokens::global(cx).dialog_w)).child(NOT_YET);
    if overlay == Overlay::Sheet {
        host.toggle(Spec::new(overlay), build, window, cx);
    } else {
        host.open(Spec::new(overlay), build, window, cx);
    }
    true
}

#[cfg(test)]
mod tests;
