//! Which pane a scroll gesture moves, and how a parked pane says so.
//!
//! What a gesture does to a pane's view — the wheel's decision between the
//! child and the history, where the view parks, what it asks the daemon —
//! is `argus_protocol::panes::view`'s, shared with the desktop window; this
//! hands it the gesture and carries out what it answers.

use super::*;
use argus_protocol::panes::view::Effect;

impl App {
    /// The wheel over `pane`, `notches` up when negative, at the cell `at`
    /// (row, column) of the pane under the pointer.
    pub(super) fn wheel_pane(&mut self, pane: PaneId, notches: i32, at: (u16, u16)) {
        if let Some(effect) = self.grids.get_mut(&pane).map(|view| view.wheel(notches, at)) {
            self.take_view_effect(effect);
        }
    }

    /// Moves `pane`'s view by whole screens, which is what Page Up/Down and
    /// the scrollback keys mean.
    pub(super) fn page_pane(&mut self, pane: PaneId, pages: i32) {
        if let Some(effect) = self.grids.get_mut(&pane).map(|view| view.page(pages)) {
            self.take_view_effect(effect);
        }
    }

    /// Drops `pane` back onto the live screen, if it was anywhere else.
    /// Called wherever the operator's attention returns to the present:
    /// typing, pasting, or leaving the pane.
    pub(super) fn scroll_to_live(&mut self, pane: PaneId) {
        if let Some(view) = self.grids.get_mut(&pane) {
            let _ = view.live();
        }
    }

    /// Sends what a pane's view asked and says what it said. Nothing is
    /// redrawn by hand: every frame draws the views whole.
    pub(super) fn take_view_effect(&mut self, effect: Effect) {
        if let Some(msg) = effect.send {
            let _ = self.out.send(msg);
        }
        if let Some(note) = effect.note {
            self.report(note);
        }
    }

    /// How a parked pane says so in its title: `↑ 120/4000`.
    pub fn scroll_indicator(&self) -> Option<String> {
        let parked = self.column_pane().and_then(|id| self.grids.get(&id))?.parked()?;
        Some(format!("↑ {}/{}", parked.offset, parked.depth))
    }
}
