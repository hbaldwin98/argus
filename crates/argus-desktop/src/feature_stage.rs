//! What the Feature stage holds — the repository's board, the feature
//! under the cursor and its tasks and flows, whether that feature is opened,
//! the lens it is read through and where each lens's cursor is, how each
//! repository lays its index out, the filter and the fold — and what its
//! messages and gestures change, and where each cursor steps, never onto a
//! row the page does not draw. Whether the stage is up is the session's; it
//! says when the board is wanted.
//!
//! The session says which repository the stage is about ([`Scope`]); the
//! protocol's `BoardFollower` decides when to ask for its board and which
//! pushes to keep, as it does for the terminal client, and this sends the
//! stage's edits, `feature_edits` saying what each edit to an opened
//! feature asks. Like everything in the window it never predicts: a hold,
//! an archive or an answered question shows once the pushed board says so.

use std::collections::BTreeMap;

use argus_protocol::{
    BoardFollower, BoardScope, CheckoutId, ClientMsg, DecisionBoard, DiagramList, FeatureAction,
    ProjectId, ProjectInfo, Received, RepositoryId, TaskList,
};

use argus_protocol::{board::diagrams::flow, board::standing};

use argus_protocol::board::outline::WorkRef;

use crate::feature_edits::{self, Target};
use crate::feature_view;
use crate::features::{self, Index, Layout, Listing};
use crate::flows_lens::{self, FlowCursor};
use crate::connection::page::{self, FeatureEdit, FeatureViewLink, Lens, WorkAt};
use crate::work_lens;

/// Which repository the stage reads, and how to address its board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    pub project: ProjectId,
    pub project_name: String,
    pub repository: RepositoryId,
    pub repository_name: String,
    /// The checkout a request names; the board is the repository's either way.
    pub checkout: CheckoutId,
    /// What the layout is remembered under: the primary checkout's path.
    pub key: String,
}

impl Scope {
    fn board(&self) -> BoardScope {
        BoardScope {
            project: self.project,
            project_name: self.project_name.clone(),
            repository: self.repository,
            checkout: self.checkout,
        }
    }
}

pub struct FeatureStage {
    /// Something on screen reads the board: this stage, or the Checkouts
    /// stage and the palette, which name features from it.
    board_wanted: bool,
    /// The board, the feature under the cursor, which is the one peeked
    /// at, and its tasks and flows.
    board: BoardFollower,
    /// The scope the follower last followed. Put away, the stage keeps
    /// following it while the session's moves on, and a push of its board
    /// is settled on its layout, not the chosen repository's.
    followed: Option<Scope>,
    show_folded: bool,
    /// What the index's filter field says, as last applied; the page keeps
    /// what is being typed.
    filter: String,
    /// The feature the person last put the cursor on, or a settle other than
    /// the filter's left it on: a filter that lists it again lands back on it,
    /// so a letter mistyped and taken back never loses their place.
    chosen: Option<String>,
    /// The feature under the cursor is opened, drawn in place of the index.
    opened: bool,
    lens: Lens,
    /// Each lens's cursor on the opened feature; `None` is its first row.
    cursor: Option<WorkRef>,
    record: Option<i64>,
    flow: FlowCursor,
    /// `Settings::layouts`: the session saves them with the rest.
    layouts: BTreeMap<String, String>,
    layouts_changed: bool,
    /// The layout of a repository that remembers none of its own, as the
    /// Settings dialog sets it.
    opens_as: Layout,
    /// The views agents showed in the repository, as `agent_views` last
    /// said, each with the feature number it names. Held here only so the
    /// index is drawn again when they change.
    views: Vec<(i64, FeatureViewLink)>,
    changed: bool,
}

impl FeatureStage {
    pub fn new(layouts: BTreeMap<String, String>) -> FeatureStage {
        FeatureStage {
            board_wanted: false,
            board: BoardFollower::new(true),
            followed: None,
            show_folded: false,
            filter: String::new(),
            chosen: None,
            opened: false,
            lens: Lens::Work,
            cursor: None,
            record: None,
            flow: FlowCursor::default(),
            layouts,
            layouts_changed: false,
            opens_as: Layout::default(),
            views: Vec::new(),
            changed: true,
        }
    }

    pub fn set_views(&mut self, views: Vec<(i64, FeatureViewLink)>) {
        if self.views != views {
            self.views = views;
            self.changed = true;
        }
    }

    pub fn set_opens_as(&mut self, layout: Layout) {
        if self.opens_as != layout {
            self.opens_as = layout;
            self.changed = true;
        }
    }

    pub fn set_board_wanted(&mut self, wanted: bool) {
        self.board_wanted = wanted;
    }

    /// The repository's board, once it has arrived.
    pub fn board(&self) -> Option<&DecisionBoard> {
        self.board.board()
    }

    /// The feature under the stage's cursor.
    pub fn peeked(&self) -> Option<&str> {
        self.board.feature()
    }

    /// A new connection: whatever was asked of the last is asked again.
    pub fn forget_asked(&mut self) {
        self.board.reconnected();
    }

    /// The tree changed: rows say which agents are on each feature.
    pub fn tree_changed(&mut self) {
        self.changed = true;
    }

    /// Follows the scope's board while something on screen reads it.
    pub fn refresh(&mut self, scope: Option<&Scope>, out: &mut Vec<ClientMsg>) {
        if !self.board_wanted {
            return;
        }
        self.followed = scope.cloned();
        let asked = out.len();
        if self.board.follow(scope.map(Scope::board), out) || out.len() > asked {
            self.changed = true;
        }
    }

    /// A board, answered or pushed: kept when it is the followed
    /// repository's, which every client decides alike.
    pub fn receive_board(&mut self, board: DecisionBoard, scope: Option<&Scope>, tree: &[ProjectInfo], out: &mut Vec<ClientMsg>) {
        let before = self.peeked().map(str::to_string);
        if self.board.receive_board(board, tree, &self.listing(scope), out) {
            self.settled(before, false);
            self.changed = true;
        }
    }

    /// A feature's tasks, the cursor held on its task by id.
    pub fn receive_tasks(&mut self, list: TaskList) {
        if let Received::Adopted { lands } = self.board.receive_tasks(list, self.cursor) {
            self.cursor = lands;
            self.changed = true;
        }
    }

    /// A feature's flows, the cursor held on its flow by id: one rewritten
    /// by a drag is still the one on screen.
    pub fn receive_diagrams(&mut self, list: DiagramList) {
        if let Received::Adopted { lands } = self.board.receive_diagrams(list, self.flow.diagram) {
            if lands != self.flow.diagram {
                self.flow = FlowCursor { diagram: lands, step: 0 };
            }
            self.changed = true;
        }
    }

    /// Puts the cursor on a feature and asks for its tasks and flows.
    pub fn peek(&mut self, slug: &str, out: &mut Vec<ClientMsg>) {
        if self.board.feature() != Some(slug) {
            self.cursor = None;
            self.record = None;
            self.flow = FlowCursor::default();
        }
        self.board.point(Some(slug), out);
        self.chosen = self.board.feature().map(str::to_string);
        self.changed = true;
    }

    /// Opens a feature to read and edit, or with `None` goes back to the
    /// index, still peeking at the one that was open while the index lists
    /// it: one folded away while it was read hands the cursor on.
    pub fn open(&mut self, slug: Option<&str>, scope: Option<&Scope>, tree: &[ProjectInfo], out: &mut Vec<ClientMsg>) {
        match slug {
            Some(slug) if self.has(slug) => {
                self.reveal(slug, scope, tree, out);
                self.opened = true;
            }
            Some(_) => return,
            None => {
                self.opened = false;
                self.lens = Lens::Work;
                self.settle(scope, tree, out);
            }
        }
        self.changed = true;
    }

    /// Reads the opened feature through another lens.
    pub fn set_lens(&mut self, lens: Lens) {
        if self.opened && self.lens != lens {
            self.lens = lens;
            self.changed = true;
        }
    }

    /// Whether a feature is opened, and its slug.
    pub fn opened(&self) -> Option<&str> {
        self.board.feature().filter(|_| self.opened)
    }

    /// Puts the opened feature's cursor on a row of its outline.
    pub fn select(&mut self, at: WorkAt) {
        self.cursor = Some(work_lens::work_ref(at));
        self.changed = true;
    }

    /// Puts the Record lens's cursor on a decision.
    pub fn select_decision(&mut self, id: i64) {
        self.record = Some(id);
        self.changed = true;
    }

    /// Puts the Flows lens's cursor on a flow and one of its steps, or with
    /// no `step` on the step it was on, when the flow is the same one.
    pub fn select_step(&mut self, diagram: i64, step: Option<usize>) {
        let step = match step {
            Some(step) => step,
            None if self.flow.diagram == Some(diagram) => self.flow.step,
            None => 0,
        };
        self.flow = FlowCursor { diagram: Some(diagram), step };
        self.changed = true;
    }

    /// Sends an edit to the opened feature, or says why it cannot be sent.
    pub fn edit(&self, edit: FeatureEdit, scope: Option<&Scope>, out: &mut Vec<ClientMsg>) -> Result<(), String> {
        let (Some(scope), Some(slug), Some(board)) = (scope, self.opened(), self.board()) else {
            return Err("no feature is open to edit".into());
        };
        let target = Target { scope, slug, board, tasks: self.board.tasks(), diagrams: self.board.diagrams() };
        out.extend(feature_edits::messages(edit, &target)?);
        Ok(())
    }

    /// Puts the cursor on a feature the index may be folding or filtering
    /// away, showing the folded ones or clearing the filter first: a peek
    /// on a row the index hides would be moved off it by the next board.
    pub fn reveal(&mut self, slug: &str, scope: Option<&Scope>, tree: &[ProjectInfo], out: &mut Vec<ClientMsg>) {
        let listing = self.listing(scope);
        let row = self.board().and_then(|board| standing::feature_rows(board, tree).into_iter().find(|row| row.slug == slug));
        if let Some(row) = row {
            if listing.folds(row.standing) && !self.show_folded {
                self.show_folded = true;
                self.changed = true;
            }
            if !listing.matches(&row) {
                self.filter.clear();
                self.changed = true;
            }
        }
        self.peek(slug, out);
        self.settle(scope, tree, out);
    }

    /// How the scope's index is laid out, as remembered for it.
    pub fn layout(&self, scope: Option<&Scope>) -> Layout {
        scope
            .and_then(|scope| self.layouts.get(&scope.key))
            .and_then(|word| Layout::from_word(word))
            .unwrap_or(self.opens_as)
    }

    pub fn set_layout(&mut self, layout: Layout, scope: Option<&Scope>, tree: &[ProjectInfo], out: &mut Vec<ClientMsg>) {
        let Some(key) = scope.map(|scope| scope.key.clone()) else {
            return;
        };
        self.layouts.insert(key, layout.word().to_string());
        self.layouts_changed = true;
        self.changed = true;
        self.settle(scope, tree, out);
    }

    /// Shows the features the fold covers, or folds them: on the list the
    /// held and archived, on the board the Archived column.
    pub fn toggle_folded(&mut self, scope: Option<&Scope>, tree: &[ProjectInfo], out: &mut Vec<ClientMsg>) {
        self.show_folded = !self.show_folded;
        self.changed = true;
        self.settle(scope, tree, out);
    }

    /// What the filter field says now: the cursor moves onto a feature it
    /// lists, the one chosen last whenever it lists that.
    pub fn set_filter(&mut self, filter: String, scope: Option<&Scope>, tree: &[ProjectInfo], out: &mut Vec<ClientMsg>) {
        if self.filter == filter {
            return;
        }
        self.filter = filter;
        self.changed = true;
        let before = self.peeked().map(str::to_string);
        let listing = self.listing(scope);
        let chosen = self.chosen.clone().filter(|slug| self.board.listed(tree, &listing).iter().any(|row| row.slug == *slug));
        if chosen.is_some() {
            self.board.point(chosen.as_deref(), out);
        }
        self.board.settle(tree, &listing, out);
        self.settled(before, true);
    }

    /// Puts the cursor back on a listed feature after what the index lists
    /// may have changed.
    pub fn settle(&mut self, scope: Option<&Scope>, tree: &[ProjectInfo], out: &mut Vec<ClientMsg>) {
        let before = self.peeked().map(str::to_string);
        self.board.settle(tree, &self.listing(scope), out);
        self.settled(before, false);
    }

    /// Moves the cursor `by` rows of what is drawn, held at either end: on
    /// the index the listed features, on an opened feature the rows of the
    /// lens read. `isize::MIN`/`isize::MAX` are the first and last, as
    /// `CheckoutsStage::step`.
    pub fn step(&mut self, by: isize, scope: Option<&Scope>, tree: &[ProjectInfo], out: &mut Vec<ClientMsg>) {
        if self.opened().is_some() {
            return self.step_lens(by);
        }
        let listed = self.board.listed(tree, &self.listing(scope));
        let at = self.board.feature().and_then(|slug| listed.iter().position(|row| row.slug == slug));
        if let Some(next) = stepped(listed.len(), at, by) {
            let slug = listed[next].slug.clone();
            self.peek(&slug, out);
        }
    }

    /// The Flows lens's flow switcher: `by` flows on, onto its first step.
    pub fn step_flow(&mut self, by: isize) {
        if self.opened().is_none() || self.lens != Lens::Flows {
            return;
        }
        let Some(list) = self.board.diagrams() else {
            return;
        };
        let at = flows_lens::on(Some(list), self.flow).and_then(|on| list.diagrams.iter().position(|d| d.id == on.id));
        if let Some(next) = stepped(list.diagrams.len(), at, by) {
            self.flow = FlowCursor { diagram: Some(list.diagrams[next].id), step: 0 };
            self.changed = true;
        }
    }

    /// Steps the opened feature's cursor in the lens read, over the rows it
    /// draws. A cursor on nothing, or on a row gone, is drawn on the first,
    /// so it steps from there.
    fn step_lens(&mut self, by: isize) {
        let (Some(slug), Some(board)) = (self.opened(), self.board()) else {
            return;
        };
        match self.lens {
            Lens::Work => {
                let rows = feature_view::cursor_rows(slug, board, self.board.tasks()).work;
                let at = self.cursor.and_then(|on| rows.iter().position(|row| *row == on)).unwrap_or(0);
                if let Some(next) = stepped(rows.len(), Some(at), by) {
                    self.cursor = Some(rows[next]);
                }
            }
            Lens::Record => {
                let rows = feature_view::cursor_rows(slug, board, self.board.tasks()).record;
                let at = self.record.and_then(|on| rows.iter().position(|id| *id == on)).unwrap_or(0);
                if let Some(next) = stepped(rows.len(), Some(at), by) {
                    self.record = Some(rows[next]);
                }
            }
            Lens::Flows => {
                // The flow is named, as a click on a step names it: a push
                // that reorders the flows must not move the cursor to another's.
                let Some(on) = flows_lens::on(self.board.diagrams(), self.flow) else {
                    return;
                };
                let (diagram, count) = (on.id, flows_lens::steps(&flow::rows(&on.body)).len());
                if let Some(step) = stepped(count, Some(self.flow.step.min(count.saturating_sub(1))), by) {
                    self.flow = FlowCursor { diagram: Some(diagram), step };
                }
            }
        }
        self.changed = true;
    }

    /// Holds a feature with a reason, or lifts its hold with `None`.
    pub fn hold(&self, slug: &str, reason: Option<String>, scope: Option<&Scope>, out: &mut Vec<ClientMsg>) {
        if let Some(scope) = scope.filter(|_| self.has(slug)) {
            let reason = reason.map(|r| r.trim().to_string()).filter(|r| !r.is_empty());
            let slug = slug.to_string();
            out.push(ClientMsg::Feature {
                project: scope.project,
                checkout: scope.checkout,
                action: match reason {
                    Some(reason) => FeatureAction::Hold { slug, reason },
                    None => FeatureAction::Unhold { slug },
                },
                request_id: 0,
            });
        }
    }

    /// Says `slug` comes after `on`, or takes that back.
    pub fn wait(&self, slug: &str, on: &str, waits: bool, scope: Option<&Scope>, out: &mut Vec<ClientMsg>) {
        if let Some(scope) = scope.filter(|_| self.has(slug) && self.has(on) && slug != on) {
            let (slug, on) = (slug.to_string(), on.to_string());
            out.push(ClientMsg::Feature {
                project: scope.project,
                checkout: scope.checkout,
                action: if waits { FeatureAction::Wait { slug, on } } else { FeatureAction::Unwait { slug, on } },
                request_id: 0,
            });
        }
    }

    /// Archives a feature, which is accepting it, or reopens one archived.
    pub fn archive(&self, slug: &str, archived: bool, scope: Option<&Scope>, out: &mut Vec<ClientMsg>) {
        if let Some(scope) = scope.filter(|_| self.has(slug)) {
            let (slug, detail) = (slug.to_string(), None);
            out.push(ClientMsg::Feature {
                project: scope.project,
                checkout: scope.checkout,
                action: if archived { FeatureAction::Done { slug, detail } } else { FeatureAction::Reopen { slug, detail } },
                request_id: 0,
            });
        }
    }

    /// The index, when anything it shows changed.
    pub fn take(&mut self, scope: Option<&Scope>, tree: &[ProjectInfo], now: u64) -> Option<page::Features> {
        if !std::mem::take(&mut self.changed) {
            return None;
        }
        Some(self.index(scope, tree, now))
    }

    /// The index whether or not it changed, for a page that holds nothing.
    pub fn everything(&mut self, scope: Option<&Scope>, tree: &[ProjectInfo], now: u64) -> page::Features {
        self.changed = false;
        self.index(scope, tree, now)
    }

    /// Whether a layout changed since last asked, so they are saved.
    pub fn take_layouts_changed(&mut self) -> bool {
        std::mem::take(&mut self.layouts_changed)
    }

    pub fn layouts(&self) -> &BTreeMap<String, String> {
        &self.layouts
    }

    fn index(&self, scope: Option<&Scope>, tree: &[ProjectInfo], now: u64) -> page::Features {
        features::build(&Index {
            repository: scope.map_or("", |scope| scope.repository_name.as_str()),
            board: self.board.board(),
            tree,
            listing: &self.listing(scope),
            peek: self.board.feature(),
            peek_tasks: self.board.tasks(),
            peek_diagrams: self.board.diagrams(),
            opened: self.opened,
            lens: self.lens,
            work: self.cursor,
            record: self.record,
            flow: self.flow,
            views: &self.views,
            now,
        })
    }

    /// What the index lists now. Owned, so it can be built while the
    /// follower is borrowed to settle on it.
    fn listing(&self, scope: Option<&Scope>) -> Listing {
        Listing {
            layout: self.layout(self.followed.as_ref().or(scope)),
            show_folded: self.show_folded,
            filter: self.filter.clone(),
            opened: self.opened().map(str::to_string),
        }
    }

    /// After the follower settled the cursor: a feature it moved onto starts
    /// its lenses afresh, since they were the last one's, and an opened
    /// feature it moved off closes. The listing keeps an opened feature, so
    /// that one left the board, and the view must not show its neighbour.
    /// Unless `filtering`, where it lands is the feature chosen from now on.
    fn settled(&mut self, before: Option<String>, filtering: bool) {
        if !filtering {
            self.chosen = self.peeked().map(str::to_string);
        }
        if self.peeked() == before.as_deref() {
            return;
        }
        self.cursor = None;
        self.record = None;
        self.flow = FlowCursor::default();
        if self.opened {
            self.opened = false;
            self.lens = Lens::Work;
        }
        self.changed = true;
    }

    fn has(&self, slug: &str) -> bool {
        self.board().is_some_and(|b| b.features.iter().any(|f| f.slug == slug))
    }
}

/// Where a cursor at `at` of `len` rows lands `by` rows on, held at either
/// end; from nowhere, on the first, or with `isize::MAX` on the last.
fn stepped(len: usize, at: Option<usize>, by: isize) -> Option<usize> {
    let last = len.checked_sub(1)?;
    Some(match at {
        Some(at) => at.saturating_add_signed(by).min(last),
        None if by == isize::MAX => last,
        None => 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    fn scope() -> Scope {
        Scope {
            project: ProjectId(1),
            project_name: "src".into(),
            repository: RepositoryId(2),
            repository_name: "argus".into(),
            checkout: argus_protocol::CheckoutId(3),
            key: "/src/main".into(),
        }
    }

    fn names(out: &[ClientMsg]) -> Vec<String> {
        out.iter()
            .map(|m| match m {
                ClientMsg::GetDecisions { checkout, .. } => format!("board {checkout}"),
                ClientMsg::Task { feature, .. } => format!("tasks {feature}"),
                ClientMsg::SequenceDiagram { feature, .. } => format!("flows {feature}"),
                ClientMsg::Feature { action, .. } => match action {
                    FeatureAction::Wait { slug, on } => format!("{slug} after {on}"),
                    FeatureAction::Unwait { slug, on } => format!("{slug} not after {on}"),
                    FeatureAction::Hold { slug, reason } => format!("hold {slug} {:?}", Some(reason)),
                    FeatureAction::Unhold { slug } => format!("hold {slug} None"),
                    FeatureAction::Done { slug, .. } => format!("move {slug} Done"),
                    FeatureAction::Reopen { slug, .. } => format!("move {slug} Open"),
                    other => format!("{other:?}"),
                },
                other => format!("{other:?}"),
            })
            .collect()
    }

    /// An open stage that has asked for and received the fixture board.
    fn loaded() -> (FeatureStage, Vec<ClientMsg>) {
        let mut stage = FeatureStage::new(BTreeMap::new());
        stage.set_board_wanted(true);
        let mut out = Vec::new();
        stage.refresh(Some(&scope()), &mut out);
        stage.receive_board(fixtures::board(), Some(&scope()), &fixtures::feature_tree(), &mut out);
        (stage, out)
    }

    #[test]
    fn opening_asks_for_the_board_once_then_its_first_features_tasks() {
        let (mut stage, out) = loaded();
        assert_eq!(names(&out), ["board 3", "tasks trim", "flows trim"]);
        let mut again = Vec::new();
        stage.refresh(Some(&scope()), &mut again);
        assert!(again.is_empty(), "asked already on this connection");
        stage.forget_asked();
        stage.refresh(Some(&scope()), &mut again);
        assert_eq!(names(&again), ["board 3", "tasks trim", "flows trim"], "a new connection asks again");
    }

    #[test]
    fn a_closed_stage_asks_for_nothing() {
        let mut stage = FeatureStage::new(BTreeMap::new());
        let mut out = Vec::new();
        stage.refresh(Some(&scope()), &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn a_board_not_asked_for_is_dropped() {
        let mut stage = FeatureStage::new(BTreeMap::new());
        stage.set_board_wanted(true);
        let mut out = Vec::new();
        stage.receive_board(fixtures::board(), Some(&scope()), &fixtures::feature_tree(), &mut out);
        assert!(!stage.everything(Some(&scope()), &[], 0).loaded, "nothing was asked");

        stage.refresh(Some(&scope()), &mut out);
        let other = DecisionBoard { name: "work".into(), ..fixtures::board() };
        stage.receive_board(other, Some(&scope()), &fixtures::feature_tree(), &mut out);
        assert!(!stage.everything(Some(&scope()), &[], 0).loaded, "another project's board");
    }

    #[test]
    fn a_board_another_repository_of_the_project_was_pushed_is_dropped() {
        let mut stage = FeatureStage::new(BTreeMap::new());
        stage.set_board_wanted(true);
        let mut out = Vec::new();
        stage.refresh(Some(&scope()), &mut out);
        let elsewhere = DecisionBoard { repositories: vec![RepositoryId(9)], ..fixtures::board() };
        stage.receive_board(elsewhere, Some(&scope()), &fixtures::feature_tree(), &mut out);
        assert!(!stage.everything(Some(&scope()), &[], 0).loaded, "the same project, another repository");

        let ours = DecisionBoard { repositories: vec![RepositoryId(2)], ..fixtures::board() };
        stage.receive_board(ours, Some(&scope()), &fixtures::feature_tree(), &mut out);
        assert!(stage.everything(Some(&scope()), &[], 0).loaded);
    }

    #[test]
    fn another_repository_drops_the_board_and_asks_for_its_own() {
        let (mut stage, _) = loaded();
        let elsewhere = Scope { repository: RepositoryId(9), checkout: argus_protocol::CheckoutId(90), ..scope() };
        let mut out = Vec::new();
        stage.refresh(Some(&elsewhere), &mut out);
        assert_eq!(names(&out), ["board 90"]);
        assert!(!stage.everything(Some(&elsewhere), &[], 0).loaded);
    }

    #[test]
    fn peeking_asks_for_that_features_tasks_and_keeps_only_them() {
        let (mut stage, _) = loaded();
        let mut out = Vec::new();
        stage.peek("bump", &mut out);
        assert_eq!(names(&out), ["tasks bump", "flows bump"]);
        stage.peek("nowhere", &mut out);
        assert_eq!(out.len(), 2, "a feature not on the board is not peeked");

        stage.receive_tasks(fixtures::tasks("trim"));
        assert!(stage.board.tasks().is_none(), "trim's tasks are not bump's");
        stage.receive_tasks(fixtures::tasks("bump"));
        assert!(stage.board.tasks().is_some());
    }

    #[test]
    fn an_archived_feature_revealed_is_listed_and_stays_peeked_past_the_next_board() {
        let (mut stage, _) = loaded();
        let mut out = Vec::new();
        stage.reveal("old", Some(&scope()), &fixtures::feature_tree(), &mut out);
        assert_eq!(names(&out), ["tasks old", "flows old"]);
        stage.receive_board(fixtures::board(), Some(&scope()), &fixtures::feature_tree(), &mut out);
        assert_eq!(stage.peeked(), Some("old"));
        assert!(stage.everything(Some(&scope()), &fixtures::feature_tree(), 0).show_folded, "the index lists it");

        let (mut stage, _) = loaded();
        stage.reveal("bump", Some(&scope()), &fixtures::feature_tree(), &mut out);
        assert_eq!(stage.peeked(), Some("bump"));
        assert!(!stage.show_folded, "a listed feature leaves the folded ones folded");
    }

    #[test]
    fn holds_and_archives_go_to_the_daemon_and_change_nothing_here() {
        let (stage, _) = loaded();
        let mut out = Vec::new();
        stage.hold("trim", Some("  waiting on design ".into()), Some(&scope()), &mut out);
        stage.hold("mouse", Some("   ".into()), Some(&scope()), &mut out);
        stage.archive("ctrlc", true, Some(&scope()), &mut out);
        stage.archive("old", false, Some(&scope()), &mut out);
        stage.hold("nowhere", None, Some(&scope()), &mut out);
        assert_eq!(
            names(&out),
            [
                "hold trim Some(\"waiting on design\")",
                "hold mouse None",
                "move ctrlc Done",
                "move old Open",
            ]
        );
    }

    #[test]
    fn an_opened_feature_takes_its_edits_to_the_daemon_and_changes_nothing_here() {
        let (mut stage, _) = loaded();
        let mut out = Vec::new();
        let refused = stage.edit(FeatureEdit::Withdraw { id: 1, withdrawn: true }, Some(&scope()), &mut out);
        assert!(refused.is_err() && out.is_empty(), "nothing is open");

        stage.open(Some("trim"), Some(&scope()), &fixtures::feature_tree(), &mut out);
        assert_eq!(stage.opened(), Some("trim"));
        stage.receive_tasks(fixtures::tasks("trim"));
        stage.select(WorkAt::Decision { id: 2 });
        let before = stage.everything(Some(&scope()), &fixtures::feature_tree(), 0);
        stage.edit(FeatureEdit::Withdraw { id: 2, withdrawn: true }, Some(&scope()), &mut out).unwrap();
        assert!(matches!(out.last(), Some(ClientMsg::ChangeDecision { .. })));
        assert_eq!(stage.everything(Some(&scope()), &fixtures::feature_tree(), 0), before, "the push is what changes it");
        assert_eq!(before.open.unwrap().decision.unwrap().id, 2);

        stage.open(None, Some(&scope()), &fixtures::feature_tree(), &mut out);
        assert_eq!((stage.opened(), stage.peeked()), (None, Some("trim")), "back on the index, still peeking");
    }

    #[test]
    fn a_lens_is_chosen_on_an_open_feature_and_closing_it_goes_back_to_work() {
        let (mut stage, _) = loaded();
        let tree = fixtures::feature_tree();
        let mut out = Vec::new();
        stage.set_lens(Lens::Record);
        assert_eq!(stage.lens, Lens::Work, "nothing is open to read");
        stage.open(Some("trim"), Some(&scope()), &tree, &mut out);
        stage.set_lens(Lens::Record);
        stage.select_decision(2);
        let open = stage.everything(Some(&scope()), &tree, 0).open.unwrap();
        assert_eq!((open.lens, open.record.unwrap().decision.unwrap().id), (Lens::Record, 2));
        stage.open(None, Some(&scope()), &tree, &mut out);
        stage.open(Some("trim"), Some(&scope()), &tree, &mut out);
        assert_eq!(stage.everything(Some(&scope()), &tree, 0).open.unwrap().lens, Lens::Work);
    }

    #[test]
    fn the_flows_cursor_stays_on_its_flow_across_a_push_and_a_flow_gone_hands_it_on() {
        let (mut stage, _) = loaded();
        let tree = fixtures::feature_tree();
        let mut out = Vec::new();
        stage.open(Some("trim"), Some(&scope()), &tree, &mut out);
        stage.set_lens(Lens::Flows);
        stage.receive_diagrams(fixtures::diagrams("trim"));
        stage.select_step(12, Some(0));
        stage.select_step(11, Some(3));
        stage.select_step(11, None);
        assert_eq!(stage.flow, FlowCursor { diagram: Some(11), step: 3 }, "the same flow keeps its step");

        stage.receive_diagrams(fixtures::diagrams("trim"));
        assert_eq!(stage.flow.step, 3, "a rewrite pushed back keeps the step");
        let mut list = fixtures::diagrams("trim");
        list.diagrams.remove(0);
        stage.receive_diagrams(list);
        let flows = stage.everything(Some(&scope()), &tree, 0).open.unwrap().flows.unwrap();
        assert_eq!(flows.flow.unwrap().id, 12);
        stage.receive_diagrams(fixtures::diagrams("bump"));
        assert_eq!(stage.flow.diagram, Some(12), "another feature's flows are not trim's");
    }

    #[test]
    fn coming_after_a_feature_is_asked_of_the_daemon_and_only_between_two_on_the_board() {
        let (stage, _) = loaded();
        let mut out = Vec::new();
        stage.wait("desktop", "trim", true, Some(&scope()), &mut out);
        stage.wait("bump", "trim", false, Some(&scope()), &mut out);
        stage.wait("bump", "bump", true, Some(&scope()), &mut out);
        stage.wait("bump", "nowhere", true, Some(&scope()), &mut out);
        assert_eq!(names(&out), ["desktop after trim", "bump not after trim"]);
    }

    #[test]
    fn a_feature_not_on_the_board_does_not_open() {
        let (mut stage, _) = loaded();
        let mut out = Vec::new();
        stage.open(Some("nowhere"), Some(&scope()), &fixtures::feature_tree(), &mut out);
        assert_eq!(stage.opened(), None);
    }

    #[test]
    fn a_layout_is_remembered_per_repository_and_asks_to_be_saved() {
        let (mut stage, _) = loaded();
        assert_eq!(stage.everything(Some(&scope()), &[], 0).layout, "list");
        stage.set_layout(Layout::Board, Some(&scope()), &[], &mut Vec::new());
        assert_eq!(stage.everything(Some(&scope()), &[], 0).layout, "board");
        assert!(stage.take_layouts_changed());
        assert_eq!(stage.layouts().get("/src/main").map(String::as_str), Some("board"));
        assert!(!stage.take_layouts_changed());
        let other = Scope { key: "/src/other".into(), ..scope() };
        stage.refresh(Some(&other), &mut Vec::new());
        assert_eq!(stage.everything(Some(&other), &[], 0).layout, "list");
    }

    #[test]
    fn a_repository_that_remembers_no_layout_opens_as_the_settings_say() {
        let (mut stage, _) = loaded();
        stage.set_layout(Layout::List, Some(&scope()), &[], &mut Vec::new());
        let other = Scope { key: "/src/other".into(), ..scope() };
        stage.refresh(Some(&other), &mut Vec::new());
        stage.take(Some(&other), &[], 0);
        stage.set_opens_as(Layout::Board);
        assert!(stage.take(Some(&other), &[], 0).is_some(), "the index is drawn again");
        assert_eq!(stage.everything(Some(&other), &[], 0).layout, "board");
        stage.refresh(Some(&scope()), &mut Vec::new());
        assert_eq!(stage.everything(Some(&scope()), &[], 0).layout, "list", "its own is kept");
    }

    #[test]
    fn showing_the_folded_keeps_the_cursor_and_hiding_them_hands_it_to_a_neighbour() {
        let (mut stage, _) = loaded();
        let tree = fixtures::feature_tree();
        let mut out = Vec::new();
        stage.toggle_folded(Some(&scope()), &tree, &mut out);
        stage.peek("mouse", &mut out);
        out.clear();
        stage.toggle_folded(Some(&scope()), &tree, &mut out);
        assert_eq!(stage.peeked(), Some("ctrlc"), "mouse is held, folded away again");
        assert_eq!(names(&out), ["tasks ctrlc", "flows ctrlc"]);
    }

    /// The fixture board with `slug` held.
    fn holding(slug: &str) -> DecisionBoard {
        let mut board = fixtures::board();
        board.features.iter_mut().find(|f| f.slug == slug).unwrap().held = Some("waiting".into());
        board
    }

    fn slugs(features: &page::Features) -> Vec<&str> {
        features.rows.iter().map(|row| row.slug.as_str()).collect()
    }

    #[test]
    fn a_held_card_peeked_on_the_board_stays_peeked_past_the_next_board() {
        let (mut stage, _) = loaded();
        let tree = fixtures::feature_tree();
        let mut out = Vec::new();
        stage.set_layout(Layout::Board, Some(&scope()), &tree, &mut out);
        stage.peek("mouse", &mut out);
        stage.receive_board(fixtures::board(), Some(&scope()), &tree, &mut out);
        assert_eq!(stage.peeked(), Some("mouse"), "the board draws its Held column");
    }

    #[test]
    fn a_board_pushed_while_another_repository_is_chosen_settles_on_its_own_layout() {
        let (mut stage, _) = loaded();
        let tree = fixtures::feature_tree();
        let mut out = Vec::new();
        stage.set_layout(Layout::Board, Some(&scope()), &tree, &mut out);
        stage.peek("mouse", &mut out);
        // Put away, the stage still follows this repository's board.
        stage.set_board_wanted(false);
        let other = Scope { repository: RepositoryId(9), key: "/src/other".into(), ..scope() };
        stage.refresh(Some(&other), &mut out);
        stage.receive_board(fixtures::board(), Some(&other), &tree, &mut out);
        assert_eq!(stage.peeked(), Some("mouse"), "settled on the board layout, not the list's");
    }

    #[test]
    fn an_archived_feature_gone_to_on_the_board_unfolds_its_column() {
        let (mut stage, _) = loaded();
        let tree = fixtures::feature_tree();
        let mut out = Vec::new();
        stage.set_layout(Layout::Board, Some(&scope()), &tree, &mut out);
        stage.reveal("mouse", Some(&scope()), &tree, &mut out);
        assert!(!stage.show_folded, "a held card is drawn on the board already");
        stage.reveal("old", Some(&scope()), &tree, &mut out);
        stage.receive_board(fixtures::board(), Some(&scope()), &tree, &mut out);
        let index = stage.everything(Some(&scope()), &tree, 0);
        assert!(index.show_folded && slugs(&index).contains(&"old"));
        assert_eq!(stage.peeked(), Some("old"));
    }

    #[test]
    fn the_boards_archived_column_folds_behind_the_same_toggle() {
        let (mut stage, _) = loaded();
        let tree = fixtures::feature_tree();
        let mut out = Vec::new();
        stage.set_layout(Layout::Board, Some(&scope()), &tree, &mut out);
        let folded = stage.everything(Some(&scope()), &tree, 0);
        assert_eq!((slugs(&folded), folded.folded), (vec!["bump", "desktop", "trim", "ctrlc", "mouse"], 1));
        stage.toggle_folded(Some(&scope()), &tree, &mut out);
        let shown = stage.everything(Some(&scope()), &tree, 0);
        assert_eq!(slugs(&shown), ["bump", "desktop", "trim", "ctrlc", "old", "mouse"]);
    }

    #[test]
    fn a_feature_held_while_it_is_open_stays_open_and_closing_it_moves_the_cursor_on() {
        let (mut stage, _) = loaded();
        let tree = fixtures::feature_tree();
        let mut out = Vec::new();
        stage.open(Some("trim"), Some(&scope()), &tree, &mut out);
        stage.receive_board(holding("trim"), Some(&scope()), &tree, &mut out);
        assert_eq!(stage.opened(), Some("trim"), "not swapped for its neighbour");
        assert!(slugs(&stage.everything(Some(&scope()), &tree, 0)).contains(&"trim"));
        stage.open(None, Some(&scope()), &tree, &mut out);
        assert_eq!((stage.opened(), stage.peeked()), (None, Some("ctrlc")), "the one in its place");
    }

    #[test]
    fn an_opened_feature_that_leaves_the_board_closes_rather_than_showing_another() {
        let (mut stage, _) = loaded();
        let tree = fixtures::feature_tree();
        let mut out = Vec::new();
        stage.open(Some("trim"), Some(&scope()), &tree, &mut out);
        stage.set_lens(Lens::Record);
        let mut board = fixtures::board();
        board.features.retain(|f| f.slug != "trim");
        stage.receive_board(board, Some(&scope()), &tree, &mut out);
        assert_eq!((stage.opened(), stage.peeked(), stage.lens), (None, Some("bump"), Lens::Work));
    }

    #[test]
    fn a_cursor_moved_by_a_push_starts_the_lenses_afresh() {
        let (mut stage, _) = loaded();
        let tree = fixtures::feature_tree();
        let mut out = Vec::new();
        stage.peek("bump", &mut out);
        stage.select(WorkAt::Decision { id: 2 });
        stage.select_decision(2);
        stage.select_step(11, Some(1));
        stage.receive_board(holding("bump"), Some(&scope()), &tree, &mut out);
        assert_eq!(stage.peeked(), Some("desktop"));
        assert_eq!((stage.cursor, stage.record, stage.flow), (None, None, FlowCursor::default()));
    }

    #[test]
    fn filtering_moves_the_cursor_onto_a_feature_it_lists() {
        let (mut stage, _) = loaded();
        let tree = fixtures::feature_tree();
        let mut out = Vec::new();
        stage.set_filter("ctrl".into(), Some(&scope()), &tree, &mut out);
        assert_eq!(stage.peeked(), Some("ctrlc"));
        assert_eq!(names(&out), ["tasks ctrlc", "flows ctrlc"]);
        assert_eq!(slugs(&stage.everything(Some(&scope()), &tree, 0)), ["ctrlc"]);
    }

    #[test]
    fn a_pushed_board_lands_the_cursor_only_on_a_feature_the_filter_lists() {
        let (mut stage, _) = loaded();
        let tree = fixtures::feature_tree();
        let mut out = Vec::new();
        stage.set_filter("c".into(), Some(&scope()), &tree, &mut out);
        assert_eq!(slugs(&stage.everything(Some(&scope()), &tree, 0)), ["trim", "desktop", "ctrlc"]);
        let mut board = fixtures::board();
        board.features.iter_mut().find(|f| f.slug == "trim").unwrap().title = "Trim the brief".into();
        stage.receive_board(board, Some(&scope()), &tree, &mut out);
        assert_eq!(stage.peeked(), Some("desktop"), "trim no longer matches");
    }

    #[test]
    fn going_to_a_feature_the_filter_hides_clears_the_filter() {
        let (mut stage, _) = loaded();
        let tree = fixtures::feature_tree();
        let mut out = Vec::new();
        stage.set_filter("ctrl".into(), Some(&scope()), &tree, &mut out);
        stage.reveal("bump", Some(&scope()), &tree, &mut out);
        let index = stage.everything(Some(&scope()), &tree, 0);
        assert_eq!((stage.peeked(), index.filter.as_str()), (Some("bump"), ""));
        stage.set_filter("ratatui".into(), Some(&scope()), &tree, &mut out);
        stage.reveal("bump", Some(&scope()), &tree, &mut out);
        assert_eq!(stage.filter, "ratatui", "a filter that lists it stays");
    }

    #[test]
    fn a_filter_matching_nothing_peeks_nothing_and_widening_it_lands_back_on_the_feature_chosen() {
        let (mut stage, _) = loaded();
        let tree = fixtures::feature_tree();
        let mut out = Vec::new();
        stage.peek("ctrlc", &mut out);
        stage.set_filter("zz".into(), Some(&scope()), &tree, &mut out);
        assert_eq!(stage.peeked(), None);
        stage.set_filter(String::new(), Some(&scope()), &tree, &mut out);
        assert_eq!(stage.peeked(), Some("ctrlc"), "not the first");
    }

    #[test]
    fn typing_a_filter_that_still_lists_the_chosen_feature_keeps_it_and_asks_nothing() {
        let (mut stage, _) = loaded();
        let tree = fixtures::feature_tree();
        let mut out = Vec::new();
        stage.peek("desktop", &mut out);
        out.clear();
        stage.set_filter("d".into(), Some(&scope()), &tree, &mut out);
        stage.set_filter("de".into(), Some(&scope()), &tree, &mut out);
        stage.take(Some(&scope()), &tree, 0);
        stage.set_filter("de".into(), Some(&scope()), &tree, &mut out);
        assert_eq!(stage.peeked(), Some("desktop"));
        assert!(out.is_empty(), "{:?}", names(&out));
        assert!(stage.take(Some(&scope()), &tree, 0).is_none(), "the same text changes nothing");
    }

    /// Steps `by` as many times as there are moves, saying where each landed.
    fn steps(stage: &mut FeatureStage, moves: &[isize]) -> Vec<String> {
        let tree = fixtures::feature_tree();
        let mut out = Vec::new();
        moves
            .iter()
            .map(|by| {
                stage.step(*by, Some(&scope()), &tree, &mut out);
                stage.peeked().unwrap_or("nothing").to_string()
            })
            .collect()
    }

    #[test]
    fn stepping_moves_the_cursor_over_the_listed_features_and_holds_at_the_ends() {
        let (mut stage, _) = loaded();
        let tree = fixtures::feature_tree();
        let mut out = Vec::new();
        stage.step(1, Some(&scope()), &tree, &mut out);
        assert_eq!(names(&out), ["tasks bump", "flows bump"], "each move asks that feature's lists");
        assert_eq!(steps(&mut stage, &[1, 1, 1, -1]), ["desktop", "ctrlc", "ctrlc", "desktop"]);
        assert_eq!(steps(&mut stage, &[isize::MIN, -1, isize::MAX]), ["trim", "trim", "ctrlc"]);
    }

    #[test]
    fn the_board_steps_column_by_column_and_skips_the_folded_archived() {
        let (mut stage, _) = loaded();
        let tree = fixtures::feature_tree();
        stage.set_layout(Layout::Board, Some(&scope()), &tree, &mut Vec::new());
        assert_eq!(steps(&mut stage, &[isize::MIN, 1, 1, 1, 1, 1]), ["bump", "desktop", "trim", "ctrlc", "mouse", "mouse"]);
    }

    #[test]
    fn stepping_from_nothing_lands_on_the_first_or_with_last_on_the_last() {
        let (mut stage, _) = loaded();
        let mut out = Vec::new();
        stage.board.point(None, &mut out);
        assert_eq!(steps(&mut stage, &[isize::MAX]), ["ctrlc"]);
        stage.board.point(None, &mut out);
        assert_eq!(steps(&mut stage, &[-1]), ["trim"]);
        stage.set_filter("zz".into(), Some(&scope()), &fixtures::feature_tree(), &mut out);
        out.clear();
        assert_eq!(steps(&mut stage, &[1]), ["nothing"], "nothing listed to step onto");
    }

    #[test]
    fn the_index_is_not_stepped_while_a_feature_is_open() {
        let (mut stage, _) = loaded();
        stage.open(Some("trim"), Some(&scope()), &fixtures::feature_tree(), &mut Vec::new());
        assert_eq!(steps(&mut stage, &[1, isize::MAX]), ["trim", "trim"]);
    }

    #[test]
    fn an_open_feature_steps_the_cursor_of_the_lens_read() {
        let (mut stage, _) = loaded();
        let tree = fixtures::feature_tree();
        let mut out = Vec::new();
        stage.open(Some("trim"), Some(&scope()), &tree, &mut out);
        stage.receive_tasks(fixtures::tasks("trim"));
        let rows = feature_view::cursor_rows("trim", stage.board().unwrap(), stage.board.tasks());
        out.clear();
        stage.step(1, Some(&scope()), &tree, &mut out);
        assert_eq!(stage.cursor, Some(rows.work[1]), "from the first, drawn on with no cursor");
        stage.step(isize::MAX, Some(&scope()), &tree, &mut out);
        assert_eq!(stage.cursor, rows.work.last().copied());
        assert_eq!(stage.peeked(), Some("trim"));

        stage.set_lens(Lens::Record);
        stage.step(1, Some(&scope()), &tree, &mut out);
        stage.step(1, Some(&scope()), &tree, &mut out);
        assert_eq!((rows.record.len(), stage.record), (2, Some(rows.record[1])), "held at the last");
        stage.step(isize::MIN, Some(&scope()), &tree, &mut out);
        assert_eq!(stage.record, rows.record.first().copied());
        assert!(out.is_empty(), "a lens step asks nothing");
    }

    #[test]
    fn the_flows_lens_steps_within_its_flow_and_a_flow_switched_to_starts_on_its_first_step() {
        let (mut stage, _) = loaded();
        let tree = fixtures::feature_tree();
        let mut out = Vec::new();
        stage.open(Some("trim"), Some(&scope()), &tree, &mut out);
        stage.set_lens(Lens::Flows);
        stage.receive_diagrams(fixtures::diagrams("trim"));
        stage.step(1, Some(&scope()), &tree, &mut out);
        assert_eq!(stage.flow, FlowCursor { diagram: Some(11), step: 1 });
        stage.step(isize::MAX, Some(&scope()), &tree, &mut out);
        assert_eq!(stage.flow.step, 4, "the last of its five steps, the branch lines left out");
        stage.step_flow(1);
        assert_eq!(stage.flow, FlowCursor { diagram: Some(12), step: 0 });
        stage.step_flow(1);
        assert_eq!(stage.flow.diagram, Some(12), "held at the last flow");
        stage.step_flow(-1);
        assert_eq!(stage.flow, FlowCursor { diagram: Some(11), step: 0 });
    }

    #[test]
    fn a_step_names_its_flow_so_a_reordering_push_keeps_it() {
        let (mut stage, _) = loaded();
        let tree = fixtures::feature_tree();
        let mut out = Vec::new();
        stage.open(Some("trim"), Some(&scope()), &tree, &mut out);
        stage.set_lens(Lens::Flows);
        stage.receive_diagrams(fixtures::diagrams("trim"));
        assert_eq!(stage.flow.diagram, None);
        stage.step(1, Some(&scope()), &tree, &mut out);
        assert_eq!(stage.flow.diagram, Some(11));
        let mut reversed = fixtures::diagrams("trim");
        reversed.diagrams.reverse();
        stage.receive_diagrams(reversed);
        assert_eq!(stage.flow, FlowCursor { diagram: Some(11), step: 1 });
    }

    #[test]
    fn changing_the_layout_moves_the_cursor_off_a_feature_the_new_one_folds() {
        let (mut stage, _) = loaded();
        let tree = fixtures::feature_tree();
        let mut out = Vec::new();
        stage.set_layout(Layout::Board, Some(&scope()), &tree, &mut out);
        stage.peek("mouse", &mut out);
        stage.set_layout(Layout::List, Some(&scope()), &tree, &mut out);
        assert_eq!(stage.peeked(), Some("ctrlc"));
    }
}
