//! A menu or a question over the stage. It steps through its items before the dialogs under it
//! hear anything (the kit's popup menu is up while its context holds the keys, `kit/menu`), and
//! its Escape answers the question no (SHELL-KEYS-15, 19).
//!
//! `LAYERS` lists it after the dialogs, so a walk that took the order alone would let a dialog
//! close under a menu opened from it; a kit menu is up only while it has the keys, which is the
//! first thing the walk finds.

use argus_desktop_core::keys::keymap::{Action, Place};
use gpui_kit::{App, Window};

use crate::keys::layer::{action_for, Handler, Press, Verdict};
use crate::keys::run;
use crate::kit::menu::stepping;
use crate::kit::overlay::host::Overlays;
use crate::kit::overlay::kind::Overlay;

pub struct Menu;

impl Handler for Menu {
    fn open(&self, window: &Window, cx: &App) -> bool {
        stepping::is_open(window) || Overlays::get(cx).is_open(Overlay::Question) || Overlays::get(cx).is_open(Overlay::Removal)
    }

    fn keys(&self, press: &Press, places: &[Place], window: &mut Window, cx: &mut App) -> Verdict {
        // The kit's menu closes on Escape and chooses on Enter itself.
        if stepping::is_open(window) {
            return if stepping::step(press.keystroke, window, cx) { Verdict::Taken } else { Verdict::Held };
        }
        match action_for(places, press.page) {
            Some(Action::CloseLayer) if run::run(Action::CloseLayer, window, cx) => Verdict::Taken,
            _ => Verdict::Held,
        }
    }

    /// A question the stage asked is answered no, not only put away.
    fn escape(&self, window: &mut Window, cx: &mut App) -> bool {
        let overlays = Overlays::get(cx);
        overlays.escape_of(Overlay::Question, window, cx) || overlays.escape_of(Overlay::Removal, window, cx)
    }
}
