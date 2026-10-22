//! The stage, always up and last: the keys of the Anywhere place, run as the page's `run` does
//! (`keys/run`), then the extension commands heard anywhere. A key it does not bind is held, not
//! typed anywhere.

use argus_desktop_core::keys::keymap::Place;
use gpui_kit::{App, Window};

use crate::keys::extension::{self, ANYWHERE};
use crate::keys::layer::{action_for, Handler, Press, Verdict};
use crate::keys::run;

pub struct Stage;

impl Handler for Stage {
    fn open(&self, _: &Window, _: &App) -> bool {
        true
    }

    fn keys(&self, press: &Press, places: &[Place], window: &mut Window, cx: &mut App) -> Verdict {
        match action_for(places, press.page) {
            Some(action) if run::run(action, window, cx) => Verdict::Taken,
            // A key the window binds nothing to may be an extension command's (SHELL-KEYS-21). The
            // Checkouts stage hears its `checkout` scope itself (`checkouts/stage/keys`), above this layer.
            None if extension::run(&[ANYWHERE], press.page, None, None, cx) => Verdict::Taken,
            _ => Verdict::Held,
        }
    }
}
