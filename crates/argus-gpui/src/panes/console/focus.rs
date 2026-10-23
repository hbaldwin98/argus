//! Which screen of the console has the keys: the session says which side is typing, and GPUI's
//! focus follows it.
//!
//! The keys are the session's: its `typing` says which side has them, and the focus follows that,
//! from where the keys already were in the Workspace's pane, a console screen or an extension's
//! console view. A rail pick or a text field that has them keeps them.

use gpui_kit::{App, Context, FocusHandle, Window};

use super::ConsoleView;
use crate::extensions::bodies::Bodies;

impl ConsoleView {
    /// The handle of the screen that has the console's keys, if a console does.
    pub fn keyed_handle(&self, cx: &App) -> Option<FocusHandle> {
        let console = &self.heard.read(cx).console;
        let typing = console.typing?;
        if Some(typing) == console.showing {
            Some(self.first.read(cx).focus_handle().clone())
        } else if Some(typing) == console.beside {
            Some(self.second.read(cx).focus_handle().clone())
        } else {
            None
        }
    }

    /// The session moved the keys: GPUI's focus goes with them, if it was in
    /// the panes (the Workspace's or a console screen). A rail pick, a field
    /// or a dialog that has the keys keeps them.
    pub(super) fn follow_keys(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let keyed = self.keyed_handle(cx);
        if keyed == self.typed {
            return;
        }
        self.typed = keyed.clone();
        let (first, second) = (self.first.read(cx).focus_handle().clone(), self.second.read(cx).focus_handle().clone());
        let bodies = Bodies::of(cx);
        let in_panes = window.focused(cx).is_none_or(|held| held == self.stage || held == first || held == second || bodies.holds_screen(&held, cx));
        if in_panes {
            window.focus(&keyed.unwrap_or_else(|| self.stage.clone()), cx);
        }
    }
}
