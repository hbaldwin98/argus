//! A tracker dialog's layer of the key walk: up while its overlay is, hearing the keys of its own
//! place and of `Anywhere`, as the page's `hear` does for a dialog with a `sink` (BOARD-LINK-8,
//! BOARD-RECONCILE-12, BOARD-LINK-14).
//!
//! `?` opens the shortcuts sheet over it; the stage switches, the composer, the console, the views,
//! the palette and the settings are not a dialog's (`NOT_THEIRS`) and are held and ignored; every
//! other key it binds goes to the dialog's own sink, which sends it to Rust. A key it does not bind
//! is held, so nothing under the scrim hears it. A field of the dialog answers its own keys first
//! (`keys/field`), as that layer is above this one.

use std::rc::Rc;

use argus_desktop_core::keys::keymap::{Action, Place};
use gpui_kit::{App, Window};

use crate::keys::layer::{action_for, Handler, Press, Verdict};
use crate::keys::run;
use crate::kit::overlay::host::Overlays;
use crate::kit::overlay::kind::Overlay;

/// What a dialog does with an action of its own: whether it ran it.
pub type Sink = Rc<dyn Fn(Action, &mut Window, &mut App) -> bool>;

/// The keys a dialog does not answer, open over it (`NOT_THEIRS` in `page/src/keys/keymap.ts`).
pub fn theirs(action: Action) -> bool {
    !matches!(
        action,
        Action::Workspace
            | Action::Feature
            | Action::Checkouts
            | Action::Compose
            | Action::Console
            | Action::ToggleConsole
            | Action::Views
            | Action::Palette
            | Action::Settings
    )
}

pub struct DialogKeys {
    pub overlay: Overlay,
    pub sink: Sink,
}

impl Handler for DialogKeys {
    fn open(&self, _: &Window, cx: &App) -> bool {
        Overlays::get(cx).is_open(self.overlay)
    }

    fn keys(&self, press: &Press, places: &[Place], window: &mut Window, cx: &mut App) -> Verdict {
        match action_for(places, press.page) {
            Some(Action::Shortcuts) => {
                run::run(Action::Shortcuts, window, cx);
                Verdict::Taken
            }
            Some(action) if theirs(action) && (self.sink)(action, window, cx) => Verdict::Taken,
            _ => Verdict::Held,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stage_switches_and_the_windows_own_commands_are_not_a_dialogs() {
        for action in [Action::Workspace, Action::Feature, Action::Checkouts, Action::Compose, Action::Console, Action::ToggleConsole, Action::Views, Action::Palette, Action::Settings] {
            assert!(!theirs(action), "{action:?}");
        }
        for action in [Action::Next, Action::Choose, Action::CloseLayer, Action::Tick, Action::Discard] {
            assert!(theirs(action), "{action:?}");
        }
    }
}
