//! Tasks: the work under a feature, one row per card.
//!
//! A feature says what is being built and its decision tree says why it is
//! being built that way. Neither says what is left to do, which is what a
//! human actually wants to hand an agent. So a feature carries a list of
//! tasks, and those are drawn as a board of their own.
//!
//! A task keeps a compact title and may carry a multiline brief. Tasks can
//! be nested without a depth limit, so an agent can record newly discovered
//! work under the task that exposed it. The list stays one wire value and
//! carries parent ids; readers project it into the same depth-first tree.
//! It also has a column and optionally the key it has in whatever tracker the
//! team really uses — Argus stores that key and knows nothing else about it.
//! Populating tasks from Jira, Linear or a spreadsheet is something an agent
//! with access to that board does, which is why Argus works the same with any
//! of them, or with none.

use serde::{Deserialize, Serialize};

use crate::ids::{PaneId, RepositoryId};

/// Past this a task title has stopped being a line on a card.
pub const MAX_TASK_TITLE_BYTES: usize = 300;
/// Enough room for task-specific context and acceptance criteria without
/// turning one task into an unbounded document.
pub const MAX_TASK_BODY_BYTES: usize = 8192;
/// A criterion is one checkable line, and a task with more than a screen of
/// them is several tasks.
pub const MAX_CRITERION_BYTES: usize = 300;
pub const MAX_TASK_CRITERIA: usize = 20;
pub const MAX_TASK_NOTE_BYTES: usize = 600;
/// The notes a task keeps. Past this the oldest go: a note says where the
/// work stands, and what stood forty notes ago is the transcript's to keep.
pub const MAX_TASK_NOTES: usize = 40;

/// How far along a task is.
///
/// Three states, and unlike the feature columns they used to sit beside,
/// these are maintained by whoever is doing the work: an agent takes a
/// task up and finishes it as part of the job, so `doing` says which card
/// somebody is actually on rather than which card was last dragged.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskState {
    #[default]
    Todo,
    Doing,
    Done,
}

impl TaskState {
    pub const ALL: [TaskState; 3] = [TaskState::Todo, TaskState::Doing, TaskState::Done];

    pub fn as_str(self) -> &'static str {
        match self {
            TaskState::Todo => "todo",
            TaskState::Doing => "doing",
            TaskState::Done => "done",
        }
    }

    pub fn parse(text: &str) -> Option<TaskState> {
        TaskState::ALL
            .into_iter()
            .find(|s| s.as_str() == text.trim().to_ascii_lowercase())
    }
}

impl std::fmt::Display for TaskState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How a feature's tasks stand, without carrying the tasks themselves.
///
/// A list of features wants a progress line on every row, and the tasks
/// behind it only on the row being read. Counting in the daemon is one
/// grouped query; sending every task of every feature so the client can
/// count them itself is a list that grows with the project.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskCounts {
    pub todo: usize,
    pub doing: usize,
    pub done: usize,
}

impl TaskCounts {
    pub fn total(self) -> usize {
        self.todo + self.doing + self.done
    }

    /// Whether every task under the feature is finished. False for a
    /// feature with no tasks at all: nothing to do is not the same answer
    /// as everything done, and a progress line that claimed otherwise
    /// would read as complete on work nobody has broken down yet.
    pub fn all_done(self) -> bool {
        self.total() > 0 && self.todo == 0 && self.doing == 0
    }

    pub fn add(&mut self, state: TaskState) {
        match state {
            TaskState::Todo => self.todo += 1,
            TaskState::Doing => self.doing += 1,
            TaskState::Done => self.done += 1,
        }
    }
}

/// One thing that has to be true before a task is done.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Criterion {
    pub text: String,
    #[serde(default)]
    pub done: bool,
}

/// A line left on a task by whoever is working it, or by the person
/// steering them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskNote {
    pub at: i64,
    /// The agent session that wrote it, `None` when a person did.
    pub session: Option<String>,
    /// The pane it was written from, when the daemon knew.
    #[serde(default)]
    pub pane: Option<PaneId>,
    pub text: String,
}

/// Trims a criterion or a note and holds it to its bound.
pub fn checked_task_line(text: &str, max: usize) -> Result<String, &'static str> {
    let text = text.trim();
    if text.is_empty() {
        return Err("there is nothing written there");
    }
    if text.len() > max {
        return Err("that is a paragraph; a task's brief is where prose goes");
    }
    Ok(text.to_string())
}

/// One task, as it is stored and drawn.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    /// Stable for the life of the row, which is the whole reason a task is
    /// a row: a human rewriting the list around it must not renumber it.
    pub id: i64,
    /// The feature it is under, by slug.
    pub feature: String,
    /// The task that discovered or groups this work. `None` is a task
    /// directly under the feature. Parent ids are checked against the same
    /// feature before they are stored.
    #[serde(default)]
    pub parent: Option<i64>,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    pub state: TaskState,
    /// The agent session that took it. One task at a time is not enforced
    /// — two agents on one feature is a thing that happens, and a board
    /// that refuses to describe it is not more correct, only less useful.
    pub claimed_by: Option<String>,
    /// Whatever key the team's tracker uses, if it came from one. Opaque:
    /// Argus never parses, fetches or reconciles it.
    pub external: Option<String>,
    /// Where it sits among its siblings. A human says what to do first,
    /// which is most of what a list is for.
    pub position: i64,
    pub at: i64,
    pub session: Option<String>,
    /// The pane `claimed_by` was running in when it took the task, when the
    /// daemon knew. A session id tells a reader nothing; a pane is
    /// somewhere to go.
    #[serde(default)]
    pub claimed_pane: Option<PaneId>,
    /// What "done" means for this task, each checked off as it lands.
    #[serde(default)]
    pub criteria: Vec<Criterion>,
    /// Oldest first.
    #[serde(default)]
    pub notes: Vec<TaskNote>,
}

/// A task as it is asked for, before the store gives it an id.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskWrite {
    pub title: String,
    pub external: Option<String>,
    /// Put the new task under an existing task instead of directly under
    /// the feature. The store verifies that the parent belongs to the same
    /// feature, so an id copied from another board cannot create a hidden
    /// cross-feature edge.
    #[serde(default)]
    pub parent: Option<i64>,
}

impl TaskWrite {
    pub fn checked(self) -> Result<TaskWrite, &'static str> {
        let parent = self.parent;
        if parent.is_some_and(|id| id <= 0) {
            return Err("a parent task id must be positive");
        }
        let title = self.title.trim().to_string();
        let external = self
            .external
            .map(|e| e.trim().to_string())
            .filter(|e| !e.is_empty());
        if title.is_empty() {
            return Err("a task has to say what it is");
        }
        if title.len() > MAX_TASK_TITLE_BYTES {
            return Err("a task is a line, not a brief — that belongs in the feature document");
        }
        Ok(TaskWrite {
            title,
            external,
            parent,
        })
    }
}

/// Normalizes a task brief at every write boundary. Empty prose means no
/// brief, while meaningful indentation and trailing newlines are preserved.
pub fn checked_task_body(body: String) -> Result<Option<String>, &'static str> {
    if body.trim().is_empty() {
        return Ok(None);
    }
    if body.len() > MAX_TASK_BODY_BYTES {
        return Err("a task brief is too large");
    }
    Ok(Some(body))
}

/// A change to a feature's tasks, or the read. Both sides speak it: an agent
/// through the task endpoint, a client inside `ClientMsg::Task`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskAction {
    /// Reads the current feature's tasks and changes nothing.
    List,
    Add(TaskWrite),
    /// Moves a task to another column, claiming it on the way into
    /// `doing` and releasing it on `done`.
    Move {
        id: i64,
        state: TaskState,
    },
    /// Rewrites a task's text, which is the one part of it that is not a
    /// state. Kept separate so a move never carries a title with it.
    Retitle {
        id: i64,
        title: String,
    },
    /// Replaces the task-specific brief. Empty text clears it.
    SetBody {
        id: i64,
        body: String,
    },
    Remove {
        id: i64,
    },
    /// Puts a task at a zero-based index in its sibling list.
    Reorder {
        id: i64,
        to: i64,
    },
    /// Moves a task, and the subtasks under it, somewhere else in the tree
    /// or to another feature on the same board.
    Place {
        id: i64,
        place: TaskPlace,
    },
    /// Adds one thing that has to be true before the task is done.
    AddCriterion {
        id: i64,
        text: String,
    },
    /// Checks a criterion off, or takes that back. `index` is zero-based
    /// into the task's criteria as they were last read.
    SetCriterion {
        id: i64,
        index: usize,
        done: bool,
    },
    RemoveCriterion {
        id: i64,
        index: usize,
    },
    /// Leaves a line on the task.
    Note {
        id: i64,
        text: String,
    },
}

/// Where [`TaskAction::Place`] puts a task. Named against another task
/// rather than as an index, so an agent says what it means and the daemon,
/// which holds the tree, works out the numbers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskPlace {
    /// Last among this task's subtasks.
    Under(i64),
    /// Last among the feature's top-level tasks.
    Top,
    /// Just before this task, beside it under its parent.
    Before(i64),
    /// Just after this task, beside it under its parent.
    After(i64),
    /// Last among another feature's top-level tasks.
    Feature(String),
}

/// A feature's tasks, which is how both sides read them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskList {
    pub project_name: String,
    /// `None` when the checkout that asked is on no feature, which is the
    /// case the answer has to teach rather than report as empty.
    pub feature: Option<String>,
    /// Tasks are sent in depth-first order. Each row still carries its
    /// parent id so a consumer that does not use the renderer can rebuild
    /// the hierarchy without relying on ordering.
    pub tasks: Vec<Task>,
    /// The repositories in the tree whose board this is, so a client keeps
    /// only its own: every board is pushed to every client, and the
    /// repositories of a project share its name. Empty from a daemon that
    /// does not say, which leaves the name to go by.
    #[serde(default)]
    pub repositories: Vec<RepositoryId>,
}

/// One task row with the topology a renderer needs for branch guides.
///
/// The wire stays a flat vector because that is cheap to append to and easy
/// for agents to parse. This borrowed projection makes the hierarchy
/// visible without making every client reimplement parent lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskTreeRow<'a> {
    pub depth: usize,
    pub task: &'a Task,
    /// Whether each non-root ancestor has a sibling below this row.
    pub ancestor_continuations: Vec<bool>,
    pub has_next_sibling: bool,
    pub has_children: bool,
    /// Its one-based place among its siblings at each depth: `[2, 2]` is
    /// task `2.2`. For reading; a write names the task by `id`.
    pub number: Vec<usize>,
}

impl TaskTreeRow<'_> {
    pub fn label(&self) -> String {
        crate::decisions::outline_number(&self.number)
    }
}

impl TaskList {
    /// Whether this is a list from the board of `repository` in the
    /// project named `project_name`; see [`crate::decisions::is_for`].
    pub fn is_for(&self, project_name: &str, repository: RepositoryId) -> bool {
        crate::decisions::is_for(&self.project_name, &self.repositories, project_name, repository)
    }

    /// The tasks flattened depth-first, with sibling order taken from their
    /// stored positions. Missing parents are treated as roots rather than
    /// making a malformed row disappear.
    pub fn tree_rows(&self) -> Vec<TaskTreeRow<'_>> {
        let known: std::collections::HashSet<i64> = self.tasks.iter().map(|task| task.id).collect();
        let mut children: std::collections::HashMap<Option<i64>, Vec<&Task>> =
            std::collections::HashMap::new();
        for task in &self.tasks {
            let parent = task.parent.filter(|parent| known.contains(parent));
            children.entry(parent).or_default().push(task);
        }
        for siblings in children.values_mut() {
            siblings.sort_by_key(|task| (task.position, task.id));
        }

        let mut out = Vec::new();
        let mut visited = std::collections::HashSet::new();
        walk_tasks(&children, None, &mut Vec::new(), &mut Vec::new(), &mut visited, &mut out);
        // A cycle cannot be created through the normal write path, but the
        // database intentionally has no foreign key so an interrupted or
        // hand-edited store must not hang a client while it renders. Start
        // any cycle member as a visible root and let `visited` stop the loop.
        let mut roots = children.get(&None).map_or(0, Vec::len);
        for task in &self.tasks {
            if visited.insert(task.id) {
                roots += 1;
                out.push(TaskTreeRow {
                    depth: 0,
                    task,
                    ancestor_continuations: Vec::new(),
                    has_next_sibling: false,
                    has_children: children.contains_key(&Some(task.id)),
                    number: vec![roots],
                });
                walk_tasks(
                    &children,
                    Some(task.id),
                    &mut vec![roots],
                    &mut Vec::new(),
                    &mut visited,
                    &mut out,
                );
            }
        }
        out
    }

    /// Returns owned tasks in the order a human or agent should read them.
    /// Store reads use this once after their single query; clients use the
    /// same projection when accepting a list from an older peer.
    pub fn tasks_in_tree_order(&self) -> Vec<Task> {
        self.tree_rows()
            .into_iter()
            .map(|row| row.task.clone())
            .collect()
    }
}

fn walk_tasks<'a>(
    children: &std::collections::HashMap<Option<i64>, Vec<&'a Task>>,
    parent: Option<i64>,
    number: &mut Vec<usize>,
    ancestor_continuations: &mut Vec<bool>,
    visited: &mut std::collections::HashSet<i64>,
    out: &mut Vec<TaskTreeRow<'a>>,
) {
    let Some(siblings) = children.get(&parent) else {
        return;
    };
    let depth = number.len();
    for (index, task) in siblings.iter().enumerate() {
        if !visited.insert(task.id) {
            continue;
        }
        let has_next_sibling = index + 1 < siblings.len();
        number.push(index + 1);
        out.push(TaskTreeRow {
            depth,
            task,
            ancestor_continuations: ancestor_continuations.clone(),
            has_next_sibling,
            has_children: children.contains_key(&Some(task.id)),
            number: number.clone(),
        });
        if depth > 0 {
            ancestor_continuations.push(has_next_sibling);
        }
        walk_tasks(
            children,
            Some(task.id),
            number,
            ancestor_continuations,
            visited,
            out,
        );
        if depth > 0 {
            ancestor_continuations.pop();
        }
        number.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_state_survives_the_round_trip_through_its_name() {
        for state in TaskState::ALL {
            assert_eq!(TaskState::parse(state.as_str()), Some(state));
        }
        assert_eq!(TaskState::parse("blocked"), None);
    }

    #[test]
    fn a_task_has_to_say_what_it_is() {
        assert!(TaskWrite {
            title: "   ".into(),
            external: None,
            parent: None,
        }
        .checked()
        .is_err());
        let ok = TaskWrite {
            title: "  backpressure on the reader  ".into(),
            external: Some("  ORION-412 ".into()),
            parent: Some(7),
        }
        .checked()
        .unwrap();
        assert_eq!(ok.title, "backpressure on the reader");
        assert_eq!(ok.external.as_deref(), Some("ORION-412"));
        assert_eq!(ok.parent, Some(7));
    }

    #[test]
    fn a_parent_task_id_has_to_be_positive() {
        assert!(TaskWrite {
            title: "child".into(),
            external: None,
            parent: Some(0),
        }
        .checked()
        .is_err());
    }

    #[test]
    fn an_empty_tracker_key_is_no_key_at_all() {
        let task = TaskWrite {
            title: "port the parser".into(),
            external: Some("  ".into()),
            parent: None,
        }
        .checked()
        .unwrap();
        assert_eq!(task.external, None, "a blank key would draw as a label");
    }

    #[test]
    fn a_task_brief_is_optional_but_bounded() {
        assert_eq!(checked_task_body("  \n".into()).unwrap(), None);
        assert_eq!(
            checked_task_body("Why this matters\n\n- observable result\n".into()).unwrap(),
            Some("Why this matters\n\n- observable result\n".into())
        );
        assert!(checked_task_body("x".repeat(MAX_TASK_BODY_BYTES + 1)).is_err());
    }

    fn task(id: i64, parent: Option<i64>, position: i64, title: &str) -> Task {
        Task {
            id,
            feature: "feature".into(),
            parent,
            title: title.into(),
            body: None,
            state: TaskState::Todo,
            claimed_by: None,
            external: None,
            position,
            at: 0,
            session: None,
            ..Task::default()
        }
    }

    #[test]
    fn tasks_and_decisions_are_numbered_by_their_place_in_the_outline() {
        let list = TaskList {
            repositories: Vec::new(),
            project_name: "argus".into(),
            feature: Some("feature".into()),
            tasks: vec![
                task(1, None, 0, "root"),
                task(2, None, 1, "second"),
                task(3, Some(2), 0, "child"),
                task(4, Some(2), 1, "sibling"),
            ],
        };

        let labels: Vec<String> = list.tree_rows().iter().map(TaskTreeRow::label).collect();

        assert_eq!(labels, ["1", "2", "2.1", "2.2"]);
    }

    #[test]
    fn nested_tasks_are_read_depth_first_in_sibling_order() {
        let list = TaskList {
            repositories: Vec::new(),
            project_name: "argus".into(),
            feature: Some("feature".into()),
            tasks: vec![
                task(1, None, 0, "parent"),
                task(2, Some(1), 1, "second child"),
                task(3, Some(1), 0, "first child"),
                task(4, Some(3), 0, "grandchild"),
                task(5, None, 1, "second root"),
            ],
        };
        let rows = list.tree_rows();
        assert_eq!(
            rows.iter()
                .map(|row| (row.depth, row.task.id))
                .collect::<Vec<_>>(),
            [(0, 1), (1, 3), (2, 4), (1, 2), (0, 5)]
        );
        assert!(rows[0].has_children);
        assert!(rows[1].has_next_sibling);
        assert_eq!(rows[2].ancestor_continuations, [true]);
    }
}
