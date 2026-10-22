//! The Outline and Diagram layers of the key walk: up while a feature is open, and while its flow is
//! expanded over the window; each hears its own place and then the window's, and holds every other key,
//! so none is typed into the pane under the stage (BOARD-HEAD-6, BOARD-WORK-24, BOARD-SEQ-11, -19).
//!
//! The Diagram layer hears the Diagram place before the Outline's, so its digits and its `f` are its own
//! and the window's stage switches stay under them. A key a lens has nothing to do with is the window's,
//! asked for as the page's `run` does (`keys/run`), and else held. Escape is the Outline layer's: it puts
//! away what the feature has open in the order the page does (`OpenFeature::leave`).

use argus_desktop_core::keys::keymap::Place;
use gpui_kit::{App, WeakEntity, Window};

use super::OpenFeature;
use crate::keys::extension::{self, ANYWHERE};
use crate::keys::layer::{action_for, Handler, Press, Verdict};
use crate::keys::run;

/// A key of `places`, run by the feature or else the window.
fn hear(feature: &WeakEntity<OpenFeature>, press: &Press, places: &[Place], window: &mut Window, cx: &mut App) -> Verdict {
    let Some(feature) = feature.upgrade() else { return Verdict::Pass };
    let Some(action) = action_for(places, press.page) else {
        // A key the window binds nothing to may be an extension command's (SHELL-KEYS-21).
        return if extension::run(&[ANYWHERE], press.page, None, None, cx) { Verdict::Taken } else { Verdict::Held };
    };
    if feature.update(cx, |feature, cx| feature.run_key(action, window, cx)) || run::run(action, window, cx) {
        Verdict::Taken
    } else {
        Verdict::Held
    }
}

pub struct OutlineKeys {
    pub feature: WeakEntity<OpenFeature>,
}

impl Handler for OutlineKeys {
    fn open(&self, _: &Window, cx: &App) -> bool {
        self.feature.upgrade().is_some_and(|feature| feature.read(cx).is_open(cx))
    }

    fn keys(&self, press: &Press, places: &[Place], window: &mut Window, cx: &mut App) -> Verdict {
        hear(&self.feature, press, places, window, cx)
    }

    fn escape(&self, window: &mut Window, cx: &mut App) -> bool {
        let Some(feature) = self.feature.upgrade() else { return false };
        if !feature.read(cx).is_open(cx) {
            return false;
        }
        feature.update(cx, |feature, cx| feature.leave(window, cx))
    }
}

pub struct DiagramKeys {
    pub feature: WeakEntity<OpenFeature>,
}

impl Handler for DiagramKeys {
    fn open(&self, _: &Window, cx: &App) -> bool {
        self.feature.upgrade().is_some_and(|feature| feature.read(cx).is_expanded(cx))
    }

    fn keys(&self, press: &Press, places: &[Place], window: &mut Window, cx: &mut App) -> Verdict {
        hear(&self.feature, press, places, window, cx)
    }
}
