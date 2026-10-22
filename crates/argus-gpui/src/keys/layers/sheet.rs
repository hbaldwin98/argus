//! The shortcuts sheet, over everything: the keys that close it, `?` and Escape, close it, and
//! every other key is held for the sheet's own list and find field (SHELL-KEYS-13).

use argus_desktop_core::keys::keymap::{Action, Place};
use gpui_kit::{App, Window};

use crate::keys::layer::{action_for, Handler, Press, Verdict};
use crate::kit::overlay::host::Overlays;
use crate::kit::overlay::kind::Overlay;

pub struct Sheet;

impl Handler for Sheet {
    fn open(&self, _: &Window, cx: &App) -> bool {
        Overlays::get(cx).is_open(Overlay::Sheet)
    }

    fn keys(&self, press: &Press, places: &[Place], window: &mut Window, cx: &mut App) -> Verdict {
        match action_for(places, press.page) {
            Some(Action::CloseLayer | Action::Shortcuts) => {
                Overlays::get(cx).close(Overlay::Sheet, window, cx);
                Verdict::Taken
            }
            _ => Verdict::Held,
        }
    }
}
