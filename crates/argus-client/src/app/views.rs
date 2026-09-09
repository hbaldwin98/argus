//! The client's top-level views.
//!
//! The spine — five columns and a live pane — was for a long time the only
//! thing the content area could hold. A feature is read at project scope,
//! all at once, and says nothing useful in a thirty-column strip beside a
//! pane, so it wants the screen rather than a column (TARGET.md, "Product
//! boundary").
//!
//! There is one feature view and not three. A brief, the tasks left under
//! it and the reasoning behind it are one object read together: which
//! feature you are on is a single selection, and the three used to be
//! three views with three selections that could disagree about it. They
//! did — pressing the tasks view's digit from the decisions view showed
//! whichever card the board happened to be sitting on.
//!
//! What a view replaces is the screen, never the running work: every pane
//! keeps running while another view is up, and the spine is one keystroke
//! back. Which view is open is this client's business and is not sent to
//! the daemon — two people attached to one daemon are not necessarily
//! reading the same thing.

use argus_protocol::{FeatureState, PaneKind, PaneStatus, TaskState};

use super::*;

/// A line being typed in the feature view.
///
/// One line and no cursor movement beyond the end. A feature's title and a
/// task's are sentences somebody dictates; anything that wants real
/// editing wants the brief instead, which has an editor of its own.
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

/// Which panel of the feature view the keys are in.
///
/// Three panels, one cursor. `h` and `l` cross between the feature list
/// and the feature being read, the same gesture the spine's columns take,
/// and `Tab` steps through the panels in the order they are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FeaturePanel {
    /// The features of the project, down the left.
    #[default]
    Features,
    /// What is left to do under the selected feature.
    Tasks,
    /// Why it has the shape it does.
    Decisions,
}

impl FeaturePanel {
    pub const ALL: [FeaturePanel; 3] = [
        FeaturePanel::Features,
        FeaturePanel::Tasks,
        FeaturePanel::Decisions,
    ];

    fn step(self, delta: i32) -> FeaturePanel {
        let index = FeaturePanel::ALL
            .iter()
            .position(|p| *p == self)
            .unwrap_or(0) as i32;
        let next = (index + delta).rem_euclid(FeaturePanel::ALL.len() as i32) as usize;
        FeaturePanel::ALL[next]
    }
}

impl App {
    // ---- the feature the whole view is about -------------------------

    /// The features the left column offers. Decisions from before features
    /// existed get a row of their own at the end rather than being hidden:
    /// a record that is silently dropped is worse than an awkward row.
    pub fn feature_rows(&self) -> Vec<FeatureRow> {
        let Some(board) = self.board.as_ref() else {
            return Vec::new();
        };
        let mut rows: Vec<FeatureRow> = board
            .features
            .iter()
            .map(|f| {
                let (detail, attention) = self.feature_detail(f, board.count_for(Some(&f.slug)));
                FeatureRow {
                    slug: Some(f.slug.clone()),
                    title: f.title.clone(),
                    detail,
                    attention,
                    done: f.state == FeatureState::Done,
                }
            })
            .collect();
        let unfiled = board.count_for(None);
        if unfiled > 0 {
            rows.push(FeatureRow {
                slug: None,
                title: "before features".to_string(),
                detail: format!("{unfiled} decided"),
                attention: None,
                done: false,
            });
        }
        rows
    }

    /// What a feature row says about itself, and whether it is asking for
    /// somebody.
    ///
    /// Every part of it is observed rather than maintained. The agents are
    /// the ones actually running on the checkouts this feature is worked
    /// in, the counts are its own tasks, and the branch is what the row
    /// falls back to when nothing is happening yet. The five board columns
    /// this replaced said only what somebody last dragged, which is why
    /// they were always a little bit wrong.
    fn feature_detail(
        &self,
        feature: &argus_protocol::Feature,
        decisions: usize,
    ) -> (String, Option<PaneStatus>) {
        let mut parts = Vec::new();
        if feature.state == FeatureState::Done {
            parts.push("done".to_string());
        }
        let (live, attention) = self.agents_on(feature);
        if let Some(live) = live {
            parts.push(live);
        }
        if feature.tasks.total() > 0 {
            parts.push(format!(
                "{}/{} tasks",
                feature.tasks.done,
                feature.tasks.total()
            ));
        }
        if decisions > 0 {
            parts.push(format!("{decisions} decided"));
        }
        // Only when there is nothing else to say. A branch is where to
        // look for work nobody has started; once something is happening
        // on it, what is happening is the more useful line.
        if parts.is_empty() {
            match &feature.origin_branch {
                Some(branch) => parts.push(branch.clone()),
                None => parts.push("nothing running".to_string()),
            }
        }
        (parts.join(" · "), attention)
    }

    /// The agent panes running in this feature's checkouts, summarized.
    ///
    /// Reported as the one thing worth knowing rather than a tally of all
    /// of them: a feature with an agent stopped on a question and two
    /// others working is a feature somebody has to go to.
    fn agents_on(&self, feature: &argus_protocol::Feature) -> (Option<String>, Option<PaneStatus>) {
        let mut live: Vec<&argus_protocol::PaneInfo> = Vec::new();
        for checkout in checkouts_in(&self.tree) {
            if !feature.checkouts.iter().any(|c| c == &checkout.path) {
                continue;
            }
            live.extend(
                checkout
                    .panes
                    .iter()
                    .filter(|p| p.kind == PaneKind::Agent)
                    .filter(|p| !matches!(p.status, PaneStatus::Exited { .. })),
            );
        }
        let Some(worst) = live.iter().max_by_key(|p| p.status.urgency()) else {
            return (None, None);
        };
        let note = |pane: &argus_protocol::PaneInfo| {
            pane.note
                .as_ref()
                .map(|n| format!(": {n}"))
                .unwrap_or_default()
        };
        let line = match worst.status {
            PaneStatus::Waiting => format!("waiting{}", note(worst)),
            PaneStatus::Failed => format!("failed{}", note(worst)),
            PaneStatus::NeedsReview => "needs review".to_string(),
            _ => {
                let working = live
                    .iter()
                    .filter(|p| p.status == PaneStatus::Working)
                    .count();
                match working {
                    0 => format!("{} idle", live.len()),
                    n => format!("{n} working"),
                }
            }
        };
        (Some(line), worst.status.needs_you().then_some(worst.status))
    }

    pub fn current_feature_row(&self) -> Option<FeatureRow> {
        self.feature_rows().get(self.feature_sel).cloned()
    }

    /// The slug the whole view is scoped to — the tasks it lists, the
    /// decisions it draws, and the brief above them. One answer, so the
    /// three panels cannot disagree about which feature you are reading.
    pub fn feature_slug(&self) -> Option<String> {
        self.current_feature_row().and_then(|row| row.slug)
    }

    pub fn selected_feature(&self) -> Option<&argus_protocol::Feature> {
        let slug = self.feature_slug()?;
        self.board.as_ref()?.features.iter().find(|f| f.slug == slug)
    }

    /// Narrows the decisions and asks for the tasks of whichever feature
    /// is selected. Called whenever the selection or the board changes,
    /// since everything the right-hand side draws hangs off it.
    pub(super) fn rescope_feature(&mut self) {
        let rows = self.feature_rows();
        if self.feature_sel >= rows.len() {
            self.feature_sel = rows.len().saturating_sub(1);
        }
        let slug = rows.get(self.feature_sel).and_then(|row| row.slug.clone());
        self.board_scoped = self
            .board
            .as_ref()
            .map(|board| board.scoped(slug.as_deref()));
        let count = self.board_rows().len();
        if self.decision_sel >= count {
            self.decision_sel = count.saturating_sub(1);
        }
        self.ask_for_tasks();
        self.clamp_task_selection();
    }

    /// Selects the feature a click landed on, and moves the keys with it —
    /// a click that selected a feature but left `j` walking the old list
    /// would answer half the gesture.
    pub(super) fn select_feature_row(&mut self, row: usize) {
        if row < self.feature_rows().len() {
            self.feature_sel = row;
            self.decision_sel = 0;
            self.task_sel = 0;
            self.panel = FeaturePanel::Features;
            self.rescope_feature();
        }
    }

    pub(super) fn move_feature_selection(&mut self, delta: i32) {
        let rows = self.feature_rows().len();
        if rows == 0 {
            return;
        }
        let next = (self.feature_sel as i32)
            .saturating_add(delta)
            .clamp(0, rows as i32 - 1);
        if next as usize != self.feature_sel {
            self.feature_sel = next as usize;
            self.decision_sel = 0;
            self.task_sel = 0;
            self.rescope_feature();
        }
    }

    // ---- panels ------------------------------------------------------

    /// Crosses to another panel. `Tab` steps forward through them and
    /// `h`/`l` cross between the list and what is being read, which is the
    /// gesture the spine already teaches.
    pub(super) fn step_panel(&mut self, delta: i32) {
        self.panel = self.panel.step(delta);
    }

    pub(super) fn go_to_panel(&mut self, panel: FeaturePanel) {
        self.panel = panel;
    }

    /// Steps rightward into the feature under the cursor: from the list
    /// into its tasks, and from its tasks into its reasoning. It stops at
    /// the last panel rather than wrapping, because `l` is a direction.
    pub(super) fn enter_feature(&mut self) {
        if self.panel != FeaturePanel::Decisions {
            self.step_panel(1);
        }
    }

    /// Rewrites whatever has the keys. On the list that is the brief,
    /// which is prose and opens in the note editor; on the tasks it is the
    /// line under the cursor. The decision tree is append-only, so there
    /// is nothing here to rewrite — a decision that turned out wrong is
    /// superseded by a new one rather than edited.
    pub(super) fn edit_in_feature(&mut self) {
        match self.panel {
            FeaturePanel::Features => self.open_feature_brief(),
            FeaturePanel::Tasks => self.begin_task_edit(),
            FeaturePanel::Decisions => {}
        }
    }

    pub(super) fn add_in_feature(&mut self) {
        match self.panel {
            FeaturePanel::Features => self.begin_feature(),
            FeaturePanel::Tasks => self.begin_task(),
            FeaturePanel::Decisions => self.report("decisions are recorded by agents"),
        }
    }

    pub(super) fn drop_in_feature(&mut self) {
        match self.panel {
            FeaturePanel::Features => self.drop_selected_feature(),
            FeaturePanel::Tasks => self.drop_selected_task(),
            // Nothing is ever removed from the board. A decision a later
            // finding invalidates is superseded, because the road not
            // taken is most of what a reader came back for.
            FeaturePanel::Decisions => self.report("the decision board is append-only"),
        }
    }

    /// Re-asks for everything the view draws. One key, because the brief,
    /// the tasks and the decisions are one thing being read.
    pub(super) fn refresh_feature(&mut self) {
        self.ask_for_decisions();
        self.ask_for_tasks();
    }

    /// Moves the cursor in whichever panel has the keys.
    pub(super) fn move_in_feature(&mut self, delta: i32) {
        match self.panel {
            FeaturePanel::Features => self.move_feature_selection(delta),
            FeaturePanel::Tasks => self.move_task_selection(delta),
            FeaturePanel::Decisions => self.move_decision_selection(delta),
        }
    }

    // ---- tasks -------------------------------------------------------

    /// The selected feature's tasks, in the order a person put them in.
    ///
    /// One list rather than three columns. A task's state is a mark on its
    /// row, so the order stays what it is for — what to do first — instead
    /// of being spent on saying the same thing three columns already do.
    pub fn feature_tasks(&self) -> &[argus_protocol::Task] {
        self.tasks
            .as_ref()
            .filter(|list| list.feature == self.feature_slug())
            .map(|list| list.tasks.as_slice())
            .unwrap_or_default()
    }

    pub fn selected_task(&self) -> Option<&argus_protocol::Task> {
        self.feature_tasks().get(self.task_sel)
    }

    pub(super) fn ask_for_tasks(&mut self) {
        let (Some(project), Some(feature)) = (
            self.board.as_ref().and_then(|b| b.project),
            self.feature_slug(),
        ) else {
            self.tasks = None;
            return;
        };
        let _ = self.out.send(ClientMsg::GetTasks { project, feature });
    }

    pub(super) fn clamp_task_selection(&mut self) {
        self.task_sel = self
            .task_sel
            .min(self.feature_tasks().len().saturating_sub(1));
    }

    pub(super) fn move_task_selection(&mut self, delta: i32) {
        let count = self.feature_tasks().len();
        if count == 0 {
            self.task_sel = 0;
            return;
        }
        self.task_sel = (self.task_sel as i32)
            .saturating_add(delta)
            .clamp(0, count as i32 - 1) as usize;
    }

    pub(super) fn select_task(&mut self, row: usize) {
        let count = self.feature_tasks().len();
        if count > 0 {
            self.task_sel = row.min(count - 1);
            self.panel = FeaturePanel::Tasks;
        }
    }

    /// Moves the selected task along todo → doing → done, or back.
    ///
    /// Only while the tasks have the keys. A capital `H` on the feature
    /// list would otherwise move a task the cursor is nowhere near.
    pub(super) fn move_selected_task(&mut self, delta: i32) {
        if self.panel != FeaturePanel::Tasks {
            return;
        }
        let Some(current) = self.selected_task().map(|t| t.state) else {
            return;
        };
        let at = TaskState::ALL.iter().position(|s| *s == current).unwrap_or(0) as i32;
        let next = at.saturating_add(delta);
        if next < 0 || next as usize >= TaskState::ALL.len() {
            return;
        }
        let state = TaskState::ALL[next as usize];
        let Some((project, feature, id)) = self.task_target() else {
            return;
        };
        let _ = self.out.send(ClientMsg::MoveTask {
            project,
            feature,
            id,
            state,
        });
        // Applied here as well as sent, so the mark under the cursor
        // changes at once; the push is what makes it true.
        if let Some(task) = self
            .tasks
            .as_mut()
            .and_then(|l| l.tasks.iter_mut().find(|t| t.id == id))
        {
            task.state = state;
        }
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
        if self.panel != FeaturePanel::Tasks {
            return;
        }
        let Some(position) = self.selected_task().map(|t| t.position) else {
            return;
        };
        let Some((project, feature, id)) = self.task_target() else {
            return;
        };
        let last = self.feature_tasks().len() as i64 - 1;
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
        if self.feature_slug().is_some() {
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

    fn task_target(&self) -> Option<(argus_protocol::ProjectId, String, i64)> {
        let project = self.board.as_ref().and_then(|b| b.project)?;
        let feature = self.feature_slug()?;
        let id = self.selected_task()?.id;
        Some((project, feature, id))
    }

    // ---- decisions ---------------------------------------------------

    /// The decision tree as it is drawn: depth-first, with the topology
    /// needed to connect each row to the decisions around it. One
    /// feature's, because that is the only scope a tree means anything at.
    pub fn board_rows(&self) -> Vec<argus_protocol::DecisionTreeRow<'_>> {
        self.board_scoped
            .as_ref()
            .map(|b| b.tree_rows())
            .unwrap_or_default()
    }

    /// Selects the row a click landed on, ignoring a click past the last
    /// one: the empty space under a short tree is not a row.
    pub(super) fn select_decision_row(&mut self, row: usize) {
        if row < self.board_rows().len() {
            self.decision_sel = row;
            self.panel = FeaturePanel::Decisions;
        }
    }

    pub(super) fn move_decision_selection(&mut self, delta: i32) {
        let rows = self.board_rows().len();
        if rows == 0 {
            return;
        }
        self.decision_sel = (self.decision_sel as i32)
            .saturating_add(delta)
            .clamp(0, rows as i32 - 1) as usize;
    }

    // ---- writing -----------------------------------------------------

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
    /// row with no text, which the daemon would refuse anyway.
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
        let feature = self.feature_slug();
        let msg = match (input.what, feature) {
            (LineEdit::NewFeature, _) => ClientMsg::OpenFeature {
                project,
                write: argus_protocol::FeatureWrite { title, body: None },
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
            // been on one, so there is nothing to write.
            (LineEdit::Task(_) | LineEdit::NewTask, None) => return,
        };
        let _ = self.out.send(msg);
    }

    /// Removes the selected feature. Its decisions survive as unfiled; the
    /// daemon is where that rule lives, and the push is what redraws it.
    pub(super) fn drop_selected_feature(&mut self) {
        let (Some(project), Some(slug)) = (
            self.board.as_ref().and_then(|b| b.project),
            self.feature_slug(),
        ) else {
            return;
        };
        let _ = self.out.send(ClientMsg::RemoveFeature { project, slug });
    }

    /// Accepts the selected feature, or reopens one already accepted.
    ///
    /// The only state anyone sets by hand, and the human's alone: an agent
    /// cannot accept its own work. Everything else a row says about a
    /// feature is read off the panes running on it and the state of its
    /// tasks, which is why there is one key here and not five columns.
    pub(super) fn toggle_selected_feature_done(&mut self) {
        let (Some(project), Some(feature)) = (
            self.board.as_ref().and_then(|b| b.project),
            self.selected_feature(),
        ) else {
            return;
        };
        let slug = feature.slug.clone();
        let state = match feature.state {
            FeatureState::Done => FeatureState::Open,
            FeatureState::Open => FeatureState::Done,
        };
        let _ = self.out.send(ClientMsg::MoveFeature {
            project,
            slug: slug.clone(),
            state,
            detail: None,
        });
        // Applied to this client's copy at once so the row answers the
        // key; the pushed board is what actually makes it true, so a
        // refusal puts it back to what it really is.
        if let Some(feature) = self
            .board
            .as_mut()
            .and_then(|b| b.features.iter_mut().find(|f| f.slug == slug))
        {
            feature.state = state;
        }
        self.report(format!("{slug} → {state}"));
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
}

/// One row of the feature column.
///
/// The detail line is built where the tree is in reach rather than in the
/// renderer, because most of what it says is about panes rather than about
/// the feature: what a row is worth reading is what is happening to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureRow {
    /// `None` for the unfiled row, which is not a feature and cannot be
    /// worked on — only read.
    pub slug: Option<String>,
    pub title: String,
    pub detail: String,
    /// Set when an agent on this feature has stopped for a person. Drawn
    /// in the attention colour, since a list of features is a list of
    /// places work might be stuck.
    pub attention: Option<PaneStatus>,
    pub done: bool,
}

/// Which top-level surface the content area is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Spine,
    /// The project's features, and whichever one is selected read whole:
    /// its brief, what is left to do under it, and why it has the shape it
    /// does.
    Feature,
}

impl View {
    /// Every view, in the order the tab strip draws them. The spine is
    /// first because it is the default and the one you return to.
    pub const ALL: [View; 2] = [View::Spine, View::Feature];

    /// What the tab says. Short by intent: the strip is one row, and every
    /// cell it spends is a cell the view underneath could have used.
    pub fn label(self) -> &'static str {
        match self {
            View::Spine => "spine",
            View::Feature => "features",
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
        // A client that attached after the last write has never been
        // pushed a board, so opening the view is a fetch.
        if view == View::Feature {
            self.ask_for_decisions();
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
    /// is anything to fetch for: the client draws before the first board
    /// arrives, and a workspace switch re-scopes the tree under whatever
    /// is open. Called on every tree, and cheap when nothing has moved —
    /// an adopted board carries the project's name, so the comparison
    /// fails exactly once per change.
    pub(super) fn refresh_board_if_stale(&mut self) {
        if self.view != View::Feature {
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
        assert_eq!(View::from_digit('3'), None);
        assert_eq!(View::from_digit('x'), None);
    }

    #[test]
    fn tab_steps_through_the_panels_and_wraps() {
        assert_eq!(FeaturePanel::Features.step(1), FeaturePanel::Tasks);
        assert_eq!(FeaturePanel::Tasks.step(1), FeaturePanel::Decisions);
        assert_eq!(FeaturePanel::Decisions.step(1), FeaturePanel::Features);
        assert_eq!(FeaturePanel::Features.step(-1), FeaturePanel::Decisions);
    }
}
