//! A drawing opened larger holds every key while it is up, ahead of every layer and of the window's
//! own keys, as the page's capture listener does (FIGURE-6): Escape closes it, `+` and `-` zoom,
//! `0` is 100% and `f` fits it, and any other key is held and does nothing, so nothing under the view
//! hears it.
//!
//! It is not one of `argus-desktop-core`'s layers, which the page's key handlers answer one for one;
//! the page's larger view never went through them. It hears the diagram place's keys, which the
//! sheet already lists.

use argus_desktop_core::keys::keymap::{Action, Place};
use gpui_kit::{App, Window};

use crate::keys::layer::{action_for, Handler, Press, Verdict};
use crate::kit::overlay::host::Overlays;
use crate::kit::overlay::kind::Overlay;
use crate::views::blocks::figure::larger::Move;
use crate::views::blocks::figure::open::zoom;

pub struct Figure;

impl Handler for Figure {
    fn open(&self, _: &Window, cx: &App) -> bool {
        Overlays::get(cx).is_open(Overlay::Figure)
    }

    fn keys(&self, press: &Press, _: &[Place], window: &mut Window, cx: &mut App) -> Verdict {
        let by = match action_for(&[Place::Diagram, Place::Anywhere], press.page) {
            Some(Action::CloseLayer) => {
                Overlays::get(cx).close(Overlay::Figure, window, cx);
                return Verdict::Taken;
            }
            Some(Action::ZoomIn) => Move::In,
            Some(Action::ZoomOut) => Move::Out,
            Some(Action::ZoomReset) => Move::Actual,
            Some(Action::Fit) => Move::Fit,
            _ => return Verdict::Held,
        };
        if zoom(by, window, cx) { Verdict::Taken } else { Verdict::Held }
    }
}
