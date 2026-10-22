//! The keys the stage hears while no field of it has them: the open tab's, read from the session's
//! bindings, the sideways scroll of the diff on screen, and the copy of what a drag selected.

use gpui_kit::{Context, KeyDownEvent, Window};

use super::GitStage;
use crate::keys::page_key;
use crate::review::entry::Tab;
use crate::review::keys::{self, Press};

impl GitStage {
    /// A key while no field of the stage has the keys: the review's, or held.
    pub(super) fn press(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // A field's keys are its own: the press reaches here from it too.
        if !self.focus.is_focused(window) {
            return;
        }
        cx.stop_propagation();
        let (tab, reviewing, historical) = {
            let heard = self.heard.read(cx);
            (heard.git.tab(heard.open()), heard.review.is_some(), heard.history.is_some())
        };
        // Left and Right are no key of the review's or the history's: they
        // scroll the diff on screen sideways, as the page's native box does.
        let sideways = match event.keystroke.key.as_str() {
            "left" => -4.0,
            "right" => 4.0,
            _ => 0.0,
        };
        if sideways != 0.0 && !event.keystroke.modifiers.modified() {
            let diff = match tab {
                Tab::Changes => self.changes.read(cx).diff().clone(),
                Tab::History => self.history.read(cx).diff().clone(),
            };
            diff.update(cx, |diff, cx| diff.nudge(sideways, cx));
            return;
        }
        // Ctrl+C (Cmd+C) copies what a drag selected in the diff on screen;
        // with nothing selected it is no key of the review's.
        let keystroke = &event.keystroke;
        if keystroke.key == "c" && (keystroke.modifiers.control || keystroke.modifiers.platform) && !keystroke.modifiers.alt {
            let copied = match tab {
                Tab::Changes => self.changes.update(cx, |changes, cx| changes.copy(cx)),
                Tab::History => self.history.update(cx, |history, cx| history.copy(cx)),
            };
            if copied {
                return;
            }
        }
        let Some(key) = page_key::from_keystroke(&event.keystroke) else { return };
        // Each tab hears its own keys once the session has it open; before that
        // only Escape is heard, which shuts the stage as it does the tab.
        let pressed = match tab {
            Tab::Changes if reviewing => keys::review(&key),
            Tab::History if historical => keys::history(&key),
            _ => keys::review(&key).filter(|press| matches!(press, Press::Close)),
        };
        match pressed {
            Some(Press::Ask(call)) => self.ask(call, cx),
            Some(Press::Close) => self.close(cx),
            // `/`: the page's own, nothing is sent (SHARED-9).
            Some(Press::Field) => self.filter.update(cx, |filter, cx| filter.focus(window, cx)),
            None => {}
        }
    }
}
