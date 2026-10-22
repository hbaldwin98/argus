//! The Code layer of the key walk: up while the session's stage is Code, hearing the stage's own
//! place and the window's (`Anywhere`), and holding every other key so none is typed into the pane
//! under the stage (`code` in the page's `handlers`).
//!
//! A key of the stage's place is sent as the action it is (`codeAction`): the session moves the
//! cursor, opens the go-to list and the search, flips the outline and goes back. A key of the
//! window's own is run as anywhere else; Escape puts away the first thing the ladder finds and,
//! in this stage, never the stage: what is over the file closes one thing at a time and the rest is Rust's.

use argus_desktop_core::code::stage::CodeGesture;
use argus_desktop_core::connection::page::Stage;
use argus_desktop_core::keys::keymap::{Action, Place};
use gpui_kit::{App, WeakEntity, Window};

use super::CodeStage;
use crate::keys::extension::{self, ANYWHERE};
use crate::keys::layer::{Handler, Press, Verdict};
use crate::keys::place::{look_up, Looked};
use crate::keys::run;

pub struct StageKeys {
    pub stage: WeakEntity<CodeStage>,
}

impl Handler for StageKeys {
    fn open(&self, _: &Window, cx: &App) -> bool {
        self.stage.upgrade().is_some_and(|stage| stage.read(cx).heard.read(cx).rail.stage == Stage::Code)
    }

    fn keys(&self, press: &Press, _: &[Place], window: &mut Window, cx: &mut App) -> Verdict {
        let Some(stage) = self.stage.upgrade() else { return Verdict::Pass };
        match look_up(Place::Code, &[Place::Anywhere], press.page) {
            Looked::Own(action) => {
                stage.read(cx).send(CodeGesture::Action { action }, cx);
                Verdict::Taken
            }
            Looked::Beyond(action) if run::run(action, window, cx) => Verdict::Taken,
            Looked::Beyond(_) => Verdict::Held,
            Looked::Held if extension::run(&[ANYWHERE], press.page, None, None, cx) => Verdict::Taken,
            Looked::Held => Verdict::Held,
        }
    }

    /// Escape in the stage is the stage's to answer, with or without the layer being up.
    fn escape(&self, _: &mut Window, cx: &mut App) -> bool {
        let Some(stage) = self.stage.upgrade() else { return false };
        let stage = stage.read(cx);
        if stage.heard.read(cx).rail.stage != Stage::Code {
            return false;
        }
        stage.send(CodeGesture::Action { action: Action::CloseLayer }, cx);
        true
    }
}
