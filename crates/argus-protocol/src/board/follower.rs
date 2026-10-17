//! How a client follows one repository's board: what it asks the daemon
//! for and when, which pushed boards and lists it keeps, and where the
//! reader's cursor lands when one of them is replaced under it.
//! Which features an index lists is the client's to say (`FeatureListing`):
//! the cursor lands only on one it lists.
//!
//! Every board, task list and diagram list is pushed to every client,
//! because the daemon does not track who is reading what. So each client
//! decides when to ask and what to keep, and two clients that decided
//! differently showed different boards. Said once here, they cannot.
//!
//! The rule for asking: once per repository per connection. The board is
//! the repository's whichever of its checkouts names it, so moving between
//! checkouts asks nothing; moving to another repository drops the board at
//! once, so the wrong one is never drawn while its own is fetched; and a
//! new connection asks again, keeping what it holds meanwhile, because
//! whatever was pushed while the daemon was away was pushed to nobody.
//! Between those the pushes keep it current, so nothing is asked twice.

use crate::tree::ids::{CheckoutId, ProjectId, RepositoryId};
use crate::board::outline::WorkRef;
use crate::board::standing::{self, FeatureRow};
use crate::{ClientMsg, DecisionBoard, DiagramAction, DiagramList, ProjectInfo, TaskAction, TaskList};

/// The repository whose board is followed, and how a request names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardScope {
    pub project: ProjectId,
    /// What a push names its project by.
    pub project_name: String,
    pub repository: RepositoryId,
    /// The checkout a request names; the board is the repository's either way.
    pub checkout: CheckoutId,
}

/// What became of a pushed list, and where the cursor that was on it is now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub enum Received<C> {
    /// Another repository's, or another feature's: some other reader's.
    Ignored,
    /// Held now. `lands` is what the cursor is on after it, `None` when
    /// the list has nothing left to be on.
    Adopted { lands: Option<C> },
}

/// Which of a board's features a client's index lists, in the order it lists
/// them, so a cursor settled after a push lands only on a row that is drawn.
pub trait FeatureListing {
    /// `rows` are every feature on the board, as `standing::feature_rows` files them.
    fn list(&self, rows: Vec<FeatureRow>) -> Vec<FeatureRow>;
}

/// The terminal client's index: every feature in `standing`'s order, the held
/// and archived folded away unless `show_folded`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Folding {
    pub show_folded: bool,
}

impl FeatureListing for Folding {
    fn list(&self, rows: Vec<FeatureRow>) -> Vec<FeatureRow> {
        rows.into_iter().filter(|row| self.show_folded || !row.standing.folded()).collect()
    }
}

pub struct BoardFollower {
    /// Whether this client draws a feature's sequence diagrams, and so asks
    /// for them.
    flows: bool,
    scope: Option<BoardScope>,
    /// Whether the scope's board was asked for on this connection.
    asked: bool,
    board: Option<DecisionBoard>,
    /// The feature the reader is on, whose tasks and diagrams are held.
    feature: Option<String>,
    /// The features as last listed, so a feature that leaves the list hands
    /// the cursor to the one that took its place rather than to the top.
    listed: Vec<String>,
    tasks: Option<TaskList>,
    diagrams: Option<DiagramList>,
}

impl BoardFollower {
    pub fn new(flows: bool) -> BoardFollower {
        BoardFollower {
            flows,
            scope: None,
            asked: false,
            board: None,
            feature: None,
            listed: Vec::new(),
            tasks: None,
            diagrams: None,
        }
    }

    /// Follows `scope`'s board, asking for it unless it was asked for on
    /// this connection. Called whenever the scope may have moved, and cheap
    /// when it has not. `None` follows nothing and drops everything held.
    /// Answers whether anything held was dropped.
    pub fn follow(&mut self, scope: Option<BoardScope>, out: &mut Vec<ClientMsg>) -> bool {
        let same = match (&self.scope, &scope) {
            (Some(held), Some(scope)) => held.project == scope.project && held.repository == scope.repository,
            (None, None) => true,
            _ => false,
        };
        let dropped = !same && self.board.is_some();
        if !same {
            self.board = None;
            self.feature = None;
            self.listed.clear();
            self.tasks = None;
            self.diagrams = None;
            self.asked = false;
        }
        self.scope = scope;
        if !self.asked && self.scope.is_some() {
            self.refresh(out);
        }
        dropped
    }

    /// A new connection: the next [`Self::follow`] asks again. What is held
    /// stays on screen until the answer replaces it.
    pub fn reconnected(&mut self) {
        self.asked = false;
    }

    /// Asks for the board and the feature's lists again, as a person
    /// pressing refresh does.
    pub fn refresh(&mut self, out: &mut Vec<ClientMsg>) {
        let Some(scope) = &self.scope else {
            return;
        };
        self.asked = true;
        out.push(ClientMsg::GetDecisions { project: scope.project, checkout: scope.checkout });
        self.ask_lists(out);
    }

    /// The reader moved onto a feature, or onto none: its lists are asked
    /// for and the last one's dropped. A feature not on the board is not
    /// moved onto, and the reader stays where it was.
    pub fn point(&mut self, feature: Option<&str>, out: &mut Vec<ClientMsg>) {
        if feature.is_some_and(|slug| !self.on_board(slug)) || feature == self.feature.as_deref() {
            return;
        }
        self.feature = feature.map(str::to_string);
        self.tasks = None;
        self.diagrams = None;
        self.ask_lists(out);
    }

    /// A board, answered or pushed. Kept when it is the followed
    /// repository's (`DecisionBoard::is_for`), and the reader's feature
    /// then settled on it. Answers whether it was kept.
    pub fn receive_board(&mut self, board: DecisionBoard, tree: &[ProjectInfo], listing: &impl FeatureListing, out: &mut Vec<ClientMsg>) -> bool {
        let ours = self.scope.as_ref().is_some_and(|scope| board.is_for(&scope.project_name, scope.repository));
        if !ours {
            return false;
        }
        self.board = Some(board);
        self.settle(tree, listing, out);
        true
    }

    /// Puts the reader on a listed feature after the list changed: the one
    /// it was on while it is still listed, else the one that took its
    /// place, else the first.
    pub fn settle(&mut self, tree: &[ProjectInfo], listing: &impl FeatureListing, out: &mut Vec<ClientMsg>) {
        let listed: Vec<String> = self.listed(tree, listing).into_iter().map(|row| row.slug).collect();
        let lands = match self.feature.as_deref() {
            Some(held) if listed.iter().any(|slug| slug == held) => Some(held.to_string()),
            held => {
                let was = held.and_then(|held| self.listed.iter().position(|slug| slug == held));
                was.and_then(|at| listed.get(at.min(listed.len().saturating_sub(1))))
                    .or_else(|| listed.first())
                    .cloned()
            }
        };
        self.listed = listed;
        self.point(lands.as_deref(), out);
    }

    /// A feature's tasks. Kept when they are the reader's feature's, with
    /// the cursor `on` held by id: a task reordered or moved under it is
    /// still the one under it, and one removed hands it to its parent, or
    /// to the task that took its place.
    pub fn receive_tasks(&mut self, list: TaskList, on: Option<WorkRef>) -> Received<WorkRef> {
        if !self.is_ours(|project, repository| list.is_for(project, repository), list.feature.as_deref()) {
            return Received::Ignored;
        }
        let mut list = list;
        list.tasks = list.tasks_in_tree_order();
        let lands = match on {
            Some(WorkRef::Task(id)) => land_task(self.tasks.as_ref(), &list, id).map(WorkRef::Task),
            // Decisions are on the board, which a task list does not replace.
            other => other,
        };
        self.tasks = Some(list);
        Received::Adopted { lands }
    }

    /// A feature's diagrams, kept as tasks are, with the cursor `on` a
    /// diagram held by id.
    pub fn receive_diagrams(&mut self, list: DiagramList, on: Option<i64>) -> Received<i64> {
        if !self.is_ours(|project, repository| list.is_for(project, repository), list.feature.as_deref()) {
            return Received::Ignored;
        }
        let ids = |list: &DiagramList| list.diagrams.iter().map(|d| d.id).collect::<Vec<_>>();
        let before = self.diagrams.as_ref().map(ids).unwrap_or_default();
        let lands = on.and_then(|id| land_id(&before, &ids(&list), id));
        self.diagrams = Some(list);
        Received::Adopted { lands }
    }

    pub fn scope(&self) -> Option<&BoardScope> {
        self.scope.as_ref()
    }

    /// The followed repository's board, once it has arrived.
    pub fn board(&self) -> Option<&DecisionBoard> {
        self.board.as_ref()
    }

    /// The feature the reader is on.
    pub fn feature(&self) -> Option<&str> {
        self.feature.as_deref()
    }

    /// The reader's feature's tasks, in tree order, once they have arrived.
    pub fn tasks(&self) -> Option<&TaskList> {
        self.tasks.as_ref()
    }

    pub fn diagrams(&self) -> Option<&DiagramList> {
        self.diagrams.as_ref()
    }

    /// The features an index lists, as `listing` says.
    pub fn listed(&self, tree: &[ProjectInfo], listing: &impl FeatureListing) -> Vec<FeatureRow> {
        let Some(board) = &self.board else {
            return Vec::new();
        };
        listing.list(standing::feature_rows(board, tree))
    }

    fn on_board(&self, slug: &str) -> bool {
        self.board.as_ref().is_some_and(|board| board.features.iter().any(|f| f.slug == slug))
    }

    /// Feature slugs are unique only inside a board, so a list is the
    /// reader's when both its repository and its feature are.
    fn is_ours(&self, is_for: impl Fn(&str, RepositoryId) -> bool, feature: Option<&str>) -> bool {
        let Some(scope) = &self.scope else {
            return false;
        };
        is_for(&scope.project_name, scope.repository) && feature.is_some() && feature == self.feature.as_deref()
    }

    fn ask_lists(&self, out: &mut Vec<ClientMsg>) {
        let (Some(scope), Some(feature)) = (&self.scope, &self.feature) else {
            return;
        };
        out.push(ClientMsg::Task {
            project: scope.project,
            checkout: scope.checkout,
            feature: feature.clone(),
            action: TaskAction::List,
        });
        if self.flows {
            out.push(ClientMsg::SequenceDiagram {
                project: scope.project,
                checkout: scope.checkout,
                feature: feature.clone(),
                action: DiagramAction::List,
            });
        }
    }
}

/// Where a cursor on task `id` lands in `now`: on it, else on the nearest
/// of its ancestors still there, else on whatever took its place.
fn land_task(before: Option<&TaskList>, now: &TaskList, id: i64) -> Option<i64> {
    let ids: Vec<i64> = now.tasks.iter().map(|task| task.id).collect();
    if ids.contains(&id) {
        return Some(id);
    }
    let before = before.map(|list| list.tasks.as_slice()).unwrap_or_default();
    let mut parent = before.iter().find(|task| task.id == id).and_then(|task| task.parent);
    while let Some(up) = parent {
        if ids.contains(&up) {
            return Some(up);
        }
        parent = before.iter().find(|task| task.id == up).and_then(|task| task.parent);
    }
    let before: Vec<i64> = before.iter().map(|task| task.id).collect();
    land_id(&before, &ids, id)
}

/// Where a cursor on `id` lands in `now`: on it, else on the row that took
/// its place in `before`, clamped to the end.
fn land_id(before: &[i64], now: &[i64], id: i64) -> Option<i64> {
    if now.contains(&id) {
        return Some(id);
    }
    let at = before.iter().position(|was| *was == id)?;
    now.get(at.min(now.len().saturating_sub(1))).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Feature, SequenceDiagram, Task};

    const FOLDED: Folding = Folding { show_folded: false };
    const SHOWN: Folding = Folding { show_folded: true };

    fn scope() -> BoardScope {
        BoardScope {
            project: ProjectId(1),
            project_name: "src".into(),
            repository: RepositoryId(2),
            checkout: CheckoutId(3),
        }
    }

    fn feature(slug: &str) -> Feature {
        Feature { slug: slug.into(), ..Feature::default() }
    }

    fn board(features: Vec<Feature>) -> DecisionBoard {
        DecisionBoard {
            project: Some(ProjectId(1)),
            name: "src".into(),
            features,
            repositories: vec![RepositoryId(2)],
            ..DecisionBoard::default()
        }
    }

    fn board_of(slugs: &[&str]) -> DecisionBoard {
        board(slugs.iter().map(|slug| feature(slug)).collect())
    }

    fn task(id: i64, parent: Option<i64>) -> Task {
        Task { id, parent, feature: "trim".into(), title: format!("task {id}"), ..Task::default() }
    }

    fn tasks(feature: &str, tasks: Vec<Task>) -> TaskList {
        TaskList {
            project_name: "src".into(),
            feature: Some(feature.into()),
            tasks,
            repositories: vec![RepositoryId(2)],
        }
    }

    fn diagrams(ids: &[i64]) -> DiagramList {
        let diagram = |id: &i64| SequenceDiagram {
            id: *id,
            feature: "trim".into(),
            title: String::new(),
            body: String::new(),
            at: 0,
            session: None,
        };
        DiagramList {
            project_name: "src".into(),
            feature: Some("trim".into()),
            diagrams: ids.iter().map(diagram).collect(),
            repositories: vec![RepositoryId(2)],
        }
    }

    fn names(out: &[ClientMsg]) -> Vec<String> {
        out.iter()
            .map(|msg| match msg {
                ClientMsg::GetDecisions { checkout, .. } => format!("board {checkout}"),
                ClientMsg::Task { feature, .. } => format!("tasks {feature}"),
                ClientMsg::SequenceDiagram { feature, .. } => format!("diagrams {feature}"),
                other => format!("{other:?}"),
            })
            .collect()
    }

    /// Following the scope, with its board of `trim`, `bump`, `mouse` in.
    fn following() -> BoardFollower {
        let mut follower = BoardFollower::new(true);
        let mut out = Vec::new();
        follower.follow(Some(scope()), &mut out);
        assert!(follower.receive_board(board_of(&["trim", "bump", "mouse"]), &[], &FOLDED, &mut out));
        follower
    }

    #[test]
    fn a_scope_followed_asks_for_its_board_once_then_for_the_first_features_lists() {
        let mut follower = BoardFollower::new(true);
        let mut out = Vec::new();
        follower.follow(Some(scope()), &mut out);
        assert_eq!(names(&out), ["board 3"]);
        follower.follow(Some(scope()), &mut out);
        assert_eq!(out.len(), 1, "asked already on this connection");
        assert!(follower.receive_board(board_of(&["trim", "bump"]), &[], &FOLDED, &mut out));
        assert_eq!(names(&out[1..]), ["tasks trim", "diagrams trim"]);
        assert_eq!(follower.feature(), Some("trim"));
    }

    #[test]
    fn another_checkout_of_the_repository_asks_nothing_but_is_named_from_then_on() {
        let mut follower = following();
        let mut out = Vec::new();
        let worktree = BoardScope { checkout: CheckoutId(30), ..scope() };
        assert!(!follower.follow(Some(worktree), &mut out));
        assert!(out.is_empty(), "the board is the repository's");
        assert!(follower.board().is_some());
        follower.point(Some("bump"), &mut out);
        assert!(matches!(out[0], ClientMsg::Task { checkout: CheckoutId(30), .. }));
    }

    #[test]
    fn another_repository_drops_the_board_at_once_and_asks_for_its_own() {
        let mut follower = following();
        let mut out = Vec::new();
        let elsewhere = BoardScope { repository: RepositoryId(9), checkout: CheckoutId(90), ..scope() };
        assert!(follower.follow(Some(elsewhere), &mut out));
        assert_eq!(names(&out), ["board 90"]);
        assert!(follower.board().is_none(), "never the wrong board while its own is fetched");
        assert_eq!(follower.feature(), None);
        assert!(!follower.follow(None, &mut out), "nothing held to drop");
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn a_reconnect_asks_again_and_keeps_what_it_holds_meanwhile() {
        let mut follower = following();
        let mut out = Vec::new();
        follower.reconnected();
        assert!(follower.board().is_some());
        follower.follow(Some(scope()), &mut out);
        assert_eq!(names(&out), ["board 3", "tasks trim", "diagrams trim"]);
        follower.follow(Some(scope()), &mut out);
        assert_eq!(out.len(), 3, "once per connection");
    }

    #[test]
    fn a_client_without_flows_does_not_ask_for_diagrams() {
        let mut follower = BoardFollower::new(false);
        let mut out = Vec::new();
        follower.follow(Some(scope()), &mut out);
        let _ = follower.receive_board(board_of(&["trim"]), &[], &FOLDED, &mut out);
        assert_eq!(names(&out), ["board 3", "tasks trim"]);
    }

    #[test]
    fn a_push_for_another_repository_or_project_is_ignored() {
        let mut follower = following();
        let mut out = Vec::new();
        let elsewhere = DecisionBoard { repositories: vec![RepositoryId(9)], ..board_of(&["other"]) };
        assert!(!follower.receive_board(elsewhere, &[], &FOLDED, &mut out));
        let renamed = DecisionBoard { name: "work".into(), ..board_of(&["other"]) };
        assert!(!follower.receive_board(renamed, &[], &FOLDED, &mut out));
        assert_eq!(follower.board().unwrap().features.len(), 3);

        let mut list = tasks("trim", vec![task(1, None)]);
        list.repositories = vec![RepositoryId(9)];
        assert_eq!(follower.receive_tasks(list, None), Received::Ignored);
        let bumps = tasks("bump", vec![task(1, None)]);
        assert_eq!(follower.receive_tasks(bumps, None), Received::Ignored, "another feature's");
        assert!(follower.tasks().is_none());

        let mut nothing = BoardFollower::new(true);
        assert!(!nothing.receive_board(board_of(&["trim"]), &[], &FOLDED, &mut out), "following nothing keeps nothing");
    }

    #[test]
    fn a_push_for_this_board_keeps_the_cursor_on_its_feature() {
        let mut follower = following();
        let mut out = Vec::new();
        follower.point(Some("bump"), &mut out);
        out.clear();
        assert!(follower.receive_board(board_of(&["new", "trim", "bump", "mouse"]), &[], &FOLDED, &mut out));
        assert_eq!(follower.feature(), Some("bump"));
        assert!(out.is_empty(), "the same feature's lists are not asked again");
    }

    #[test]
    fn a_feature_leaving_the_list_hands_the_cursor_to_the_one_in_its_place() {
        let mut follower = following();
        let mut out = Vec::new();
        follower.point(Some("bump"), &mut out);
        out.clear();
        let held = Feature { held: Some("waiting".into()), ..feature("bump") };
        assert!(follower.receive_board(board(vec![feature("trim"), held, feature("mouse")]), &[], &FOLDED, &mut out));
        assert_eq!(follower.feature(), Some("mouse"), "bump is folded away");
        assert_eq!(names(&out), ["tasks mouse", "diagrams mouse"]);

        follower.settle(&[], &SHOWN, &mut out);
        assert_eq!(follower.feature(), Some("mouse"), "showing the folded keeps the cursor");
        let _ = follower.receive_board(board_of(&[]), &[], &FOLDED, &mut out);
        assert_eq!(follower.feature(), None, "nothing left to be on");
    }

    /// A listing in reverse, without the one feature it drops.
    struct Reversed(&'static str);

    impl FeatureListing for Reversed {
        fn list(&self, rows: Vec<FeatureRow>) -> Vec<FeatureRow> {
            rows.into_iter().rev().filter(|row| row.slug != self.0).collect()
        }
    }

    #[test]
    fn a_listing_decides_where_the_cursor_lands_when_its_feature_leaves() {
        let mut follower = BoardFollower::new(true);
        let mut out = Vec::new();
        follower.follow(Some(scope()), &mut out);
        assert!(follower.receive_board(board_of(&["trim", "bump", "mouse"]), &[], &Reversed(""), &mut out));
        assert_eq!(follower.feature(), Some("mouse"), "the first the listing lists");
        let slugs = |rows: Vec<FeatureRow>| rows.into_iter().map(|row| row.slug).collect::<Vec<_>>();
        assert_eq!(slugs(follower.listed(&[], &Reversed(""))), ["mouse", "bump", "trim"]);

        follower.point(Some("bump"), &mut out);
        follower.settle(&[], &Reversed("bump"), &mut out);
        assert_eq!(follower.feature(), Some("trim"), "the one in its place in the listing's order");
    }

    #[test]
    fn a_feature_not_on_the_board_is_not_moved_onto() {
        let mut follower = following();
        let mut out = Vec::new();
        follower.point(Some("nowhere"), &mut out);
        assert_eq!(follower.feature(), Some("trim"), "still where it was");
        assert!(out.is_empty());
    }

    #[test]
    fn a_task_reordered_under_the_cursor_is_still_under_it() {
        let mut follower = following();
        let first = tasks("trim", vec![task(1, None), task(2, None), task(3, None)]);
        assert_eq!(follower.receive_tasks(first, None), Received::Adopted { lands: None });
        let reordered = tasks("trim", vec![task(3, None), task(1, None), task(2, Some(3))]);
        assert_eq!(
            follower.receive_tasks(reordered, Some(WorkRef::Task(2))),
            Received::Adopted { lands: Some(WorkRef::Task(2)) }
        );
        let order: Vec<i64> = follower.tasks().unwrap().tasks.iter().map(|t| t.id).collect();
        let parent = order.iter().position(|id| *id == 3).unwrap();
        assert_eq!(order[parent + 1], 2, "held in tree order, a child under its parent");
    }

    #[test]
    fn a_task_removed_under_the_cursor_hands_it_to_its_parent_or_its_place() {
        let mut follower = following();
        let at = |id| Some(WorkRef::Task(id));
        let lands = |id| Received::Adopted { lands: Some(WorkRef::Task(id)) };
        let _ = follower.receive_tasks(tasks("trim", vec![task(1, None), task(4, Some(1)), task(2, None), task(3, None)]), None);
        let child_gone = tasks("trim", vec![task(1, None), task(2, None), task(3, None)]);
        assert_eq!(follower.receive_tasks(child_gone, at(4)), lands(1), "its parent");
        let top_gone = tasks("trim", vec![task(1, None), task(3, None)]);
        assert_eq!(follower.receive_tasks(top_gone, at(2)), lands(3), "the task in its place");
        let last_gone = tasks("trim", vec![task(1, None)]);
        assert_eq!(follower.receive_tasks(last_gone, at(3)), lands(1), "clamped to the end");
        assert_eq!(follower.receive_tasks(tasks("trim", vec![]), at(1)), Received::Adopted { lands: None });
        assert_eq!(
            follower.receive_tasks(tasks("trim", vec![]), Some(WorkRef::Decision(7))),
            Received::Adopted { lands: Some(WorkRef::Decision(7)) },
            "a decision is the board's, not the list's"
        );
    }

    #[test]
    fn a_diagram_is_held_by_id_across_a_push() {
        let mut follower = following();
        let _ = follower.receive_diagrams(diagrams(&[1, 2, 3]), None);
        assert_eq!(follower.receive_diagrams(diagrams(&[3, 2, 1]), Some(2)), Received::Adopted { lands: Some(2) });
        assert_eq!(follower.receive_diagrams(diagrams(&[3, 1]), Some(2)), Received::Adopted { lands: Some(1) });
    }

    #[test]
    fn moving_to_another_feature_drops_the_last_ones_lists_and_asks_for_its_own() {
        let mut follower = following();
        let _ = follower.receive_tasks(tasks("trim", vec![task(1, None)]), None);
        let mut out = Vec::new();
        follower.point(Some("bump"), &mut out);
        assert!(follower.tasks().is_none());
        assert_eq!(names(&out), ["tasks bump", "diagrams bump"]);
        follower.point(Some("bump"), &mut out);
        assert_eq!(out.len(), 2, "already on it");
    }
}
