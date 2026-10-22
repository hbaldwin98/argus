//! The Files layer of the key walk: up once a click or a key has given the panel's tree the keys,
//! hearing the panel's own place first and letting every other key go on to the Checkouts layer
//! (`files` in the page's `handlers`).
//!
//! `/` takes the filter, Escape puts the panel away through the window's one Escape ladder (a menu
//! or a question over it goes first), and the rest are sent to the session as the panel's keys, which
//! move its selection and open what is selected.

use argus_desktop_core::checkouts::files::FilesGesture;
use argus_desktop_core::connection::page::Stage;
use argus_desktop_core::keys::keymap::{Action, Place};
use gpui_kit::{App, WeakEntity, Window};

use super::FilesPanel;
use crate::keys::layer::{Handler, Press, Verdict};
use crate::keys::place::{look_up, Looked};
use crate::keys::run;

pub struct PanelKeys {
    pub panel: WeakEntity<FilesPanel>,
}

impl Handler for PanelKeys {
    fn open(&self, window: &Window, cx: &App) -> bool {
        self.panel.upgrade().is_some_and(|panel| {
            let panel = panel.read(cx);
            panel.heard.read(cx).rail.stage == Stage::Checkouts && panel.tree_has_keys(window, cx)
        })
    }

    fn keys(&self, press: &Press, _: &[Place], window: &mut Window, cx: &mut App) -> Verdict {
        let Some(panel) = self.panel.upgrade() else { return Verdict::Pass };
        // No place beyond its own: a key it does not bind is the Checkouts layer's.
        let Looked::Own(action) = look_up(Place::Files, &[], press.page) else { return Verdict::Pass };
        match action {
            Action::Filter => panel.update(cx, |panel, cx| panel.focus_filter(window, cx)),
            Action::CloseLayer => {
                run::run(Action::CloseLayer, window, cx);
            }
            action => panel.read(cx).ask(FilesGesture::Action { action }, cx),
        }
        Verdict::Taken
    }

    /// Escape in the panel puts it away, only while the tree has the keys.
    fn escape(&self, window: &mut Window, cx: &mut App) -> bool {
        if !self.open(window, cx) {
            return false;
        }
        match self.panel.upgrade() {
            Some(panel) => {
                panel.read(cx).ask(FilesGesture::Close, cx);
                true
            }
            None => false,
        }
    }
}
