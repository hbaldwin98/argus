//! Which screen of the grid has GPUI's focus: the session says which cell has the keys, and the
//! focus follows it (SHELL-GRID-15).
//!
//! The Workspace's own screen is where the rest of the window hands the keys back to (the rail on
//! Enter, a dialog closing, the console letting its panes go), so a focus that lands on it while a
//! split has the keys is sent on to that split. A rail pick, a text field or a dialog that has the
//! keys keeps them: the focus moves only from where the panes already had it.

use gpui_kit::{Context, Entity, Subscription, Window};

use super::SplitView;
use crate::panes::terminal::Terminal;

/// The cell the focus was last put on, to move it only when the session moves the keys.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Keys(Option<u8>);

impl Keys {
    /// Notes that the keys are in `cell` now; whether that is a move.
    pub fn moved(&mut self, cell: u8) -> bool {
        let moved = self.0 != Some(cell);
        self.0 = Some(cell);
        moved
    }
}

/// Sends a focus that lands on the Workspace's screen to the cell that has the keys, if it is
/// another.
pub fn redirect(workspace: &Entity<Terminal>, window: &mut Window, cx: &mut Context<SplitView>) -> Subscription {
    let handle = workspace.read(cx).focus_handle().clone();
    cx.on_focus_in(&handle, window, |this, window, cx| {
        let heard = this.heard.read(cx);
        let cell = heard.keyed_cell().unwrap_or(0);
        if cell == 0 || heard.console.typing.is_some() {
            return;
        }
        if let Some(handle) = this.handle_of(cell, cx) {
            window.focus(&handle, cx);
        }
    })
}

impl SplitView {
    /// The session moved the keys to another cell: GPUI's focus goes with them, if it was in the
    /// panes (a cell's screen, or nowhere).
    pub(super) fn follow_keys(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (cell, typing) = {
            let heard = self.heard.read(cx);
            (heard.keyed_cell().unwrap_or(0), heard.console.typing.is_some())
        };
        if !self.keys.moved(cell) || typing {
            return;
        }
        let cells = 1 + self.beside.len();
        let held = window.focused(cx);
        let in_panes = held.as_ref().is_none_or(|held| (0..cells as u8).any(|n| self.handle_of(n, cx).as_ref() == Some(held)));
        if !in_panes {
            return;
        }
        if let Some(handle) = self.handle_of(cell, cx) {
            window.focus(&handle, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_focus_moves_when_the_keys_do_and_not_when_they_stay() {
        let mut keys = Keys::default();
        assert!(keys.moved(0), "the first word is a move");
        assert!(!keys.moved(0));
        assert!(keys.moved(2));
        assert!(!keys.moved(2));
        assert!(keys.moved(0));
    }
}
