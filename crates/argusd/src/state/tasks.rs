//! The tasks under a feature: what is left to do, as opposed to what is
//! being built (the feature) or why it is built that way (the decisions).
//!
//! Both sides write here, and for once they write the same things. A human
//! populates a list by hand or asks an agent to read it out of whatever
//! tracker the team uses; an agent takes a task up, finishes it, adds what
//! it found on the way under the task that exposed it, and moves a task it
//! filed in the wrong place. There is no acceptance step and so no move
//! either side is refused — that ceremony belongs to the feature the tasks
//! are under, which is where a human accepts the work as a whole.
//!
//! So there is one way in, [`TaskAction`], routed like every other part of
//! a feature (`board_parts`); this says only how a task is stored.

use argus_protocol::{TaskAction, TaskList, TaskPlace};

use super::board_parts::{BoardTarget, FeaturePart};
use super::*;
use crate::store::Store;

impl FeaturePart for TaskAction {
    type List = TaskList;
    const NOUN: &'static str = "task";

    fn is_read(&self) -> bool {
        matches!(self, TaskAction::List)
    }

    fn is_add(&self) -> bool {
        matches!(self, TaskAction::Add(_))
    }

    fn id(&self) -> Option<i64> {
        match self {
            TaskAction::List | TaskAction::Add(_) => None,
            TaskAction::Move { id, .. }
            | TaskAction::Retitle { id, .. }
            | TaskAction::SetBody { id, .. }
            | TaskAction::Remove { id }
            | TaskAction::Reorder { id, .. }
            | TaskAction::Place { id, .. } => Some(*id),
        }
    }

    fn also_changes(&self) -> Option<&str> {
        match self {
            TaskAction::Place {
                place: TaskPlace::Feature(slug),
                ..
            } => Some(slug),
            _ => None,
        }
    }

    fn ids(store: &Store, key: &str, feature: &str) -> anyhow::Result<Vec<i64>> {
        Ok(store.tasks(key, feature)?.iter().map(|t| t.id).collect())
    }

    fn apply(
        self,
        store: &Store,
        key: &str,
        feature: &str,
        at: i64,
        session: Option<&str>,
    ) -> anyhow::Result<()> {
        match self {
            TaskAction::List => {}
            TaskAction::Add(write) => {
                let write = write.checked().map_err(|e| anyhow::anyhow!("{e}"))?;
                store.add_task(key, feature, &write, at, session)?;
            }
            TaskAction::Move { id, state } => store.move_task(key, id, state, session)?,
            TaskAction::Retitle { id, title } => store.retitle_task(key, id, &title)?,
            TaskAction::SetBody { id, body } => store.set_task_body(key, id, body)?,
            TaskAction::Remove { id } => store.remove_task(key, id)?,
            TaskAction::Reorder { id, to } => store.reorder_task(key, id, to)?,
            TaskAction::Place { id, place } => {
                store.place_task(key, id, &place)?;
            }
        }
        Ok(())
    }

    fn list(store: &Store, target: &BoardTarget) -> anyhow::Result<TaskList> {
        Ok(TaskList {
            project_name: target.project_name.clone(),
            feature: Some(target.feature.clone()),
            tasks: store.tasks(&target.key, &target.feature)?,
        })
    }

    fn empty(project_name: String) -> TaskList {
        TaskList {
            project_name,
            feature: None,
            tasks: Vec::new(),
        }
    }

    /// A task list is watched while it is worked — that is most of why it
    /// is on screen — so a client holding one open does not have to ask.
    ///
    /// The features go with it. Every feature row carries how its tasks
    /// stand, so a list that changed without the features following it
    /// would leave the row above the list contradicting the list.
    fn broadcast(daemon: &Daemon, target: &BoardTarget) {
        let BoardTarget {
            project_name,
            key,
            feature,
        } = target;
        let tasks = daemon.store.tasks(key, feature).unwrap_or_default();
        let _ = daemon.tasks_tx.send(TaskList {
            project_name: project_name.clone(),
            feature: Some(feature.clone()),
            tasks,
        });
        daemon.broadcast_decisions(
            project_name,
            key,
            daemon.store.decisions(key).unwrap_or_default(),
        );
    }
}
