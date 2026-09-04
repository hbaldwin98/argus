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

/// A line being typed on one of the boards.
///
/// One line and no cursor movement beyond the end. A card's title is a
/// sentence somebody dictates to a board; anything that wants real editing
/// wants the feature document instead, which has an editor of its own.
///
/// Shared by both boards rather than one struct each: adding a feature and
/// adding a task are the same gesture on the same kind of surface, and two
/// of them would drift.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineInput {
    pub text: String,
    pub what: LineEdit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineEdit {
    NewTask,
    Task(i64),
    NewFeature,
    Feature(String),
}

impl LineInput {
    /// What the prompt calls itself, which is the whole of the affordance:
    /// there is no other cue that the keys have changed meaning.
    pub fn label(&self) -> &'static str {
        match self.what {
            LineEdit::NewTask => "new task",
            LineEdit::Task(_) => "rewrite",
            LineEdit::NewFeature => "new feature",
            LineEdit::Feature(_) => "rename",
        }
    }
}

/// How many columns the task board draws.
pub const TASK_COLUMNS: usize = argus_protocol::TaskState::ALL.len();

/// How many columns the board draws — one per state.
pub const COLUMNS: usize = argus_protocol::FeatureState::ALL.len();

impl App {
    /// The feature the tasks view is showing, which is whichever card the
    /// feature board is on. There is no task list at project scope: a
    /// task means nothing outside the feature it is under.
    pub fn tasks_feature(&self) -> Option<String> {
        self.selected_card()
            .map(|f| f.slug.clone())
            .or_else(|| self.current_feature_row().and_then(|row| row.slug))
    }

    pub(super) fn ask_for_tasks(&mut self) {
        let (Some(project), Some(feature)) = (
            self.board.as_ref().and_then(|b| b.project),
            self.tasks_feature(),
        ) else {
            self.tasks = None;
            return;
        };
        // The list on screen is another feature's until the answer lands,
        // and drawing it under this feature's name would be a lie.
        if self.tasks.as_ref().and_then(|l| l.feature.clone()).as_deref() != Some(feature.as_str()) {
            self.tasks = None;
        }
        let _ = self.out.send(ClientMsg::GetTasks { project, feature });
    }

    /// The tasks in one column, in the order a human put them in.
    pub fn task_column(&self, state: argus_protocol::TaskState) -> Vec<&argus_protocol::Task> {
        self.tasks
            .as_ref()
            .map(|l| l.tasks.iter().filter(|t| t.state == state).collect())
            .unwrap_or_default()
    }

    pub fn task_column_state(&self) -> argus_protocol::TaskState {
        argus_protocol::TaskState::ALL[self.task_column.min(TASK_COLUMNS - 1)]
    }

    pub fn selected_task(&self) -> Option<&argus_protocol::Task> {
        self.task_column(self.task_column_state())
            .get(self.task_card)
            .copied()
    }

    pub(super) fn clamp_task_selection(&mut self) {
        let count = self.task_column(self.task_column_state()).len();
        self.task_card = self.task_card.min(count.saturating_sub(1));
    }

    pub(super) fn move_task_column(&mut self, delta: i32) {
        let next = (self.task_column as i32)
            .saturating_add(delta)
            .clamp(0, TASK_COLUMNS as i32 - 1) as usize;
        if next != self.task_column {
            self.task_column = next;
            self.task_card = 0;
        }
    }

    pub(super) fn move_task_card(&mut self, delta: i32) {
        let count = self.task_column(self.task_column_state()).len();
        if count == 0 {
            self.task_card = 0;
            return;
        }
        self.task_card = (self.task_card as i32)
            .saturating_add(delta)
            .clamp(0, count as i32 - 1) as usize;
    }

    pub(super) fn select_task(&mut self, column: usize, row: usize) {
        if column >= TASK_COLUMNS {
            return;
        }
        self.task_column = column;
        let count = self.task_column(argus_protocol::TaskState::ALL[column]).len();
        self.task_card = row.min(count.saturating_sub(1));
    }

    /// Moves the selected task a column along, and follows it there.
    pub(super) fn move_selected_task(&mut self, delta: i32) {
        let next = (self.task_column as i32).saturating_add(delta);
        if next < 0 || next as usize >= TASK_COLUMNS {
            return;
        }
        let state = argus_protocol::TaskState::ALL[next as usize];
        let Some((project, feature, id)) = self.task_target() else {
            return;
        };
        let _ = self.out.send(ClientMsg::MoveTask {
            project,
            feature,
            id,
            state,
        });
        // Applied here as well as sent, so the card is under the cursor
        // before the push arrives; the push is what makes it true.
        if let Some(task) = self
            .tasks
            .as_mut()
            .and_then(|l| l.tasks.iter_mut().find(|t| t.id == id))
        {
            task.state = state;
        }
        self.task_column = next as usize;
        self.task_card = self
            .task_column(state)
            .iter()
            .position(|t| t.id == id)
            .unwrap_or(0);
    }

    pub(super) fn drop_selected_task(&mut self) {
        let Some((project, feature, id)) = self.task_target() else {
            return;
        };
        let _ = self.out.send(ClientMsg::RemoveTask {
            project,
            feature,
            id,
        });
    }

    /// Moves the selected task up or down its feature's list, which is
    /// what says to do it first.
    pub(super) fn reorder_selected_task(&mut self, delta: i64) {
        let Some(position) = self.selected_task().map(|t| t.position) else {
            return;
        };
        let Some((project, feature, id)) = self.task_target() else {
            return;
        };
        let last = self
            .tasks
            .as_ref()
            .map(|l| l.tasks.len() as i64 - 1)
            .unwrap_or(0);
        let to = (position + delta).clamp(0, last.max(0));
        if to == position {
            return;
        }
        let _ = self.out.send(ClientMsg::ReorderTask {
            project,
            feature,
            id,
            to,
        });
    }

    /// Starts a new task, typed into the line at the foot of the view.
    pub(super) fn begin_task(&mut self) {
        if self.tasks.as_ref().and_then(|l| l.feature.as_ref()).is_some() {
            self.begin_line(LineEdit::NewTask, String::new());
        }
    }

    /// Rewrites the selected task, starting from what it already says —
    /// a correction is almost always a few words off an existing line.
    pub(super) fn begin_task_edit(&mut self) {
        if let Some(task) = self.selected_task() {
            let (id, title) = (task.id, task.title.clone());
            self.begin_line(LineEdit::Task(id), title);
        }
    }

    /// Starts a feature, which needs no checkout and no agent: this is a
    /// person writing down work that has not begun.
    pub(super) fn begin_feature(&mut self) {
        if self.board.as_ref().and_then(|b| b.project).is_some() {
            self.begin_line(LineEdit::NewFeature, String::new());
        }
    }

    pub(super) fn begin_feature_rename(&mut self) {
        if let Some(feature) = self.selected_feature() {
            let (slug, title) = (feature.slug.clone(), feature.title.clone());
            self.begin_line(LineEdit::Feature(slug), title);
        }
    }

    fn begin_line(&mut self, what: LineEdit, text: String) {
        self.line = Some(LineInput { text, what });
    }

    pub(super) fn type_into_line(&mut self, c: char) {
        if let Some(input) = self.line.as_mut() {
            input.text.push(c);
        }
    }

    pub(super) fn backspace_line(&mut self) {
        if let Some(input) = self.line.as_mut() {
            input.text.pop();
        }
    }

    /// Sends what was typed. An empty line cancels rather than writing a
    /// card with no text, which the daemon would refuse anyway.
    pub(super) fn commit_line(&mut self) {
        let Some(input) = self.line.take() else {
            return;
        };
        let title = input.text.trim().to_string();
        if title.is_empty() {
            return;
        }
        let Some(project) = self.board.as_ref().and_then(|b| b.project) else {
            return;
        };
        let feature = self.tasks.as_ref().and_then(|l| l.feature.clone());
        let msg = match (input.what, feature) {
            (LineEdit::NewFeature, _) => ClientMsg::OpenFeature {
                project,
                write: argus_protocol::FeatureWrite {
                    title,
                    body: None,
                },
            },
            (LineEdit::Feature(slug), _) => ClientMsg::RenameFeature {
                project,
                slug,
                title,
            },
            (LineEdit::Task(id), Some(feature)) => ClientMsg::RetitleTask {
                project,
                feature,
                id,
                title,
            },
            (LineEdit::NewTask, Some(feature)) => ClientMsg::AddTask {
                project,
                feature,
                write: argus_protocol::TaskWrite {
                    title,
                    external: None,
                },
            },
            // A task with no feature to be under: the view cannot have
            // been open on one, so there is nothing to write.
            (LineEdit::Task(_) | LineEdit::NewTask, None) => return,
        };
        let _ = self.out.send(msg);
    }

    /// Removes the selected feature. Its decisions survive as unfiled; the
    /// daemon is where that rule lives, and the push is what redraws it.
    pub(super) fn drop_selected_feature(&mut self) {
        let (Some(project), Some(slug)) = (
            self.board.as_ref().and_then(|b| b.project),
            self.selected_feature().map(|f| f.slug.clone()),
        ) else {
            return;
        };
        let _ = self.out.send(ClientMsg::RemoveFeature { project, slug });
    }

    fn task_target(&self) -> Option<(argus_protocol::ProjectId, String, i64)> {
        let project = self.board.as_ref().and_then(|b| b.project)?;
        let feature = self.tasks.as_ref().and_then(|l| l.feature.clone())?;
        let id = self.selected_task()?.id;
        Some((project, feature, id))
    }
}

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
    /// One feature's tasks, in columns of their own: what is left to do,
    /// as against what is being built and why.
    Tasks,
}

impl View {
    /// Every view, in the order the tab strip draws them. The spine is
    /// first because it is the default and the one you return to.
    pub const ALL: [View; 4] = [View::Spine, View::Decisions, View::Board, View::Tasks];

    /// What the tab says. Short by intent: the strip is one row, and every
    /// cell it spends is a cell the view underneath could have used.
    pub fn label(self) -> &'static str {
        match self {
            View::Spine => "spine",
            View::Decisions => "decisions",
            View::Board => "board",
            View::Tasks => "tasks",
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
        if view == View::Tasks {
            self.ask_for_tasks();
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

    /// The brief of whichever feature is selected, whether that is a card
    /// on the board or a row in the decision view's feature column.
    pub fn selected_feature(&self) -> Option<&argus_protocol::Feature> {
        let slug = match self.view {
            View::Board => self.selected_card().map(|f| f.slug.clone()),
            _ => self.current_feature_row().and_then(|row| row.slug),
        }?;
        self.board
            .as_ref()?
            .features
            .iter()
            .find(|f| f.slug == slug)
    }

    /// Opens the selected feature's brief in the note editor.
    ///
    /// The same editor a note gets, because it is the same job: prose a
    /// human reads and corrects. A second editor for a second kind of
    /// document would only be a place for the two to drift apart.
    pub(super) fn open_feature_brief(&mut self) {
        let (Some(project), Some(feature)) = (
            self.board.as_ref().and_then(|b| b.project),
            self.selected_feature(),
        ) else {
            return self.report("no feature selected");
        };
        let view = crate::notes::NoteView::brief(
            project,
            &feature.slug,
            feature.title.clone(),
            &feature.body,
        );
        self.notes = Some(view);
        self.overlay = Some(Overlay::Notes);
        self.focus = Focus::Overlay;
    }

    /// Goes into the selected card: the tasks under that feature, which
    /// is what a reader who has found the card in flight wants next.
    ///
    /// The decisions are one tab away rather than behind this key. Both
    /// are worth reaching from a card, and the list of what is left to do
    /// is the one you act on.
    pub(super) fn open_selected_card(&mut self) {
        if self.selected_card().is_none() {
            return;
        }
        self.open_view(View::Tasks);
    }

    /// Opens the selected card's reasoning: the decisions view, already on
    /// that feature. The link the roadmap asks for, in the direction a
    /// reader actually goes — you see what is in flight, then ask why.
    pub(super) fn open_card_decisions(&mut self) {
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
