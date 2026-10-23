//! The Git stage's layers of the key walk: the review's (Changes) and the history's (History), up
//! while the stage has the keys and its tab is the layer's, hearing the open tab's keys from the
//! session's bindings, the sideways scroll of the diff on screen, and the copy of what a drag
//! selected. A field of the stage has its own keys (`keys/field`), and is a layer above these.

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::keys::keymap::{Action, Layer, Place};
use gpui_kit::{App, Context, WeakEntity, Window};

use super::GitStage;
use crate::keys::layer::{action_for, Handler, Press, Verdict};
use crate::keys::run;
use crate::review::entry::Tab;
use crate::review::keys::{self, Press as Gesture};

/// One tab's layer.
pub struct StageKeys {
    pub stage: WeakEntity<GitStage>,
    pub layer: Layer,
}

impl StageKeys {
    fn tab(&self) -> Tab {
        if self.layer == Layer::History {
            Tab::History
        } else {
            Tab::Changes
        }
    }
}

impl Handler for StageKeys {
    fn open(&self, window: &Window, cx: &App) -> bool {
        self.stage.upgrade().is_some_and(|stage| {
            let stage = stage.read(cx);
            let heard = stage.heard.read(cx);
            heard.git_open() && stage.has_keys(window, cx) && heard.git.tab(heard.open()) == self.tab()
        })
    }

    /// Every key is held, so none is typed into the pane under the stage. The one key of
    /// `Anywhere` the dialog hears is the sheet's: `?` opens it over the dialog, as the page's
    /// `hear` does for a review or history; the rest of the place is not the dialog's (SHELL-KEYS-14).
    fn keys(&self, press: &Press, places: &[Place], window: &mut Window, cx: &mut App) -> Verdict {
        if action_for(places, press.page) == Some(Action::Shortcuts) {
            run::run(Action::Shortcuts, window, cx);
            return Verdict::Taken;
        }
        if let Some(stage) = self.stage.upgrade() {
            stage.update(cx, |stage, cx| stage.press(press, window, cx));
        }
        Verdict::Taken
    }
}

impl GitStage {
    /// A key while no field of the stage has the keys: the review's, or held.
    pub(super) fn press(&mut self, press: &Press, window: &mut Window, cx: &mut Context<Self>) {
        let (tab, reviewing, historical) = {
            let heard = self.heard.read(cx);
            (heard.git.tab(heard.open()), heard.review.is_some(), heard.history.is_some())
        };
        // Left and Right are no key of the review's or the history's: they
        // scroll the diff on screen sideways, as the page's native box does.
        let sideways = match press.keystroke.key.as_str() {
            "left" => -4.0,
            "right" => 4.0,
            _ => 0.0,
        };
        if sideways != 0.0 && !press.keystroke.modifiers.modified() {
            let diff = match tab {
                Tab::Changes => self.changes.read(cx).diff().clone(),
                Tab::History => self.history.read(cx).diff().clone(),
            };
            diff.update(cx, |diff, cx| diff.nudge(sideways, cx));
            return;
        }
        // Ctrl+C (Cmd+C) copies what a drag selected in the diff on screen;
        // with nothing selected it is no key of the review's.
        let keystroke = press.keystroke;
        if keystroke.key == "c" && (keystroke.modifiers.control || keystroke.modifiers.platform) && !keystroke.modifiers.alt {
            let copied = match tab {
                Tab::Changes => self.changes.update(cx, |changes, cx| changes.copy(cx)),
                Tab::History => self.history.update(cx, |history, cx| history.copy(cx)),
            };
            if copied {
                return;
            }
        }
        let key = press.page;
        // Each tab hears its own keys once the session has it open; before that
        // only Escape is heard, which shuts the stage as it does the tab.
        let pressed = match tab {
            Tab::Changes if reviewing => keys::review(key),
            Tab::History if historical => keys::history(key),
            _ => keys::review(key).filter(|pressed| matches!(pressed, Gesture::Close)),
        };
        // The Files tab steps through its own files rather than the diff's lines.
        if let Some(Gesture::Ask(Call::ReviewAction { action: action @ (Action::Next | Action::Previous) })) = &pressed {
            let forward = *action == Action::Next;
            if tab == Tab::Changes && self.changes.update(cx, |changes, cx| changes.step_files(forward, cx)) {
                return;
            }
        }
        match pressed {
            Some(Gesture::Ask(call)) => self.ask(call, cx),
            Some(Gesture::Close) => self.close(cx),
            // `/`: the page's own, nothing is sent (SHARED-9).
            Some(Gesture::Field) => self.filter.update(cx, |filter, cx| filter.focus(window, cx)),
            None => {}
        }
    }
}
