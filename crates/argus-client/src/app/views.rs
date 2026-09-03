//! The client's top-level views.
//!
//! The spine — five columns and a live pane — was for a long time the only
//! thing the content area could hold. A decision tree and a work board are
//! read at project scope, all at once, and neither says anything useful in
//! a thirty-column strip beside a pane, so they want the screen rather
//! than a column (TARGET.md, "Product boundary").
//!
//! What a view replaces is the screen, never the running work: every pane
//! keeps running while another view is up, and the spine is one keystroke
//! back. Which view is open is this client's business and is not sent to
//! the daemon — two people attached to one daemon are not necessarily
//! reading the same thing.

use super::*;

/// Which top-level surface the content area is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Spine,
    Decisions,
}

impl View {
    /// Every view, in the order the tab strip draws them. The spine is
    /// first because it is the default and the one you return to.
    pub const ALL: [View; 2] = [View::Spine, View::Decisions];

    /// What the tab says. Short by intent: the strip is one row, and every
    /// cell it spends is a cell the view underneath could have used.
    pub fn label(self) -> &'static str {
        match self {
            View::Spine => "spine",
            View::Decisions => "decisions",
        }
    }

    /// The digit that opens this view, which is also its place in the
    /// strip. Numbered rather than cycled because a cycle key makes the
    /// second view cheap to reach and the fourth expensive, and because a
    /// tab strip that shows numbers teaches its own bindings.
    pub fn digit(self) -> char {
        let index = View::ALL.iter().position(|v| *v == self).unwrap_or(0);
        char::from_digit(index as u32 + 1, 10).unwrap_or('1')
    }

    pub fn from_digit(c: char) -> Option<View> {
        let index = c.to_digit(10)?.checked_sub(1)? as usize;
        View::ALL.get(index).copied()
    }
}

impl App {
    /// Opens a view, remembering where focus was on the spine so coming
    /// back does not cost you your place.
    ///
    /// Focus has to move: a view that is not the spine has no columns to
    /// move between, and leaving focus in a pane would send every key to
    /// the child of a pane that is no longer on screen.
    pub fn open_view(&mut self, view: View) {
        if self.view == view {
            return;
        }
        if self.view == View::Spine {
            self.spine_focus = self.focus;
        }
        self.view = view;
        self.focus = match view {
            View::Spine => self.spine_focus,
            _ => Focus::View,
        };
        self.leader_pending = false;
        if view == View::Decisions {
            self.ask_for_decisions();
        }
        self.report(view.label());
    }

    /// Asks for the board of the project the spine is on. Sent on opening
    /// the view and on `r`, because a client that attached after the last
    /// write has never been pushed one.
    pub(super) fn ask_for_decisions(&mut self) {
        let Some(project) = self.current_project().map(|p| p.id) else {
            self.board = None;
            return;
        };
        let _ = self.out.send(ClientMsg::GetDecisions { project });
    }

    /// Asks again when the board on screen is not the one the view should
    /// be showing.
    ///
    /// Opening the view is a fetch, but the view can be open before there
    /// is anything to fetch for: the client draws before the first tree
    /// arrives, and a workspace switch re-scopes the tree under whatever
    /// is open. Called on every tree, and cheap when nothing has moved —
    /// an adopted board carries the project's name, so the comparison
    /// fails exactly once per change.
    pub(super) fn refresh_board_if_stale(&mut self) {
        if self.view != View::Decisions {
            return;
        }
        let Some(name) = self.current_project().map(|p| p.name.clone()) else {
            self.board = None;
            return;
        };
        if self.board.as_ref().map(|b| b.name.as_str()) != Some(name.as_str()) {
            self.ask_for_decisions();
        }
    }

    /// The board as it is drawn: depth-first, with the topology needed to
    /// connect each row to the decisions around it.
    pub fn board_rows(&self) -> Vec<argus_protocol::DecisionTreeRow<'_>> {
        self.board
            .as_ref()
            .map(|b| b.tree_rows())
            .unwrap_or_default()
    }

    /// Selects the row a click landed on, ignoring a click past the last
    /// one: the empty space under a short board is not a row.
    pub(super) fn select_board_row(&mut self, row: usize) {
        if row < self.board_rows().len() {
            self.board_sel = row;
        }
    }

    pub(super) fn move_board_selection(&mut self, delta: i32) {
        let rows = self.board_rows().len();
        if rows == 0 {
            return;
        }
        let next = (self.board_sel as i32).saturating_add(delta).clamp(0, rows as i32 - 1);
        self.board_sel = next as usize;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_views_digit_is_its_place_in_the_strip() {
        for view in View::ALL {
            assert_eq!(View::from_digit(view.digit()), Some(view));
        }
        assert_eq!(View::from_digit('1'), Some(View::Spine));
    }

    #[test]
    fn a_digit_no_view_sits_on_opens_nothing() {
        assert_eq!(View::from_digit('0'), None);
        assert_eq!(View::from_digit('9'), None);
        assert_eq!(View::from_digit('x'), None);
    }
}
