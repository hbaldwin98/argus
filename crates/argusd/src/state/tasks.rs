//! The tasks under a feature: what is left to do, as opposed to what is
//! being built (the feature) or why it is built that way (the decisions).
//!
//! Both sides write here, and for once they write the same things. A human
//! populates a list by hand or asks an agent to read it out of whatever
//! tracker the team uses; an agent takes a task up, finishes it, and adds
//! what it found on the way. There is no acceptance step and so no move
//! either side is refused — that ceremony belongs to the feature the tasks
//! are under, which is where a human accepts the work as a whole.
//!
//! So there is one way in, [`TaskAction`], and one place it is applied.
//! The two callers differ only in how they name the feature: an agent is
//! on whichever feature its checkout points at, and a client names it.

use argus_protocol::{ArtifactScope, PaneId, ProjectId, Task, TaskAction, TaskList};

use super::*;

/// The one feature a task change lands in, however the caller named it.
struct TaskTarget {
    project_name: String,
    key: String,
    feature: String,
}

impl Daemon {
    /// A task change from an agent, applied to the feature its checkout is
    /// on, answered with the list as it stands afterwards — an agent that
    /// has just added three tasks needs the ids they were given before it
    /// can take one up.
    ///
    /// Refused when the checkout is on no feature, for the same reason a
    /// decision is: a task with nothing to be under is the pile all of this
    /// exists to end. A read there is an empty list rather than an error.
    pub fn task_action_for_agent(
        &self,
        pane_id: PaneId,
        session: Option<&str>,
        action: TaskAction,
        artifact_scope: ArtifactScope,
    ) -> anyhow::Result<TaskList> {
        let scope = self.agent_scope(pane_id)?;
        let key = scope.artifact_key(artifact_scope).to_string();
        let Some(feature) = self.feature_for_agent(&scope, artifact_scope)? else {
            return match action {
                TaskAction::List => Ok(TaskList {
                    project_name: scope.project_name,
                    feature: None,
                    tasks: Vec::new(),
                }),
                TaskAction::Add(_) => anyhow::bail!(
                    "this checkout is not on a feature yet — open one with \
                     `argus-hook feature open` before adding tasks to it"
                ),
                _ => anyhow::bail!("this checkout is not on a feature yet"),
            };
        };
        match &action {
            TaskAction::Reorder { .. } => {
                anyhow::bail!("the order of a feature's tasks is the human's to set")
            }
            TaskAction::Move { id, .. }
            | TaskAction::Retitle { id, .. }
            | TaskAction::SetBody { id, .. }
            | TaskAction::Remove { id } => self.guard_task(&key, &feature, *id)?,
            TaskAction::List | TaskAction::Add(_) => {}
        }
        let target = TaskTarget {
            project_name: scope.project_name,
            key,
            feature,
        };
        self.apply_task(&target, action, session)
    }

    /// A task change from the feature view, which names its feature
    /// outright rather than resolving one from a checkout.
    pub fn task_action_for_client(
        &self,
        project: ProjectId,
        checkout: CheckoutId,
        feature: &str,
        action: TaskAction,
    ) -> anyhow::Result<TaskList> {
        let (project_name, key) = self.client_artifact_scope(project, checkout)?;
        let target = TaskTarget {
            project_name,
            key,
            feature: feature.to_string(),
        };
        self.apply_task(&target, action, None)
    }

    /// Applies one change and answers with the list afterwards. Every
    /// change is also pushed, so a client that did not ask still sees it.
    fn apply_task(
        &self,
        target: &TaskTarget,
        action: TaskAction,
        session: Option<&str>,
    ) -> anyhow::Result<TaskList> {
        let TaskTarget { key, feature, .. } = target;
        let changes = !matches!(action, TaskAction::List);
        match action {
            TaskAction::List => {}
            TaskAction::Add(write) => {
                let write = write.checked().map_err(|e| anyhow::anyhow!("{e}"))?;
                let at = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or_default();
                self.store.add_task(key, feature, &write, at, session)?;
            }
            TaskAction::Move { id, state } => self.store.move_task(key, id, state, session)?,
            TaskAction::Retitle { id, title } => self.store.retitle_task(key, id, &title)?,
            TaskAction::SetBody { id, body } => self.store.set_task_body(key, id, body)?,
            TaskAction::Remove { id } => self.store.remove_task(key, id)?,
            TaskAction::Reorder { id, to } => self.store.reorder_task(key, id, to)?,
        }
        if changes {
            self.broadcast_tasks(&target.project_name, key, feature);
        }
        Ok(TaskList {
            project_name: target.project_name.clone(),
            feature: Some(feature.clone()),
            tasks: self.store.tasks(key, feature)?,
        })
    }

    /// Refuses a task that is not under the feature this checkout is on.
    ///
    /// Ids are database-wide and an agent numbers its tasks from what it
    /// last read, so a stale id would otherwise let one feature's agent
    /// tick off another's work by arithmetic.
    fn guard_task(&self, key: &str, feature: &str, id: i64) -> anyhow::Result<()> {
        let mine = self
            .store
            .tasks(key, feature)?
            .iter()
            .any(|t: &Task| t.id == id);
        if !mine {
            anyhow::bail!("task {id} is not under this checkout's feature");
        }
        Ok(())
    }

    /// Pushes a changed list at every attached client, the way a board is.
    ///
    /// A task list is watched while it is worked — that is most of why it
    /// is on screen — so a client holding one open does not have to ask.
    ///
    /// The features go with it. Every feature row carries how its tasks
    /// stand, so a list that changed without the features following it
    /// would leave the row above the list contradicting the list.
    fn broadcast_tasks(&self, name: &str, key: &str, feature: &str) {
        let tasks = self.store.tasks(key, feature).unwrap_or_default();
        let _ = self.tasks_tx.send(TaskList {
            project_name: name.to_string(),
            feature: Some(feature.to_string()),
            tasks,
        });
        self.broadcast_decisions(name, key, self.store.decisions(key).unwrap_or_default());
    }
}
