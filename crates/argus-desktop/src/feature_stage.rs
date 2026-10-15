//! What the Feature stage holds — the repository's board, the feature
//! under the cursor and its tasks, how each repository lays its index out
//! — and what its messages and gestures change. Whether the stage is up is
//! the session's; it says when the board is wanted.
//!
//! The session says which repository the stage is about ([`Scope`]); the
//! protocol's `BoardFollower` decides when to ask for its board and which
//! pushes to keep, as it does for the terminal client, and this sends the
//! stage's edits. Like everything in the window it never predicts: a hold
//! or an archive shows once the pushed board says so.

use std::collections::BTreeMap;

use argus_protocol::{
    BoardFollower, BoardScope, CheckoutId, ClientMsg, DecisionBoard, FeatureAction, ProjectId,
    ProjectInfo, Received, RepositoryId, TaskList,
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
    /// at, and its tasks.
    board: BoardFollower,
    show_folded: bool,
    /// `Settings::layouts`: the session saves them with the rest.
    layouts: BTreeMap<String, String>,
    layouts_changed: bool,
    changed: bool,
}

impl FeatureStage {
    pub fn new(layouts: BTreeMap<String, String>) -> FeatureStage {
        FeatureStage {
            board_wanted: false,
            // The window draws no sequence diagrams.
            board: BoardFollower::new(false),
            show_folded: false,
            layouts,
            layouts_changed: false,
            changed: true,
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
        let asked = out.len();
        if self.board.follow(scope.map(Scope::board), out) || out.len() > asked {
            self.changed = true;
        }
    }

    /// A board, answered or pushed: kept when it is the followed
    /// repository's, which every client decides alike.
    pub fn receive_board(&mut self, board: DecisionBoard, tree: &[ProjectInfo], out: &mut Vec<ClientMsg>) {
        if self.board.receive_board(board, tree, self.show_folded, out) {
            self.changed = true;
        }
    }

    pub fn receive_tasks(&mut self, list: TaskList) {
        if let Received::Adopted { .. } = self.board.receive_tasks(list, None) {
            self.changed = true;
        }
    }

    /// Puts the cursor on a feature and asks for its tasks.
    pub fn peek(&mut self, slug: &str, out: &mut Vec<ClientMsg>) {
        self.board.point(Some(slug), out);
        self.changed = true;
    }

    /// Puts the cursor on a feature the index may be folding away, showing
    /// the folded ones first: a peek on a row the index hides would be
    /// moved off it by the next board.
    pub fn reveal(&mut self, slug: &str, tree: &[ProjectInfo], out: &mut Vec<ClientMsg>) {
        let folded = self.board().is_some_and(|board| features::is_folded(board, tree, slug));
        if folded && !self.show_folded {
            self.show_folded = true;
            self.changed = true;
        }
        self.peek(slug, out);
    }

    /// How the scope's index is laid out, as remembered for it.
    pub fn layout(&self, scope: Option<&Scope>) -> Layout {
        scope
            .and_then(|scope| self.layouts.get(&scope.key))
            .and_then(|word| Layout::from_word(word))
            .unwrap_or_default()
    }

    pub fn set_layout(&mut self, layout: Layout, scope: Option<&Scope>) {
        let Some(scope) = scope else {
            return;
        };
        self.layouts.insert(scope.key.clone(), layout.word().to_string());
        self.layouts_changed = true;
        self.changed = true;
    }

    pub fn toggle_folded(&mut self, tree: &[ProjectInfo], out: &mut Vec<ClientMsg>) {
        self.show_folded = !self.show_folded;
        self.changed = true;
        self.board.settle(tree, self.show_folded, out);
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
        let layout = self.layout(scope);
        features::build(&Index {
            repository: scope.map_or("", |scope| scope.repository_name.as_str()),
            board: self.board.board(),
            tree,
            layout,
            show_folded: self.show_folded,
            peek: self.board.feature(),
            peek_tasks: self.board.tasks(),
        })
    }

    fn has(&self, slug: &str) -> bool {
        self.board().is_some_and(|b| b.features.iter().any(|f| f.slug == slug))
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
                ClientMsg::Feature { action, .. } => match action {
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
        stage.receive_board(fixtures::board(), &fixtures::feature_tree(), &mut out);
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
        assert_eq!(names(&again), ["board 3", "tasks trim"], "a new connection asks again");
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
        stage.receive_board(fixtures::board(), &fixtures::feature_tree(), &mut out);
        assert!(!stage.everything(Some(&scope()), &[]).loaded, "nothing was asked");

        stage.refresh(Some(&scope()), &mut out);
        let other = DecisionBoard { name: "work".into(), ..fixtures::board() };
        stage.receive_board(other, &fixtures::feature_tree(), &mut out);
        assert!(!stage.everything(Some(&scope()), &[]).loaded, "another project's board");
    }

    #[test]
    fn a_board_another_repository_of_the_project_was_pushed_is_dropped() {
        let mut stage = FeatureStage::new(BTreeMap::new());
        stage.set_board_wanted(true);
        let mut out = Vec::new();
        stage.refresh(Some(&scope()), &mut out);
        let elsewhere = DecisionBoard { repositories: vec![RepositoryId(9)], ..fixtures::board() };
        stage.receive_board(elsewhere, &fixtures::feature_tree(), &mut out);
        assert!(!stage.everything(Some(&scope()), &[]).loaded, "the same project, another repository");

        let ours = DecisionBoard { repositories: vec![RepositoryId(2)], ..fixtures::board() };
        stage.receive_board(ours, &fixtures::feature_tree(), &mut out);
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
        stage.peek("bump", &mut out);
        assert_eq!(names(&out), ["tasks bump"]);
        stage.peek("nowhere", &mut out);
        assert_eq!(out.len(), 1, "a feature not on the board is not peeked");

        stage.receive_tasks(fixtures::tasks("trim"));
        assert!(stage.board.tasks().is_none(), "trim's tasks are not bump's");
        stage.receive_tasks(fixtures::tasks("bump"));
        assert!(stage.board.tasks().is_some());
    }

    #[test]
    fn an_archived_feature_revealed_is_listed_and_stays_peeked_past_the_next_board() {
        let (mut stage, _) = loaded();
        let mut out = Vec::new();
        stage.reveal("old", &fixtures::feature_tree(), &mut out);
        assert_eq!(names(&out), ["tasks old"]);
        stage.receive_board(fixtures::board(), &fixtures::feature_tree(), &mut out);
        assert_eq!(stage.peeked(), Some("old"));
        assert!(stage.everything(Some(&scope()), &fixtures::feature_tree()).show_folded, "the index lists it");

        let (mut stage, _) = loaded();
        stage.reveal("bump", &fixtures::feature_tree(), &mut out);
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
    fn showing_the_folded_keeps_the_cursor_and_hiding_them_hands_it_to_a_neighbour() {
        let (mut stage, _) = loaded();
        let tree = fixtures::feature_tree();
        let mut out = Vec::new();
        stage.toggle_folded(&tree, &mut out);
        stage.peek("mouse", &mut out);
        out.clear();
        stage.toggle_folded(&tree, &mut out);
        assert_eq!(stage.peeked(), Some("ctrlc"), "mouse is held, folded away again");
        assert_eq!(names(&out), ["tasks ctrlc"]);
    }
}
