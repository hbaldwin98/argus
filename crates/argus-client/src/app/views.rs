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

/// How many columns the board draws — one per state.
pub const COLUMNS: usize = argus_protocol::FeatureState::ALL.len();

/// One row of the feature column: a feature, or the one row that holds
/// whatever was decided before features existed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureRow {
    /// `None` for the unfiled row, which is not a feature and cannot be
    /// worked on — only read.
    pub slug: Option<String>,
    pub title: String,
    pub branch: Option<String>,
    pub decisions: usize,
}

/// Which top-level surface the content area is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Spine,
    Decisions,
    /// The features of the project in columns by state — what is in
    /// flight, rather than the reasoning under any one of them.
    Board,
}

impl View {
    /// Every view, in the order the tab strip draws them. The spine is
    /// first because it is the default and the one you return to.
    pub const ALL: [View; 3] = [View::Spine, View::Decisions, View::Board];

    /// What the tab says. Short by intent: the strip is one row, and every
    /// cell it spends is a cell the view underneath could have used.
    pub fn label(self) -> &'static str {
        match self {
            View::Spine => "spine",
            View::Decisions => "decisions",
            View::Board => "board",
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
        // Both views read the same pushed board, so both are worth a
        // fetch on opening: a client that attached after the last write
        // has never been pushed one.
        if matches!(view, View::Decisions | View::Board) {
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
        if !matches!(self.view, View::Decisions | View::Board) {
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

    /// The features the left column offers, with how many decisions each
    /// holds. Decisions from before features existed get a row of their
    /// own at the end rather than being hidden: a board that silently
    /// drops records is worse than one with an awkward row on it.
    pub fn feature_rows(&self) -> Vec<FeatureRow> {
        let Some(board) = self.board.as_ref() else {
            return Vec::new();
        };
        let mut rows: Vec<FeatureRow> = board
            .features
            .iter()
            .map(|f| FeatureRow {
                slug: Some(f.slug.clone()),
                title: f.title.clone(),
                branch: f.origin_branch.clone(),
                decisions: board.count_for(Some(&f.slug)),
            })
            .collect();
        let unfiled = board.count_for(None);
        if unfiled > 0 {
            rows.push(FeatureRow {
                slug: None,
                title: "before features".to_string(),
                branch: None,
                decisions: unfiled,
            });
        }
        rows
    }

    pub fn current_feature_row(&self) -> Option<FeatureRow> {
        self.feature_rows().get(self.board_feature_sel).cloned()
    }

    /// Narrows the board to the feature the left column is on. Called
    /// whenever either half changes, since the rows drawn borrow from it.
    pub(super) fn rescope_board(&mut self) {
        let rows = self.feature_rows();
        if self.board_feature_sel >= rows.len() {
            self.board_feature_sel = rows.len().saturating_sub(1);
        }
        let slug = rows
            .get(self.board_feature_sel)
            .and_then(|row| row.slug.clone());
        self.board_scoped = self
            .board
            .as_ref()
            .map(|board| board.scoped(slug.as_deref()));
        let count = self.board_rows().len();
        if self.board_sel >= count {
            self.board_sel = count.saturating_sub(1);
        }
    }

    /// The board as it is drawn: depth-first, with the topology needed to
    /// connect each row to the decisions around it. One feature's, because
    /// that is the only scope a decision tree means anything at.
    pub fn board_rows(&self) -> Vec<argus_protocol::DecisionTreeRow<'_>> {
        self.board_scoped
            .as_ref()
            .map(|b| b.tree_rows())
            .unwrap_or_default()
    }

    /// Selects the feature a click landed on, and moves the keys with it —
    /// a click that selected a feature but left `j` walking the old tree
    /// would answer half the gesture.
    pub(super) fn select_feature_row(&mut self, row: usize) {
        if row < self.feature_rows().len() {
            self.board_feature_sel = row;
            self.board_sel = 0;
            self.board_on_features = true;
            self.rescope_board();
        }
    }

    pub(super) fn move_feature_selection(&mut self, delta: i32) {
        let rows = self.feature_rows().len();
        if rows == 0 {
            return;
        }
        let next = (self.board_feature_sel as i32)
            .saturating_add(delta)
            .clamp(0, rows as i32 - 1);
        if next as usize != self.board_feature_sel {
            self.board_feature_sel = next as usize;
            self.board_sel = 0;
            self.rescope_board();
        }
    }

    /// Selects the row a click landed on, ignoring a click past the last
    /// one: the empty space under a short board is not a row.
    pub(super) fn select_board_row(&mut self, row: usize) {
        if row < self.board_rows().len() {
            self.board_sel = row;
        }
    }

    /// The features in one column of the board, oldest first — which is
    /// the order they were opened in, and so the order they were meant to
    /// be worked in.
    pub fn column_features(&self, state: argus_protocol::FeatureState) -> Vec<&argus_protocol::Feature> {
        self.board
            .as_ref()
            .map(|b| b.features.iter().filter(|f| f.state == state).collect())
            .unwrap_or_default()
    }

    pub fn board_column_state(&self) -> argus_protocol::FeatureState {
        argus_protocol::FeatureState::ALL[self.board_column.min(COLUMNS - 1)]
    }

    pub fn selected_card(&self) -> Option<&argus_protocol::Feature> {
        let column = self.column_features(self.board_column_state());
        column.get(self.board_card).copied()
    }

    /// Moves between columns, keeping the card selection in range.
    ///
    /// The row is not remembered per column. Carrying it across would put
    /// the selection on whatever happens to sit at the same depth of an
    /// unrelated column, which reads as a jump rather than a move; landing
    /// on the first card of the column you moved to is at least where you
    /// were looking.
    pub(super) fn move_board_column(&mut self, delta: i32) {
        let next = (self.board_column as i32)
            .saturating_add(delta)
            .clamp(0, COLUMNS as i32 - 1) as usize;
        if next != self.board_column {
            self.board_column = next;
            self.board_card = 0;
        }
    }

    pub(super) fn move_board_card(&mut self, delta: i32) {
        let count = self.column_features(self.board_column_state()).len();
        if count == 0 {
            self.board_card = 0;
            return;
        }
        self.board_card = (self.board_card as i32)
            .saturating_add(delta)
            .clamp(0, count as i32 - 1) as usize;
    }

    pub(super) fn select_card(&mut self, column: usize, row: usize) {
        if column >= COLUMNS {
            return;
        }
        self.board_column = column;
        let count = self
            .column_features(argus_protocol::FeatureState::ALL[column])
            .len();
        // A click on the empty space under a column's cards is a click on
        // the column, not on a card that is not there.
        self.board_card = row.min(count.saturating_sub(1));
    }

    /// Moves the selected card one column along, and follows it there.
    ///
    /// Following is the point: a move you have to go looking for reads as
    /// having lost the card. The selection is set optimistically and the
    /// pushed board is what actually redraws it — if the daemon refuses,
    /// the next push puts the card back where it really is.
    pub(super) fn move_selected_card(&mut self, delta: i32) {
        let Some(project) = self.board.as_ref().and_then(|b| b.project) else {
            return;
        };
        let Some(slug) = self.selected_card().map(|f| f.slug.clone()) else {
            return;
        };
        let next = (self.board_column as i32).saturating_add(delta);
        if next < 0 || next as usize >= COLUMNS {
            return;
        }
        self.send_card_to(project, slug, next as usize);
    }

    /// Sends the selected card back to whoever is working on it.
    ///
    /// Its own key because it is the one human verb the layout does not
    /// teach: accepting is a step right from `submitted`, but sending back
    /// is two columns left, and stepping through `blocked` on the way
    /// would post a blocker nobody claimed.
    pub(super) fn send_selected_card_back(&mut self) {
        let Some(project) = self.board.as_ref().and_then(|b| b.project) else {
            return;
        };
        let Some(slug) = self.selected_card().map(|f| f.slug.clone()) else {
            return;
        };
        let active = argus_protocol::FeatureState::ALL
            .iter()
            .position(|s| *s == argus_protocol::FeatureState::Active)
            .unwrap_or(0);
        self.send_card_to(project, slug, active);
    }

    fn send_card_to(&mut self, project: argus_protocol::ProjectId, slug: String, column: usize) {
        let state = argus_protocol::FeatureState::ALL[column];
        let _ = self.out.send(ClientMsg::MoveFeature {
            project,
            slug: slug.clone(),
            state,
            detail: None,
        });
        self.board_column = column;
        if let Some(feature) = self
            .board
            .as_mut()
            .and_then(|b| b.features.iter_mut().find(|f| f.slug == slug))
        {
            feature.state = state;
        }
        self.board_card = self
            .column_features(state)
            .iter()
            .position(|f| f.slug == slug)
            .unwrap_or(0);
        self.report(format!("{slug} → {state}"));
    }

    /// Opens the selected card's reasoning: the decisions view, already on
    /// that feature. The link the roadmap asks for, in the direction a
    /// reader actually goes — you see what is in flight, then ask why.
    pub(super) fn open_selected_card(&mut self) {
        let Some(slug) = self.selected_card().map(|f| f.slug.clone()) else {
            return;
        };
        self.open_view(View::Decisions);
        if let Some(row) = self.feature_rows().iter().position(|r| r.slug.as_deref() == Some(slug.as_str())) {
            self.select_feature_row(row);
        }
    }

    /// Moves whichever half of the board has the keys.
    pub(super) fn move_in_board(&mut self, delta: i32) {
        if self.board_on_features {
            self.move_feature_selection(delta);
        } else {
            self.move_board_selection(delta);
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
