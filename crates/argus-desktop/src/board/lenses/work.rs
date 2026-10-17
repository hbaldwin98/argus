//! The Work lens of an open feature: its tasks and the decisions raised
//! under them as one outline, and the task or decision under the cursor in
//! full.
//!
//! Where a decision sits among the tasks, what binds a task and what an
//! open question holds up are `argus_protocol::board::outline`'s to say, as
//! they are for the terminal client's Work lens; this only turns them into
//! what the page draws.

use argus_protocol::board::outline::{Outline, WorkRef};
use argus_protocol::{Changes, TaskTreeRow};

use crate::connection::page::{Criterion, DecisionDetail, Line, TaskDetail, WorkAt, WorkRow};
use crate::board::lenses::record::{decision_detail, decision_state};

pub struct Work {
    pub rows: Vec<WorkRow>,
    pub task: Option<TaskDetail>,
    pub decision: Option<DecisionDetail>,
}

/// The outline with `cursor` on one of its rows; a cursor on a row that
/// went away lands on the first. `changes` marks the rows changed since the
/// feature was last reconciled.
pub fn build(outline: &Outline, cursor: Option<WorkRef>, changes: Option<&Changes>) -> Work {
    let cursor = cursor
        .filter(|at| outline.rows.iter().any(|row| row.at == *at))
        .or_else(|| outline.rows.first().map(|row| row.at));
    let rows = outline
        .rows
        .iter()
        .map(|work| {
            let on = Some(work.at) == cursor;
            match work.at {
                WorkRef::Task(_) => {
                    let task = &outline.tasks[work.index];
                    WorkRow {
                        at: at(work.at),
                        label: task.label(),
                        depth: work.depth,
                        text: task.task.title.clone(),
                        state: task.task.state.as_str(),
                        waits_on: work.waits_on.clone(),
                        key: task.task.external.clone(),
                        changed: changes.is_some_and(|c| c.tasks.contains(&task.task.id)),
                        on,
                    }
                }
                WorkRef::Decision(_) => {
                    let decision = &outline.decisions[work.index];
                    WorkRow {
                        at: at(work.at),
                        label: decision.label(),
                        depth: work.depth,
                        text: decision.decision.chose.clone(),
                        state: decision_state(decision.decision),
                        waits_on: None,
                        key: None,
                        changed: changes.is_some_and(|c| c.decisions.contains(&decision.decision.id)),
                        on,
                    }
                }
            }
        })
        .collect();
    let (task, decision) = match cursor {
        Some(WorkRef::Task(id)) => (outline.task(id).map(|row| task_detail(row, outline)), None),
        Some(WorkRef::Decision(id)) => (None, outline.decision(id).map(|row| decision_detail(row, outline))),
        None => (None, None),
    };
    Work { rows, task, decision }
}

fn at(work: WorkRef) -> WorkAt {
    match work {
        WorkRef::Task(id) => WorkAt::Task { id },
        WorkRef::Decision(id) => WorkAt::Decision { id },
    }
}

pub fn work_ref(at: WorkAt) -> WorkRef {
    match at {
        WorkAt::Task { id } => WorkRef::Task(id),
        WorkAt::Decision { id } => WorkRef::Decision(id),
    }
}

fn task_detail(row: &TaskTreeRow, outline: &Outline) -> TaskDetail {
    let task = row.task;
    let waits_on = outline
        .rows
        .iter()
        .find(|work| work.at == WorkRef::Task(task.id))
        .and_then(|work| work.waits_on.clone());
    TaskDetail {
        id: task.id,
        label: row.label(),
        title: task.title.clone(),
        body: task.body.clone().unwrap_or_default(),
        state: task.state.as_str(),
        parent: task
            .parent
            .and_then(|parent| outline.task(parent))
            .map(|parent| format!("{} · {}", parent.label(), parent.task.title)),
        criteria: task.criteria.iter().map(|c| Criterion { text: c.text.clone(), done: c.done }).collect(),
        bound_by: outline
            .bound_by(task.id)
            .into_iter()
            .map(|row| Line { label: row.label(), text: row.decision.chose.clone(), aside: None })
            .collect(),
        notes: task
            .notes
            .iter()
            .map(|note| Line {
                label: if note.session.is_some() { "agent" } else { "you" }.into(),
                text: note.text.clone(),
                aside: None,
            })
            .collect(),
        waits_on,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use std::collections::HashSet;

    fn work(cursor: Option<WorkRef>) -> Work {
        let board = fixtures::open_board();
        let mut tasks = fixtures::open_tasks();
        tasks.tasks[3].external = Some("ENG-91".into());
        let scoped = board.scoped(Some("trim"));
        let outline = Outline::build(tasks.tree_rows(), scoped.tree_rows(), &HashSet::new());
        build(&outline, cursor, None)
    }

    #[test]
    fn the_outline_reads_tasks_and_the_decisions_raised_under_them_as_one() {
        let open = work(None);
        let rows: Vec<_> = open.rows.iter().map(|r| (r.label.as_str(), r.depth, r.state)).collect();
        assert_eq!(
            rows,
            [
                ("1", 0, "doing"),
                ("1.1", 1, "done"),
                ("1.2", 1, "doing"),
                ("D1", 1, "accepted"),
                ("2", 0, "todo"),
                ("D2", 1, "open"),
            ]
        );
        assert!(open.rows[0].on, "the cursor starts on the first row");
    }

    #[test]
    fn a_task_row_carries_its_tracker_key() {
        let open = work(None);
        assert_eq!(open.rows[4].key.as_deref(), Some("ENG-91"));
        assert_eq!(open.rows[0].key, None);
    }

    #[test]
    fn a_task_under_the_cursor_shows_its_criteria_what_binds_it_and_its_notes() {
        let open = work(Some(WorkRef::Task(3)));
        let task = open.task.unwrap();
        assert_eq!((task.label.as_str(), task.parent.as_deref()), ("1.2", Some("1 · Serve the brief on demand")));
        assert_eq!(task.criteria.iter().map(|c| c.done).collect::<Vec<_>>(), [true, false]);
        assert_eq!(task.bound_by[0].label, "D1");
        assert_eq!((task.notes[0].label.as_str(), task.notes[0].text.as_str()), ("agent", "wiring it now"));
        assert!(open.decision.is_none());
    }

    #[test]
    fn a_cursor_on_a_row_gone_moves_to_the_first() {
        let gone = work(Some(WorkRef::Task(99)));
        assert!(gone.rows[0].on && gone.task.is_some());
    }
}
