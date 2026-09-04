//! Tasks: the work under a feature, one row per card.
//!
//! A feature says what is being built and its decision tree says why it is
//! being built that way. Neither says what is left to do, which is what a
//! human actually wants to hand an agent. So a feature carries a list of
//! tasks, and those are drawn as a board of their own.
//!
//! A task is deliberately thin. It is a line of text, a column, and
//! optionally the key it has in whatever tracker the team really uses —
//! Argus stores that key and knows nothing else about it. Populating tasks
//! from Jira, Linear or a spreadsheet is something an agent with access to
//! that board does, which is why Argus works the same with any of them.

use serde::{Deserialize, Serialize};

/// Past this a task has stopped being a line on a card and become the
/// feature document it belongs under.
pub const MAX_TASK_TITLE_BYTES: usize = 300;

/// Which column of the task board a task sits in.
///
/// Three, not the feature's five. A feature is a piece of work with a
/// review step and a person who accepts it; a task is a thing on a list,
/// and giving it its own review column would ask for a ceremony nobody
/// performs on a checklist item.
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

/// One task, as it is stored and drawn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    /// Stable for the life of the row, which is the whole reason a task is
    /// a row: a human rewriting the list around it must not renumber it.
    pub id: i64,
    /// The feature it is under, by slug.
    pub feature: String,
    pub title: String,
    pub state: TaskState,
    /// The agent session that took it. One task at a time is not enforced
    /// — two agents on one feature is a thing that happens, and a board
    /// that refuses to describe it is not more correct, only less useful.
    pub claimed_by: Option<String>,
    /// Whatever key the team's tracker uses, if it came from one. Opaque:
    /// Argus never parses, fetches or reconciles it.
    pub external: Option<String>,
    /// Where it sits in its column. A human says what to do first, which
    /// is most of what a list is for.
    pub position: i64,
    pub at: i64,
    pub session: Option<String>,
}

/// A task as it is asked for, before the store gives it an id.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskWrite {
    pub title: String,
    pub external: Option<String>,
}

impl TaskWrite {
    pub fn checked(self) -> Result<TaskWrite, &'static str> {
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
        Ok(TaskWrite { title, external })
    }
}

/// What an agent asks the task endpoint to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskAction {
    /// Reads the current feature's tasks and changes nothing.
    List,
    Add(TaskWrite),
    /// Moves a task to another column, claiming it on the way into
    /// `doing` and releasing it on `done`.
    Move { id: i64, state: TaskState },
    /// Rewrites a task's text, which is the one part of it that is not a
    /// state. Kept separate so a move never carries a title with it.
    Retitle { id: i64, title: String },
    Remove { id: i64 },
}

/// A feature's tasks, which is how both sides read them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskList {
    pub project_name: String,
    /// `None` when the checkout that asked is on no feature, which is the
    /// case the answer has to teach rather than report as empty.
    pub feature: Option<String>,
    pub tasks: Vec<Task>,
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
            external: None
        }
        .checked()
        .is_err());
        let ok = TaskWrite {
            title: "  backpressure on the reader  ".into(),
            external: Some("  ORION-412 ".into()),
        }
        .checked()
        .unwrap();
        assert_eq!(ok.title, "backpressure on the reader");
        assert_eq!(ok.external.as_deref(), Some("ORION-412"));
    }

    #[test]
    fn an_empty_tracker_key_is_no_key_at_all() {
        let task = TaskWrite {
            title: "port the parser".into(),
            external: Some("  ".into()),
        }
        .checked()
        .unwrap();
        assert_eq!(task.external, None, "a blank key would draw as a label");
    }
}
