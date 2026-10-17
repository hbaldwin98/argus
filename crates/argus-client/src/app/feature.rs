//! Where the Feature stage is — the index of features, one feature on one
//! of its lenses, the row under the cursor — and how the keys move between
//! them.
//!
//! A feature is one object read three ways: the work left under it, the
//! record of why it has its shape, and how it flows. Each is a lens over the
//! same selection rather than a view with a selection of its own, which is
//! what three views used to be, and they disagreed about which feature was
//! on screen.

use std::collections::HashSet;

use argus_protocol::{
    Decision, DecisionTreeRow, Feature, Folding, PaneInfo, SequenceDiagram, Task, TaskTreeRow,
};

use super::*;
use argus_protocol::board::diagrams::flow::FlowRow;
use argus_protocol::board::outline::{Outline, WorkRef};

/// One way of reading the open feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lens {
    /// The tasks, and the decisions they raised.
    #[default]
    Work,
    /// Every decision, in the order it was made, read as a record.
    Record,
    /// The sequence diagrams, step by step.
    Flows,
}

impl Lens {
    pub const ALL: [Lens; 3] = [Lens::Work, Lens::Record, Lens::Flows];

    pub fn label(self) -> &'static str {
        match self {
            Lens::Work => "work",
            Lens::Record => "record",
            Lens::Flows => "flows",
        }
    }

    pub fn digit(self) -> char {
        match self {
            Lens::Work => '1',
            Lens::Record => '2',
            Lens::Flows => '3',
        }
    }
}

pub use argus_protocol::board::standing::{FeatureRow, Standing, GROUPS};

/// What a click on the stage landed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureHit {
    /// A feature in the index, as a place in `feature_rows`.
    Feature(usize),
    Lens(Lens),
    /// A flow in the flows lens's list.
    Flow(usize),
    /// A row of whatever the open lens lists.
    Row(usize),
}

impl App {
    // ---- the features -------------------------------------------------

    /// The features the index lists, grouped by how they stand. Held and
    /// archived ones are folded away unless asked for.
    pub fn feature_rows(&self) -> Vec<FeatureRow> {
        self.board_follower.listed(&self.tree, &Folding { show_folded: self.show_folded_features })
    }

    /// The live agent panes on the checkouts this feature is worked in.
    pub fn panes_on(&self, feature: &Feature) -> Vec<&PaneInfo> {
        argus_protocol::board::standing::panes_on(feature, &self.tree)
    }

    pub fn current_feature_row(&self) -> Option<FeatureRow> {
        self.feature_rows().get(self.feature_sel).cloned()
    }

    /// The slug the whole stage is scoped to. One answer, so the lenses
    /// cannot disagree about which feature is being read.
    pub fn feature_slug(&self) -> Option<String> {
        self.current_feature_row().map(|row| row.slug)
    }

    pub fn selected_feature(&self) -> Option<&Feature> {
        let slug = self.feature_slug()?;
        self.board_follower
            .board()?
            .features
            .iter()
            .find(|f| f.slug == slug)
    }

    /// Narrows the decisions to whichever feature is selected and points
    /// the follower at it, which asks for its lists when it moved. Called
    /// whenever the selection or the board changes, since everything the
    /// stage draws hangs off it.
    pub(crate) fn rescope_feature(&mut self) {
        let rows = self.feature_rows();
        if self.feature_sel >= rows.len() {
            self.feature_sel = rows.len().saturating_sub(1);
        }
        if rows.is_empty() {
            self.feature_open = false;
        }
        let slug = rows.get(self.feature_sel).map(|row| row.slug.clone());
        let mut out = Vec::new();
        self.board_follower.point(slug.as_deref(), &mut out);
        self.send_board_requests(out);
        self.board_scoped = self
            .board_follower
            .board()
            .map(|board| board.scoped(slug.as_deref()));
        self.clamp_feature_cursors();
    }

    /// Puts the index's cursor on the feature the follower settled on
    /// after its list changed, held by slug rather than by place.
    pub(super) fn land_on_feature(&mut self) {
        let landed = self.board_follower.feature().map(str::to_string);
        if let Some(at) = landed.and_then(|slug| self.feature_rows().iter().position(|row| row.slug == slug)) {
            self.feature_sel = at;
        }
        self.rescope_feature();
    }

    /// Selects the feature a click landed on.
    pub(super) fn select_feature_row(&mut self, row: usize) {
        if row < self.feature_rows().len() && row != self.feature_sel {
            self.feature_sel = row;
            self.reset_feature_cursors();
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
            .clamp(0, rows as i32 - 1) as usize;
        self.select_feature_row(next);
    }

    fn reset_feature_cursors(&mut self) {
        self.work_sel = 0;
        self.decision_sel = 0;
        self.diagram_sel = 0;
        self.step_sel = 0;
        self.detail_sel = 0;
        self.in_detail = false;
        self.folded.clear();
    }

    /// Shows or folds away the held and archived features, staying on the
    /// feature under the cursor when it is still listed.
    pub(super) fn toggle_folded_features(&mut self) {
        self.show_folded_features = !self.show_folded_features;
        let mut out = Vec::new();
        self.board_follower.settle(&self.tree, &Folding { show_folded: self.show_folded_features }, &mut out);
        self.send_board_requests(out);
        self.land_on_feature();
    }

    // ---- levels -------------------------------------------------------

    /// Opens the feature under the cursor on the lens it was last read on.
    pub(super) fn open_selected_feature(&mut self) {
        if self.selected_feature().is_some() {
            self.feature_open = true;
            self.in_detail = false;
        }
    }

    /// Up one level: out of the detail column, out of the feature, and from
    /// the index back to the workspace.
    pub(super) fn leave_feature_level(&mut self) {
        if self.in_detail {
            self.in_detail = false;
        } else if self.feature_open {
            self.feature_open = false;
        } else {
            self.open_view(View::Workspace);
        }
    }

    pub(super) fn set_lens(&mut self, lens: Lens) {
        if self.feature_open {
            self.lens = lens;
            self.in_detail = false;
            self.detail_sel = 0;
        }
    }

    /// Crosses between the list and the detail beside it. There is nothing
    /// to cross to when the selected row has nothing in it to act on.
    pub(super) fn toggle_detail(&mut self) {
        if self.in_detail {
            self.in_detail = false;
        } else if self.detail_len() > 0 {
            self.in_detail = true;
            self.detail_sel = self.detail_sel.min(self.detail_len() - 1);
        } else {
            self.report("nothing in the detail to act on");
        }
    }

    /// What the detail column's cursor can sit on: a task's criteria, or an
    /// open decision's options.
    pub fn detail_len(&self) -> usize {
        if let Some(decision) = self.selected_decision() {
            return if decision.open() { decision.options.len() } else { 0 };
        }
        self.selected_task().map_or(0, |task| task.criteria.len())
    }

    /// Re-asks for everything the stage draws.
    pub(super) fn refresh_feature(&mut self) {
        let mut out = Vec::new();
        self.board_follower.refresh(&mut out);
        self.send_board_requests(out);
    }

    /// Moves the cursor in whatever has the keys.
    pub(super) fn move_in_feature(&mut self, delta: i32) {
        let step = |at: usize, len: usize| {
            (at as i32)
                .saturating_add(delta)
                .clamp(0, len.saturating_sub(1) as i32) as usize
        };
        if !self.feature_open {
            return self.move_feature_selection(delta);
        }
        if self.in_detail {
            self.detail_sel = step(self.detail_sel, self.detail_len());
            return;
        }
        match self.lens {
            Lens::Work => self.work_sel = step(self.work_sel, self.outline().rows.len()),
            Lens::Record => self.decision_sel = step(self.decision_sel, self.board_rows().len()),
            Lens::Flows => self.step_sel = step(self.step_sel, self.flow_steps().len()),
        }
        self.detail_sel = 0;
    }

    /// Holds every cursor inside what it indexes after the lists change.
    pub(super) fn clamp_feature_cursors(&mut self) {
        let last = |len: usize| len.saturating_sub(1);
        self.work_sel = self.work_sel.min(last(self.outline().rows.len()));
        self.decision_sel = self.decision_sel.min(last(self.board_rows().len()));
        self.diagram_sel = self.diagram_sel.min(last(self.feature_diagrams().len()));
        self.step_sel = self.step_sel.min(last(self.flow_steps().len()));
        self.detail_sel = self.detail_sel.min(last(self.detail_len()));
        if self.detail_len() == 0 {
            self.in_detail = false;
        }
    }

    // ---- the work lens -------------------------------------------------

    pub fn feature_tasks(&self) -> &[Task] {
        self.board_follower
            .tasks()
            .filter(|list| list.feature == self.feature_slug())
            .map(|list| list.tasks.as_slice())
            .unwrap_or_default()
    }

    /// The selected feature's tasks with their parent/child topology.
    pub fn feature_task_rows(&self) -> Vec<TaskTreeRow<'_>> {
        self.board_follower
            .tasks()
            .filter(|list| list.feature == self.feature_slug())
            .map(|list| list.tree_rows())
            .unwrap_or_default()
    }

    /// The decision tree of the selected feature, depth-first.
    pub fn board_rows(&self) -> Vec<DecisionTreeRow<'_>> {
        self.board_scoped
            .as_ref()
            .map(|b| b.tree_rows())
            .unwrap_or_default()
    }

    /// The tasks and the decisions raised under them, as one outline.
    pub fn outline(&self) -> Outline<'_> {
        Outline::build(self.feature_task_rows(), self.board_rows(), &self.folded)
    }

    /// What the cursor is on, in whichever lens lists it.
    pub fn selected_work(&self) -> Option<WorkRef> {
        match self.lens {
            Lens::Work => self.outline().rows.get(self.work_sel).map(|row| row.at),
            Lens::Record => self
                .board_rows()
                .get(self.decision_sel)
                .map(|row| WorkRef::Decision(row.decision.id)),
            Lens::Flows => None,
        }
    }

    pub fn selected_task(&self) -> Option<&Task> {
        let Some(WorkRef::Task(id)) = self.selected_work() else {
            return None;
        };
        self.feature_tasks().iter().find(|task| task.id == id)
    }

    pub fn selected_decision(&self) -> Option<&Decision> {
        let Some(WorkRef::Decision(id)) = self.selected_work() else {
            return None;
        };
        self.board_scoped
            .as_ref()?
            .decisions
            .iter()
            .find(|decision| decision.id == id)
    }

    /// Puts the cursor back on the row it was on after the lists under it
    /// were replaced: a row you have to go looking for reads as lost.
    pub(super) fn keep_selection(&mut self, was: Option<WorkRef>) {
        if let Some(at) = was.and_then(|was| self.outline().rows.iter().position(|row| row.at == was)) {
            if self.lens == Lens::Work {
                self.work_sel = at;
            }
        }
        if let Some(WorkRef::Decision(id)) = was.filter(|_| self.lens == Lens::Record) {
            if let Some(at) = self.board_rows().iter().position(|row| row.decision.id == id) {
                self.decision_sel = at;
            }
        }
        self.clamp_feature_cursors();
    }

    /// Selects an outline or record row by what it is.
    pub(super) fn select_work(&mut self, at: WorkRef) {
        self.keep_selection(Some(at));
        self.in_detail = false;
        self.detail_sel = 0;
    }

    /// Folds the task under the cursor, or unfolds it. Folding a row with
    /// nothing under it climbs to its parent, the way a tree's `h` does.
    pub(super) fn fold_in_outline(&mut self, fold: bool) {
        if !self.feature_open || self.lens != Lens::Work {
            return;
        }
        let outline = self.outline();
        let Some(row) = outline.rows.get(self.work_sel) else {
            return;
        };
        let task = match row.at {
            WorkRef::Task(id) => outline.task(id),
            WorkRef::Decision(id) => outline.raised_under(id),
        };
        let Some(task) = task else { return };
        let (id, parent) = (task.task.id, task.task.parent);
        let holds_rows = task.has_children || outline.raised_count(id) > 0;
        let on_task = matches!(row.at, WorkRef::Task(_));
        drop(outline);
        if !fold {
            self.folded.remove(&id);
            return;
        }
        if on_task && holds_rows && !self.folded.contains(&id) {
            self.folded.insert(id);
            return;
        }
        let target = if on_task { parent } else { Some(id) };
        if let Some(target) = target {
            self.select_work(WorkRef::Task(target));
        }
    }

    // ---- the flows lens -------------------------------------------------

    pub fn feature_diagrams(&self) -> &[SequenceDiagram] {
        self.board_follower
            .diagrams()
            .filter(|list| list.feature == self.feature_slug())
            .map(|list| list.diagrams.as_slice())
            .unwrap_or_default()
    }

    pub fn selected_diagram(&self) -> Option<&SequenceDiagram> {
        self.feature_diagrams().get(self.diagram_sel)
    }

    /// The selected flow, read as rows.
    pub fn flow_rows(&self) -> Vec<FlowRow> {
        self.selected_diagram()
            .map(|diagram| argus_protocol::board::diagrams::flow::rows(&diagram.body))
            .unwrap_or_default()
    }

    /// The selected flow's steps, which is what the cursor moves over.
    pub fn flow_steps(&self) -> Vec<argus_protocol::board::diagrams::flow::Step> {
        self.flow_rows()
            .into_iter()
            .filter_map(|row| match row {
                FlowRow::Step(step) => Some(step),
                FlowRow::Branch { .. } => None,
            })
            .collect()
    }

    pub(super) fn move_flow_selection(&mut self, delta: i32) {
        let count = self.feature_diagrams().len();
        if !self.feature_open || self.lens != Lens::Flows || count == 0 {
            return;
        }
        self.select_diagram(
            (self.diagram_sel as i32)
                .saturating_add(delta)
                .clamp(0, count as i32 - 1) as usize,
        );
    }

    pub(super) fn select_diagram(&mut self, row: usize) {
        if row < self.feature_diagrams().len() && row != self.diagram_sel {
            self.diagram_sel = row;
            self.step_sel = 0;
        }
    }

    pub(super) fn open_selected_diagram(&mut self) {
        let Some(diagram) = self.selected_diagram().cloned() else {
            return;
        };
        let width = self.layout.terminal.inner.width.max(40) as usize;
        self.diagram = Some(crate::diagram::DiagramView::open(&diagram, width));
        self.overlay = Some(Overlay::SequenceDiagram);
    }

    // ---- what the keys act on ------------------------------------------

    /// `Enter`: opens whatever is under the cursor one level further.
    pub(super) fn open_in_feature(&mut self) {
        if !self.feature_open {
            return self.open_selected_feature();
        }
        if self.in_detail {
            return self.act_in_detail();
        }
        match (self.lens, self.selected_work()) {
            (Lens::Flows, _) => self.open_selected_diagram(),
            (_, Some(WorkRef::Task(_))) => self.open_task_brief(),
            // A decision in the outline is a line; the record is where it
            // is read whole.
            (Lens::Work, Some(at @ WorkRef::Decision(_))) => {
                self.lens = Lens::Record;
                self.select_work(at);
            }
            (Lens::Record, Some(WorkRef::Decision(_))) => {
                if self.detail_len() > 0 {
                    self.toggle_detail();
                } else if self.selected_decision().is_some_and(Decision::open) {
                    self.begin_resolution();
                }
            }
            (_, None) => {}
        }
    }

    /// Goes to the pane working the selected task, or the feature.
    pub(super) fn go_to_feature_pane(&mut self) {
        let pane = self
            .selected_task()
            .and_then(|task| task.claimed_pane)
            .or_else(|| self.current_feature_row().and_then(|row| row.pane));
        let location = pane.and_then(|pane| {
            self.flat_pane_locations()
                .into_iter()
                .find(|location| self.pane_at(*location).is_some_and(|p| p.id == pane))
        });
        match location {
            Some(location) => self.choose_rail_row(RailRow::Pane(location)),
            None => self.report("no pane is working this"),
        }
    }

    /// The task that comes next under a feature: the one being done, else
    /// the first not done.
    pub fn next_task(&self) -> Option<TaskTreeRow<'_>> {
        argus_protocol::board::standing::next_task(&self.feature_task_rows())
    }

    /// The task ids the outline has folded, for a renderer that marks them.
    pub fn folded_tasks(&self) -> &HashSet<i64> {
        &self.folded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lens_digit_is_its_place_in_the_strip() {
        let digits: Vec<char> = Lens::ALL.iter().map(|lens| lens.digit()).collect();

        assert_eq!(digits, ['1', '2', '3']);
    }

}
