//! How a feature's tasks and the decisions raised under them read as one
//! outline.
//!
//! A decision names the task it was raised under, and that one link is what
//! everything here is read off: where the decision sits among the tasks,
//! which tasks it binds, which carry it out, and which an open one holds
//! up. Nothing is stored about any of those, so none of them can go stale.

use std::collections::{HashMap, HashSet};

use crate::board::decisions::DecisionTreeRow;
use crate::board::tasks::{TaskState, TaskTreeRow};

/// What a row of the outline is, by the id a write would name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkRef {
    Task(i64),
    Decision(i64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkRow {
    pub at: WorkRef,
    /// How far in the outline draws it.
    pub depth: usize,
    /// Its place in the task or decision rows the outline was built from.
    pub index: usize,
    /// The label of the open decision holding this task up, if one does.
    pub waits_on: Option<String>,
}

/// The outline, and the questions the detail column asks of it.
pub struct Outline<'a> {
    pub tasks: Vec<TaskTreeRow<'a>>,
    pub decisions: Vec<DecisionTreeRow<'a>>,
    pub rows: Vec<WorkRow>,
    /// Each decision's task: its own, or the nearest one above it that has
    /// one. A sub-decision is raised where its parent was.
    anchors: HashMap<i64, i64>,
}

impl<'a> Outline<'a> {
    pub fn build(
        tasks: Vec<TaskTreeRow<'a>>,
        decisions: Vec<DecisionTreeRow<'a>>,
        folded: &HashSet<i64>,
    ) -> Outline<'a> {
        let known: HashSet<i64> = tasks.iter().map(|row| row.task.id).collect();
        let by_id: HashMap<i64, usize> = decisions
            .iter()
            .enumerate()
            .map(|(index, row)| (row.decision.id, index))
            .collect();
        let mut anchors = HashMap::new();
        for row in &decisions {
            let mut at = Some(row.decision);
            // Bounded by the board's size, so a parent cycle in a
            // hand-edited store cannot hang a frame.
            for _ in 0..=decisions.len() {
                let Some(decision) = at else { break };
                if let Some(task) = decision.task.filter(|task| known.contains(task)) {
                    anchors.insert(row.decision.id, task);
                    break;
                }
                at = decision
                    .parent
                    .and_then(|parent| by_id.get(&parent))
                    .map(|index| decisions[*index].decision);
            }
        }

        let mut outline = Outline {
            tasks,
            decisions,
            rows: Vec::new(),
            anchors,
        };
        let mut children: HashMap<Option<i64>, Vec<usize>> = HashMap::new();
        for (index, row) in outline.tasks.iter().enumerate() {
            let parent = if row.depth == 0 { None } else { row.task.parent };
            children.entry(parent).or_default().push(index);
        }
        outline.walk(&children, None, None, folded);
        // Decisions raised under no task close the outline: they bind the
        // feature as a whole.
        let unfiled: Vec<usize> = (0..outline.decisions.len())
            .filter(|index| !outline.anchors.contains_key(&outline.decisions[*index].decision.id))
            .collect();
        outline.push_decisions(&unfiled, 0);
        outline
    }

    fn walk(
        &mut self,
        children: &HashMap<Option<i64>, Vec<usize>>,
        parent: Option<i64>,
        held_by: Option<&str>,
        folded: &HashSet<i64>,
    ) {
        let Some(siblings) = children.get(&parent) else {
            return;
        };
        for &index in siblings {
            let (id, depth) = (self.tasks[index].task.id, self.tasks[index].depth);
            self.rows.push(WorkRow {
                at: WorkRef::Task(id),
                depth,
                index,
                waits_on: held_by.map(str::to_string),
            });
            if folded.contains(&id) {
                continue;
            }
            let raised: Vec<usize> = (0..self.decisions.len())
                .filter(|at| self.anchors.get(&self.decisions[*at].decision.id) == Some(&id))
                .collect();
            let holding = raised
                .iter()
                .find(|at| self.decisions[**at].decision.open())
                .map(|at| self.decisions[*at].label());
            // A question leads the work it holds up; a decision already
            // made follows the work that carries it out.
            if holding.is_some() {
                self.push_decisions(&raised, depth + 1);
            }
            let held = holding.as_deref().or(held_by);
            self.walk(children, Some(id), held, folded);
            if holding.is_none() {
                self.push_decisions(&raised, depth + 1);
            }
        }
    }

    fn push_decisions(&mut self, indexes: &[usize], depth: usize) {
        let shallowest = indexes
            .iter()
            .map(|index| self.decisions[*index].depth)
            .min()
            .unwrap_or(0);
        for &index in indexes {
            let row = &self.decisions[index];
            self.rows.push(WorkRow {
                at: WorkRef::Decision(row.decision.id),
                depth: depth + row.depth - shallowest,
                index,
                waits_on: None,
            });
        }
    }

    pub fn task(&self, id: i64) -> Option<&TaskTreeRow<'a>> {
        self.tasks.iter().find(|row| row.task.id == id)
    }

    pub fn decision(&self, id: i64) -> Option<&DecisionTreeRow<'a>> {
        self.decisions.iter().find(|row| row.decision.id == id)
    }

    /// The task a decision was raised under.
    pub fn raised_under(&self, decision: i64) -> Option<&TaskTreeRow<'a>> {
        self.task(*self.anchors.get(&decision)?)
    }

    /// The decisions that stand over a task: those raised under it or
    /// under any task above it.
    pub fn bound_by(&self, task: i64) -> Vec<&DecisionTreeRow<'a>> {
        let mut above = HashSet::new();
        let mut at = Some(task);
        while let Some(id) = at.filter(|id| above.insert(*id)) {
            at = self.task(id).and_then(|row| row.task.parent);
        }
        self.decisions
            .iter()
            .filter(|row| row.decision.stands())
            .filter(|row| {
                self.anchors
                    .get(&row.decision.id)
                    .is_some_and(|anchor| above.contains(anchor))
            })
            .collect()
    }

    /// The tasks that carry a decision out: what is under the task it was
    /// raised under, or that task itself when it has nothing under it.
    pub fn carried_out_by(&self, decision: i64) -> Vec<&TaskTreeRow<'a>> {
        let Some(anchor) = self.anchors.get(&decision) else {
            return Vec::new();
        };
        let under: Vec<&TaskTreeRow<'a>> = self
            .tasks
            .iter()
            .filter(|row| row.task.parent == Some(*anchor))
            .collect();
        if under.is_empty() {
            return self.task(*anchor).into_iter().collect();
        }
        under
    }

    /// How a task's own subtasks stand, as done and total.
    pub fn progress(&self, task: i64) -> (usize, usize) {
        let under = self.tasks.iter().filter(|row| row.task.parent == Some(task));
        under.fold((0, 0), |(done, total), row| {
            (done + usize::from(row.task.state == TaskState::Done), total + 1)
        })
    }

    /// How many decisions were raised under a task, itself only.
    pub fn raised_count(&self, task: i64) -> usize {
        self.anchors.values().filter(|anchor| **anchor == task).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::decisions::{Decision, DecisionBoard, DecisionState};
    use crate::board::tasks::{Task, TaskList};

    fn task(id: i64, parent: Option<i64>, state: TaskState) -> Task {
        Task {
            id,
            parent,
            position: id,
            state,
            title: format!("task {id}"),
            feature: "f".into(),
            ..Task::default()
        }
    }

    fn decision(id: i64, parent: Option<i64>, task: Option<i64>, state: DecisionState) -> Decision {
        Decision {
            id,
            parent,
            task,
            state,
            chose: format!("decision {id}"),
            ..Decision::default()
        }
    }

    fn fixture() -> (TaskList, DecisionBoard) {
        let tasks = TaskList {
            repositories: Vec::new(),
            project_name: "argus".into(),
            feature: Some("f".into()),
            tasks: vec![
                task(1, None, TaskState::Done),
                task(2, None, TaskState::Doing),
                task(3, Some(2), TaskState::Done),
                task(4, Some(2), TaskState::Doing),
                task(5, None, TaskState::Todo),
                task(6, Some(5), TaskState::Todo),
            ],
        };
        let board = DecisionBoard {
            decisions: vec![
                decision(10, None, Some(2), DecisionState::Accepted),
                decision(11, Some(10), None, DecisionState::Accepted),
                decision(12, Some(10), None, DecisionState::Rejected),
                decision(13, None, Some(5), DecisionState::Open),
                decision(14, None, None, DecisionState::Accepted),
            ],
            ..DecisionBoard::default()
        };
        (tasks, board)
    }

    fn order(outline: &Outline) -> Vec<(WorkRef, usize)> {
        outline.rows.iter().map(|row| (row.at, row.depth)).collect()
    }

    #[test]
    fn a_made_decision_follows_the_work_and_a_question_leads_it() {
        let (tasks, board) = fixture();
        let outline = Outline::build(tasks.tree_rows(), board.tree_rows(), &HashSet::new());

        use WorkRef::{Decision as D, Task as T};
        assert_eq!(
            order(&outline),
            [
                (T(1), 0),
                (T(2), 0),
                (T(3), 1),
                (T(4), 1),
                (D(10), 1),
                (D(11), 2),
                (D(12), 2),
                (T(5), 0),
                (D(13), 1),
                (T(6), 1),
                (D(14), 0),
            ]
        );
    }

    #[test]
    fn an_open_decision_holds_the_tasks_beneath_the_one_it_was_raised_under() {
        let (tasks, board) = fixture();
        let outline = Outline::build(tasks.tree_rows(), board.tree_rows(), &HashSet::new());

        let waits = |id| {
            outline
                .rows
                .iter()
                .find(|row| row.at == WorkRef::Task(id))
                .and_then(|row| row.waits_on.clone())
        };
        assert_eq!(waits(6).as_deref(), Some("D2"));
        assert_eq!(waits(5), None);
        assert_eq!(waits(4), None);
    }

    #[test]
    fn a_task_is_bound_by_what_stands_over_it_and_a_decision_is_carried_out_beneath_its_task() {
        let (tasks, board) = fixture();
        let outline = Outline::build(tasks.tree_rows(), board.tree_rows(), &HashSet::new());

        let bound: Vec<i64> = outline.bound_by(4).iter().map(|row| row.decision.id).collect();
        let carried: Vec<i64> = outline.carried_out_by(11).iter().map(|row| row.task.id).collect();

        assert_eq!(bound, [10, 11]);
        assert_eq!(carried, [3, 4]);
        assert_eq!(outline.raised_under(12).map(|row| row.task.id), Some(2));
        assert_eq!(outline.progress(2), (1, 2));
    }

    #[test]
    fn a_folded_task_hides_what_is_under_it() {
        let (tasks, board) = fixture();
        let outline = Outline::build(tasks.tree_rows(), board.tree_rows(), &HashSet::from([2]));

        assert!(!outline.rows.iter().any(|row| row.at == WorkRef::Task(3)));
        assert!(!outline.rows.iter().any(|row| row.at == WorkRef::Decision(10)));
        assert_eq!(outline.raised_count(2), 3);
    }
}
