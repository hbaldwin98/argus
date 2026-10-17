//! How a feature stands on the index — Active, Queued, Done, Held or
//! Archived — what a row of the index says, and which task comes next.
//!
//! Read off what Argus observes, the panes on a feature's checkouts and the
//! state of its tasks, except for the two a person sets: a hold, which
//! carries its reason, and the archive, which is acceptance. Here rather
//! than in a client because two clients filing one feature under two
//! headings would be two products.

use crate::decisions::DecisionBoard;
use crate::features::{Feature, FeatureState};
use crate::tree::held::HeldTree;
use crate::tree::ids::PaneId;
use crate::tasks::{TaskCounts, TaskState, TaskTreeRow};
use crate::tree::{PaneInfo, PaneKind, PaneStatus, ProjectInfo};

/// How a feature stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    /// Nothing in it yet: no brief, no tasks, no decisions.
    New,
    /// Written down and not started, or after a feature still open.
    Queued,
    Active,
    /// Every task done, and nobody has archived it.
    Done,
    Held,
    Archived,
}

impl Standing {
    pub fn label(self) -> &'static str {
        match self {
            Standing::New => "NEW",
            Standing::Queued => "QUEUED",
            Standing::Active => "ACTIVE",
            Standing::Done => "DONE",
            Standing::Held => "HELD",
            Standing::Archived => "ARCHIVED",
        }
    }

    /// The heading it is listed under. A new feature is queued work: it
    /// differs only in what its own page offers.
    pub fn group(self) -> Standing {
        match self {
            Standing::New => Standing::Queued,
            other => other,
        }
    }

    /// Out of the way until asked for: neither is work to pick up.
    pub fn folded(self) -> bool {
        matches!(self, Standing::Held | Standing::Archived)
    }

    /// Whether a person can put a feature here. The rest follow the work:
    /// a feature moves into them when a task starts, an agent arrives or
    /// the last task closes, never by hand.
    pub fn set_by_hand(self) -> bool {
        matches!(self, Standing::Held | Standing::Archived)
    }
}

/// The order the index lists its groups in.
pub const GROUPS: [Standing; 5] = [
    Standing::Active,
    Standing::Queued,
    Standing::Done,
    Standing::Held,
    Standing::Archived,
];

/// One feature as the index lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureRow {
    pub slug: String,
    pub number: i64,
    pub title: String,
    pub standing: Standing,
    /// Set when an agent on this feature has stopped for a person.
    pub attention: Option<PaneStatus>,
    pub tasks: TaskCounts,
    /// Decisions that stand, and questions still open.
    pub decided: usize,
    pub open: usize,
    /// The branch it is being worked on, or was cut on.
    pub branch: Option<String>,
    /// The agent pane a person would go to: the loudest on its checkouts.
    pub pane: Option<PaneId>,
    /// The numbers of the features it comes after that are still open.
    pub after: Vec<i64>,
}

/// Every feature on the board, grouped in the order of [`GROUPS`] and in
/// the board's own order within a group. Held and archived ones are
/// included; a client folds them away.
pub fn feature_rows(board: &DecisionBoard, tree: &[ProjectInfo]) -> Vec<FeatureRow> {
    let mut rows: Vec<FeatureRow> = board
        .features
        .iter()
        .map(|feature| feature_row(feature, board, tree))
        .collect();
    let place = |row: &FeatureRow| GROUPS.iter().position(|group| *group == row.standing.group());
    rows.sort_by_key(place);
    rows
}

fn feature_row(feature: &Feature, board: &DecisionBoard, tree: &[ProjectInfo]) -> FeatureRow {
    let (decided, open) = board.standing_for(Some(&feature.slug));
    let panes = panes_on(feature, tree);
    let loudest = panes
        .iter()
        .max_by_key(|pane| pane.loudest_state().status.urgency());
    let waiting: Vec<&str> = feature.waiting_on(&board.features).collect();
    let started = feature.tasks.doing + feature.tasks.done > 0 || !panes.is_empty();
    let empty = feature.tasks.total() == 0 && decided + open == 0 && feature.body.trim().is_empty();
    let standing = if feature.state == FeatureState::Done {
        Standing::Archived
    } else if feature.held.is_some() {
        Standing::Held
    } else if feature.tasks.all_done() {
        Standing::Done
    } else if !waiting.is_empty() {
        Standing::Queued
    } else if started {
        Standing::Active
    } else if empty {
        Standing::New
    } else {
        Standing::Queued
    };
    let worked_on = tree.checkouts()
        .find(|checkout| feature.checkouts.iter().any(|path| path == &checkout.path))
        .map(|checkout| checkout.name.clone());
    FeatureRow {
        slug: feature.slug.clone(),
        number: feature.number,
        title: feature.title.clone(),
        standing,
        attention: loudest
            .map(|pane| pane.loudest_state().status)
            .filter(|status| status.needs_you()),
        tasks: feature.tasks,
        decided,
        open,
        branch: worked_on.or_else(|| feature.origin_branch.clone()),
        pane: loudest.map(|pane| pane.id),
        after: board
            .features
            .iter()
            .filter(|other| waiting.contains(&other.slug.as_str()))
            .map(|other| other.number)
            .collect(),
    }
}

/// The live agent panes on the checkouts a feature is worked in.
pub fn panes_on<'a>(feature: &Feature, tree: &'a [ProjectInfo]) -> Vec<&'a PaneInfo> {
    tree.checkouts()
        .filter(|checkout| feature.checkouts.iter().any(|path| path == &checkout.path))
        .flat_map(|checkout| checkout.panes.iter())
        .filter(|pane| pane.kind == PaneKind::Agent)
        .filter(|pane| !matches!(pane.status, PaneStatus::Exited { .. }))
        .collect()
}

/// The task that comes next under a feature: the one being done, else the
/// first not done. Only leaves count; a task with subtasks is done by them.
pub fn next_task<'a>(rows: &[TaskTreeRow<'a>]) -> Option<TaskTreeRow<'a>> {
    let leaf = |row: &&TaskTreeRow<'a>| !row.has_children;
    let doing = rows.iter().filter(leaf).find(|row| row.task.state == TaskState::Doing);
    let todo = rows.iter().filter(leaf).find(|row| row.task.state == TaskState::Todo);
    doing.or(todo).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_and_archived_features_are_the_ones_folded_away_and_set_by_hand() {
        let folded: Vec<Standing> = GROUPS.into_iter().filter(|s| s.folded()).collect();
        assert_eq!(folded, [Standing::Held, Standing::Archived]);
        let by_hand: Vec<Standing> = GROUPS.into_iter().filter(|s| s.set_by_hand()).collect();
        assert_eq!(by_hand, folded);
        assert_eq!(Standing::New.group(), Standing::Queued);
    }
}
