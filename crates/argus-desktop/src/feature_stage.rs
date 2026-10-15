//! What the Feature stage holds — whether it is open, the repository's
//! board, the feature under the cursor and its tasks, how each repository
//! lays its index out — and what its messages and gestures change.
//!
//! The session says which repository the stage is about ([`Scope`]); this
//! asks the daemon for that repository's board, keeps what arrives, and
//! sends the stage's edits. Like everything in the window it never
//! predicts: a hold or an archive shows once the pushed board says so.

use std::collections::BTreeMap;

use argus_protocol::{
    CheckoutId, ClientMsg, DecisionBoard, FeatureState, ProjectId, ProjectInfo, RepositoryId,
    TaskAction, TaskList,
};

use crate::features::{self, Index, Layout};
use crate::page;

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

pub struct FeatureStage {
    open: bool,
    /// Another stage reads the board while this one is closed: the
    /// Checkouts stage names each checkout's feature.
    board_wanted: bool,
    /// The repository whose board was asked for on this connection.
    asked: Option<RepositoryId>,
    board: Option<DecisionBoard>,
    /// The repository `board` is the board of.
    board_for: Option<RepositoryId>,
    peek: Option<String>,
    peek_tasks: Option<TaskList>,
    show_folded: bool,
    /// `Settings::layouts`: the session saves them with the rest.
    layouts: BTreeMap<String, String>,
    layouts_changed: bool,
    changed: bool,
}

impl FeatureStage {
    pub fn new(layouts: BTreeMap<String, String>) -> FeatureStage {
        FeatureStage {
            open: false,
            board_wanted: false,
            asked: None,
            board: None,
            board_for: None,
            peek: None,
            peek_tasks: None,
            show_folded: false,
            layouts,
            layouts_changed: false,
            changed: true,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn set_open(&mut self, open: bool) {
        self.open = open;
        self.changed = true;
    }

    pub fn set_board_wanted(&mut self, wanted: bool) {
        self.board_wanted = wanted;
    }

    /// The repository's board, once it has arrived.
    pub fn board(&self) -> Option<&DecisionBoard> {
        self.board.as_ref()
    }

    /// The feature under the stage's cursor.
    pub fn peeked(&self) -> Option<&str> {
        self.peek.as_deref()
    }

    /// A new connection: whatever was asked of the last is asked again.
    pub fn forget_asked(&mut self) {
        self.asked = None;
    }

    /// The tree changed: rows say which agents are on each feature.
    pub fn tree_changed(&mut self) {
        self.changed = true;
    }

    /// Asks for the scope's board when the stage is open and it has not
    /// been asked for on this connection. A board for another repository
    /// is dropped at once, so the index never shows the wrong one.
    pub fn refresh(&mut self, scope: Option<&Scope>, out: &mut Vec<ClientMsg>) {
        if !self.open && !self.board_wanted {
            return;
        }
        let Some(scope) = scope else {
            if self.board.take().is_some() {
                self.changed = true;
            }
            self.board_for = None;
            self.asked = None;
            return;
        };
        if self.asked == Some(scope.repository) {
            return;
        }
        // Kept across a reconnect while it is still this repository's, so
        // the index does not blank while the same board is fetched again.
        if self.board_for != Some(scope.repository) {
            self.board = None;
            self.board_for = None;
            self.peek = None;
            self.peek_tasks = None;
        }
        self.asked = Some(scope.repository);
        self.changed = true;
        out.push(ClientMsg::GetDecisions { project: scope.project, checkout: scope.checkout });
    }

    /// A board, answered or pushed. Every client is pushed every board;
    /// this keeps only the one it asked for, by the repositories it names
    /// (`DecisionBoard::is_for`, which the terminal client asks too).
    pub fn receive_board(&mut self, board: DecisionBoard, scope: Option<&Scope>, tree: &[ProjectInfo], out: &mut Vec<ClientMsg>) {
        let Some(scope) = scope else {
            return;
        };
        if self.asked != Some(scope.repository) || !board.is_for(&scope.project_name, scope.repository) {
            return;
        }
        self.board = Some(board);
        self.board_for = Some(scope.repository);
        self.changed = true;
        self.settle_peek(scope, tree, out);
    }

    pub fn receive_tasks(&mut self, list: TaskList, scope: Option<&Scope>) {
        let ours = scope.is_some_and(|scope| list.is_for(&scope.project_name, scope.repository))
            && list.feature.is_some()
            && list.feature == self.peek;
        if ours {
            self.peek_tasks = Some(list);
            self.changed = true;
        }
    }

    /// Puts the cursor on a feature and asks for its tasks.
    pub fn peek(&mut self, slug: &str, scope: Option<&Scope>, out: &mut Vec<ClientMsg>) {
        let listed = self.board.as_ref().is_some_and(|b| b.features.iter().any(|f| f.slug == slug));
        if !listed || self.peek.as_deref() == Some(slug) {
            return;
        }
        self.peek = Some(slug.to_string());
        self.peek_tasks = None;
        self.changed = true;
        self.ask_tasks(scope, out);
    }

    pub fn set_layout(&mut self, layout: Layout, scope: Option<&Scope>) {
        let Some(scope) = scope else {
            return;
        };
        self.layouts.insert(scope.key.clone(), layout.word().to_string());
        self.layouts_changed = true;
        self.changed = true;
    }

    pub fn toggle_folded(&mut self, scope: Option<&Scope>, tree: &[ProjectInfo], out: &mut Vec<ClientMsg>) {
        self.show_folded = !self.show_folded;
        self.changed = true;
        if let Some(scope) = scope {
            self.settle_peek(scope, tree, out);
        }
    }

    /// Holds a feature with a reason, or lifts its hold with `None`.
    pub fn hold(&self, slug: &str, reason: Option<String>, scope: Option<&Scope>, out: &mut Vec<ClientMsg>) {
        if let Some(scope) = scope.filter(|_| self.has(slug)) {
            let reason = reason.map(|r| r.trim().to_string()).filter(|r| !r.is_empty());
            out.push(ClientMsg::HoldFeature {
                project: scope.project,
                checkout: scope.checkout,
                slug: slug.to_string(),
                reason,
            });
        }
    }

    /// Archives a feature, which is accepting it, or reopens one archived.
    pub fn archive(&self, slug: &str, archived: bool, scope: Option<&Scope>, out: &mut Vec<ClientMsg>) {
        if let Some(scope) = scope.filter(|_| self.has(slug)) {
            out.push(ClientMsg::MoveFeature {
                project: scope.project,
                checkout: scope.checkout,
                slug: slug.to_string(),
                state: if archived { FeatureState::Done } else { FeatureState::Open },
                detail: None,
            });
        }
    }

    /// The index, when anything it shows changed.
    pub fn take(&mut self, scope: Option<&Scope>, tree: &[ProjectInfo]) -> Option<page::Features> {
        if !std::mem::take(&mut self.changed) {
            return None;
        }
        Some(self.index(scope, tree))
    }

    /// The index whether or not it changed, for a page that holds nothing.
    pub fn everything(&mut self, scope: Option<&Scope>, tree: &[ProjectInfo]) -> page::Features {
        self.changed = false;
        self.index(scope, tree)
    }

    /// Whether a layout changed since last asked, so they are saved.
    pub fn take_layouts_changed(&mut self) -> bool {
        std::mem::take(&mut self.layouts_changed)
    }

    pub fn layouts(&self) -> &BTreeMap<String, String> {
        &self.layouts
    }

    fn index(&self, scope: Option<&Scope>, tree: &[ProjectInfo]) -> page::Features {
        let layout = scope
            .and_then(|scope| self.layouts.get(&scope.key))
            .and_then(|word| Layout::from_word(word))
            .unwrap_or_default();
        features::build(&Index {
            repository: scope.map_or("", |scope| scope.repository_name.as_str()),
            board: self.board.as_ref(),
            tree,
            layout,
            show_folded: self.show_folded,
            peek: self.peek.as_deref(),
            peek_tasks: self.peek_tasks.as_ref(),
        })
    }

    fn settle_peek(&mut self, scope: &Scope, tree: &[ProjectInfo], out: &mut Vec<ClientMsg>) {
        let Some(board) = &self.board else {
            return;
        };
        let settled = features::settle_peek(board, tree, self.peek.as_deref(), self.show_folded);
        if settled != self.peek {
            self.peek = settled;
            self.peek_tasks = None;
            self.ask_tasks(Some(scope), out);
        } else if self.peek_tasks.is_none() {
            self.ask_tasks(Some(scope), out);
        }
    }

    fn ask_tasks(&self, scope: Option<&Scope>, out: &mut Vec<ClientMsg>) {
        let (Some(scope), Some(slug)) = (scope, &self.peek) else {
            return;
        };
        out.push(ClientMsg::Task {
            project: scope.project,
            checkout: scope.checkout,
            feature: slug.clone(),
            action: TaskAction::List,
        });
    }

    fn has(&self, slug: &str) -> bool {
        self.board.as_ref().is_some_and(|b| b.features.iter().any(|f| f.slug == slug))
    }
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
                ClientMsg::HoldFeature { slug, reason, .. } => format!("hold {slug} {reason:?}"),
                ClientMsg::MoveFeature { slug, state, .. } => format!("move {slug} {state:?}"),
                other => format!("{other:?}"),
            })
            .collect()
    }

    /// An open stage that has asked for and received the fixture board.
    fn loaded() -> (FeatureStage, Vec<ClientMsg>) {
        let mut stage = FeatureStage::new(BTreeMap::new());
        stage.set_open(true);
        let mut out = Vec::new();
        stage.refresh(Some(&scope()), &mut out);
        stage.receive_board(fixtures::board(), Some(&scope()), &fixtures::feature_tree(), &mut out);
        (stage, out)
    }

    #[test]
    fn opening_asks_for_the_board_once_then_its_first_features_tasks() {
        let (mut stage, out) = loaded();
        assert_eq!(names(&out), ["board 3", "tasks trim"]);
        let mut again = Vec::new();
        stage.refresh(Some(&scope()), &mut again);
        assert!(again.is_empty(), "asked already on this connection");
        stage.forget_asked();
        stage.refresh(Some(&scope()), &mut again);
        assert_eq!(names(&again), ["board 3"], "a new connection asks again");
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
        stage.set_open(true);
        let mut out = Vec::new();
        stage.receive_board(fixtures::board(), Some(&scope()), &fixtures::feature_tree(), &mut out);
        assert!(!stage.everything(Some(&scope()), &[]).loaded, "nothing was asked");

        stage.refresh(Some(&scope()), &mut out);
        let other = DecisionBoard { name: "work".into(), ..fixtures::board() };
        stage.receive_board(other, Some(&scope()), &fixtures::feature_tree(), &mut out);
        assert!(!stage.everything(Some(&scope()), &[]).loaded, "another project's board");
    }

    #[test]
    fn a_board_another_repository_of_the_project_was_pushed_is_dropped() {
        let mut stage = FeatureStage::new(BTreeMap::new());
        stage.set_open(true);
        let mut out = Vec::new();
        stage.refresh(Some(&scope()), &mut out);
        let elsewhere = DecisionBoard { repositories: vec![RepositoryId(9)], ..fixtures::board() };
        stage.receive_board(elsewhere, Some(&scope()), &fixtures::feature_tree(), &mut out);
        assert!(!stage.everything(Some(&scope()), &[]).loaded, "the same project, another repository");

        let ours = DecisionBoard { repositories: vec![RepositoryId(2)], ..fixtures::board() };
        stage.receive_board(ours, Some(&scope()), &fixtures::feature_tree(), &mut out);
        assert!(stage.everything(Some(&scope()), &[]).loaded);
    }

    #[test]
    fn another_repository_drops_the_board_and_asks_for_its_own() {
        let (mut stage, _) = loaded();
        let elsewhere = Scope { repository: RepositoryId(9), checkout: argus_protocol::CheckoutId(90), ..scope() };
        let mut out = Vec::new();
        stage.refresh(Some(&elsewhere), &mut out);
        assert_eq!(names(&out), ["board 90"]);
        assert!(!stage.everything(Some(&elsewhere), &[]).loaded);
    }

    #[test]
    fn peeking_asks_for_that_features_tasks_and_keeps_only_them() {
        let (mut stage, _) = loaded();
        let mut out = Vec::new();
        stage.peek("bump", Some(&scope()), &mut out);
        assert_eq!(names(&out), ["tasks bump"]);
        stage.peek("nowhere", Some(&scope()), &mut out);
        assert_eq!(out.len(), 1, "a feature not on the board is not peeked");

        stage.receive_tasks(fixtures::tasks("trim"), Some(&scope()));
        assert!(stage.peek_tasks.is_none(), "trim's tasks are not bump's");
        stage.receive_tasks(fixtures::tasks("bump"), Some(&scope()));
        assert!(stage.peek_tasks.is_some());
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
    fn a_layout_is_remembered_per_repository_and_asks_to_be_saved() {
        let (mut stage, _) = loaded();
        assert_eq!(stage.everything(Some(&scope()), &[]).layout, "list");
        stage.set_layout(Layout::Board, Some(&scope()));
        assert_eq!(stage.everything(Some(&scope()), &[]).layout, "board");
        assert!(stage.take_layouts_changed());
        assert_eq!(stage.layouts().get("/src/main").map(String::as_str), Some("board"));
        assert!(!stage.take_layouts_changed());
        let other = Scope { key: "/src/other".into(), ..scope() };
        assert_eq!(stage.everything(Some(&other), &[]).layout, "list");
    }

    #[test]
    fn showing_the_folded_keeps_the_cursor_and_hiding_them_moves_it_off() {
        let (mut stage, _) = loaded();
        let tree = fixtures::feature_tree();
        let mut out = Vec::new();
        stage.toggle_folded(Some(&scope()), &tree, &mut out);
        stage.peek("mouse", Some(&scope()), &mut out);
        out.clear();
        stage.toggle_folded(Some(&scope()), &tree, &mut out);
        assert_eq!(stage.peek.as_deref(), Some("trim"), "mouse is held, folded away again");
        assert_eq!(names(&out), ["tasks trim"]);
    }
}
