//! The Checkouts layer of the key walk: up while the session's stage is Checkouts, hearing the
//! table's keys, the window's own (`Anywhere`) and the extension commands of the `anywhere` and
//! `checkout` scopes, and holding every other key so none is typed into the pane under the stage
//! (`LAYERS`, `stageKeys` in the page).
//!
//! The lookup is the shared one (`keys/place`): the table's own place first, then `Anywhere`. No
//! key is bound in both, so which is asked first changes nothing the page would do. A key of the
//! table is run through `argus-desktop-core`'s `ways::key`: `/` takes the filter field (nothing is
//! sent), `N` asks for a name on a checkout row, and the rest go to the session as they are.
//! Escape on the table puts away what the ladder finds and otherwise does nothing (CHECKOUTS-32).

use argus_desktop_core::checkouts::ways::{self, Key};
use argus_desktop_core::connection::page::Stage;
use argus_desktop_core::keys::keymap::{Action, Place};
use gpui_kit::{App, Context, WeakEntity, Window};

use super::CheckoutsStage;
use crate::checkouts::act;
use crate::keys::extension::{self, ANYWHERE, CHECKOUT};
use crate::keys::layer::{Handler, Press, Verdict};
use crate::keys::place::{look_up, Looked};
use crate::keys::run;

pub struct StageKeys {
    pub stage: WeakEntity<CheckoutsStage>,
}

impl Handler for StageKeys {
    fn open(&self, _: &Window, cx: &App) -> bool {
        self.stage.upgrade().is_some_and(|stage| stage.read(cx).heard.read(cx).rail.stage == Stage::Checkouts)
    }

    fn keys(&self, press: &Press, _: &[Place], window: &mut Window, cx: &mut App) -> Verdict {
        let Some(stage) = self.stage.upgrade() else { return Verdict::Pass };
        match look_up(Place::Checkouts, &[Place::Anywhere], press.page) {
            Looked::Own(action) => {
                stage.update(cx, |stage, cx| stage.run(action, window, cx));
                Verdict::Taken
            }
            Looked::Beyond(action) if run::run(action, window, cx) => Verdict::Taken,
            Looked::Beyond(_) => Verdict::Held,
            // A key the window binds nothing to may be an extension command's, heard on the
            // selected checkout: the row the rail has picked, as the page says it, else the
            // stage's own, which the session resolves (SHELL-KEYS-21, CHECKOUTS-47).
            Looked::Held => {
                let pick = stage.read(cx).heard.read(cx).rail.cursor.pick;
                let (checkout, pane) = (pick.map(|pick| pick.checkout), pick.and_then(|pick| pick.pane));
                if extension::run(&[ANYWHERE, CHECKOUT], press.page, checkout, pane, cx) {
                    Verdict::Taken
                } else {
                    Verdict::Held
                }
            }
        }
    }
}

impl CheckoutsStage {
    /// A key of the table's place, run against the row the table has selected.
    fn run(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        let table = self.heard.read(cx).checkouts.clone();
        match ways::key(action, table.as_ref().and_then(|table| table.selected_row())) {
            Some(Key::Filter) => self.tools.update(cx, |tools, cx| tools.focus(window, cx)),
            Some(Key::Then(then)) => act::run(then, &self.caller, &self.heard, window, cx),
            None => {}
        }
    }
}
