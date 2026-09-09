//! The tasks under a feature: what is left to do, as opposed to what is
//! being built (the feature) or why it is built that way (the decisions).
//!
//! Both sides write here, and for once they write the same things. A human
//! populates a list by hand or asks an agent to read it out of whatever
//! tracker the team uses; an agent takes a task up, finishes it, and adds
//! what it found on the way. There is no acceptance step and so no move
//! either side is refused — that ceremony belongs to the feature the tasks
//! are under, which is where a human accepts the work as a whole.

use argus_protocol::{ArtifactScope, PaneId, ProjectId, Task, TaskList, TaskState, TaskWrite};

use super::agents::AgentScope;
use super::*;

impl Daemon {
    /// The tasks of the feature this checkout is on.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn tasks_for_agent(&self, pane_id: PaneId) -> anyhow::Result<TaskList> {
        self.tasks_for_agent_in_scope(pane_id, ArtifactScope::default())
    }

    pub fn tasks_for_agent_in_scope(
        &self,
        pane_id: PaneId,
        artifact_scope: ArtifactScope,
    ) -> anyhow::Result<TaskList> {
        let scope = self.agent_scope(pane_id)?;
        self.task_list(&scope, artifact_scope)
    }

    fn task_list(
        &self,
        scope: &AgentScope,
        artifact_scope: ArtifactScope,
    ) -> anyhow::Result<TaskList> {
        let key = scope.artifact_key(artifact_scope);
        let feature = self.feature_for_agent_in_scope(scope, artifact_scope)?;
        let tasks = match &feature {
            Some(slug) => self.store.tasks(key, slug)?,
            None => Vec::new(),
        };
        Ok(TaskList {
            project_name: scope.project_name.clone(),
            feature,
            tasks,
        })
    }

    /// Adds a task to the current feature.
    ///
    /// Refused when the checkout is on no feature, for the same reason a
    /// decision is: a task with nothing to be under is the pile all of
    /// this exists to end.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn add_task_for_agent(
        &self,
        pane_id: PaneId,
        session: Option<&str>,
        write: TaskWrite,
    ) -> anyhow::Result<TaskList> {
        self.add_task_for_agent_in_scope(pane_id, session, write, ArtifactScope::default())
    }

    pub fn add_task_for_agent_in_scope(
        &self,
        pane_id: PaneId,
        session: Option<&str>,
        write: TaskWrite,
        artifact_scope: ArtifactScope,
    ) -> anyhow::Result<TaskList> {
        let scope = self.agent_scope(pane_id)?;
        let key = scope.artifact_key(artifact_scope);
        let write = write.checked().map_err(|e| anyhow::anyhow!("{e}"))?;
        let Some(slug) = self.feature_for_agent_in_scope(&scope, artifact_scope)? else {
            anyhow::bail!(
                "this checkout is not on a feature yet — open one with \
                 `argus-hook feature open` before adding tasks to it"
            );
        };
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();
        self.store.add_task(key, &slug, &write, at, session)?;
        let list = self.task_list(&scope, artifact_scope)?;
        self.broadcast_tasks(&scope.project_name, key, &slug);
        Ok(list)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn move_task_for_agent(
        &self,
        pane_id: PaneId,
        session: Option<&str>,
        id: i64,
        state: TaskState,
    ) -> anyhow::Result<TaskList> {
        self.move_task_for_agent_in_scope(pane_id, session, id, state, ArtifactScope::default())
    }

    pub fn move_task_for_agent_in_scope(
        &self,
        pane_id: PaneId,
        session: Option<&str>,
        id: i64,
        state: TaskState,
        artifact_scope: ArtifactScope,
    ) -> anyhow::Result<TaskList> {
        let scope = self.agent_scope(pane_id)?;
        let key = scope.artifact_key(artifact_scope);
        self.guard_task(&scope, id, artifact_scope)?;
        self.store.move_task(key, id, state, session)?;
        let list = self.task_list(&scope, artifact_scope)?;
        if let Some(feature) = &list.feature {
            self.broadcast_tasks(&scope.project_name, key, feature);
        }
        Ok(list)
    }

    #[allow(dead_code)]
    pub fn retitle_task_for_agent(
        &self,
        pane_id: PaneId,
        id: i64,
        title: &str,
    ) -> anyhow::Result<TaskList> {
        self.retitle_task_for_agent_in_scope(pane_id, id, title, ArtifactScope::default())
    }

    pub fn retitle_task_for_agent_in_scope(
        &self,
        pane_id: PaneId,
        id: i64,
        title: &str,
        artifact_scope: ArtifactScope,
    ) -> anyhow::Result<TaskList> {
        let scope = self.agent_scope(pane_id)?;
        let key = scope.artifact_key(artifact_scope);
        self.guard_task(&scope, id, artifact_scope)?;
        self.store.retitle_task(key, id, title)?;
        let list = self.task_list(&scope, artifact_scope)?;
        if let Some(feature) = &list.feature {
            self.broadcast_tasks(&scope.project_name, key, feature);
        }
        Ok(list)
    }

    pub fn set_task_body_for_agent_in_scope(
        &self,
        pane_id: PaneId,
        id: i64,
        body: String,
        artifact_scope: ArtifactScope,
    ) -> anyhow::Result<TaskList> {
        let scope = self.agent_scope(pane_id)?;
        let key = scope.artifact_key(artifact_scope);
        self.guard_task(&scope, id, artifact_scope)?;
        self.store.set_task_body(key, id, body)?;
        let list = self.task_list(&scope, artifact_scope)?;
        if let Some(feature) = &list.feature {
            self.broadcast_tasks(&scope.project_name, key, feature);
        }
        Ok(list)
    }

    #[allow(dead_code)]
    pub fn remove_task_for_agent(&self, pane_id: PaneId, id: i64) -> anyhow::Result<TaskList> {
        self.remove_task_for_agent_in_scope(pane_id, id, ArtifactScope::default())
    }

    pub fn remove_task_for_agent_in_scope(
        &self,
        pane_id: PaneId,
        id: i64,
        artifact_scope: ArtifactScope,
    ) -> anyhow::Result<TaskList> {
        let scope = self.agent_scope(pane_id)?;
        let key = scope.artifact_key(artifact_scope);
        self.guard_task(&scope, id, artifact_scope)?;
        self.store.remove_task(key, id)?;
        let list = self.task_list(&scope, artifact_scope)?;
        if let Some(feature) = &list.feature {
            self.broadcast_tasks(&scope.project_name, key, feature);
        }
        Ok(list)
    }

    /// Refuses a task that is not under the feature this checkout is on.
    ///
    /// Ids are database-wide and an agent numbers its tasks from what it
    /// last read, so a stale id would otherwise let one feature's agent
    /// tick off another's work by arithmetic.
    fn guard_task(
        &self,
        scope: &AgentScope,
        id: i64,
        artifact_scope: ArtifactScope,
    ) -> anyhow::Result<()> {
        let key = scope.artifact_key(artifact_scope);
        let Some(slug) = self.feature_for_agent_in_scope(scope, artifact_scope)? else {
            anyhow::bail!("this checkout is not on a feature yet");
        };
        let mine = self
            .store
            .tasks(key, &slug)?
            .into_iter()
            .any(|t: Task| t.id == id);
        if !mine {
            anyhow::bail!("task {id} is not under this checkout's feature");
        }
        Ok(())
    }

    // ---- the client's side ---------------------------------------------

    pub fn task_list_for_client(
        &self,
        project: ProjectId,
        checkout: CheckoutId,
        feature: &str,
    ) -> anyhow::Result<TaskList> {
        let (name, key) = self.client_artifact_scope(project, checkout)?;
        Ok(TaskList {
            tasks: self.store.tasks(&key, feature)?,
            project_name: name,
            feature: Some(feature.to_string()),
        })
    }

    pub fn add_task_for_client(
        &self,
        project: ProjectId,
        checkout: CheckoutId,
        feature: &str,
        write: TaskWrite,
    ) -> anyhow::Result<()> {
        let (name, key) = self.client_artifact_scope(project, checkout)?;
        let write = write.checked().map_err(|e| anyhow::anyhow!("{e}"))?;
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();
        self.store.add_task(&key, feature, &write, at, None)?;
        self.broadcast_tasks(&name, &key, feature);
        Ok(())
    }

    pub fn move_task_for_client(
        &self,
        project: ProjectId,
        checkout: CheckoutId,
        feature: &str,
        id: i64,
        state: TaskState,
    ) -> anyhow::Result<()> {
        let (name, key) = self.client_artifact_scope(project, checkout)?;
        self.store.move_task(&key, id, state, None)?;
        self.broadcast_tasks(&name, &key, feature);
        Ok(())
    }

    pub fn retitle_task_for_client(
        &self,
        project: ProjectId,
        checkout: CheckoutId,
        feature: &str,
        id: i64,
        title: &str,
    ) -> anyhow::Result<()> {
        let (name, key) = self.client_artifact_scope(project, checkout)?;
        self.store.retitle_task(&key, id, title)?;
        self.broadcast_tasks(&name, &key, feature);
        Ok(())
    }

    pub fn set_task_body_for_client(
        &self,
        project: ProjectId,
        checkout: CheckoutId,
        feature: &str,
        id: i64,
        body: String,
    ) -> anyhow::Result<()> {
        let (name, key) = self.client_artifact_scope(project, checkout)?;
        self.store.set_task_body(&key, id, body)?;
        self.broadcast_tasks(&name, &key, feature);
        Ok(())
    }

    pub fn remove_task_for_client(
        &self,
        project: ProjectId,
        checkout: CheckoutId,
        feature: &str,
        id: i64,
    ) -> anyhow::Result<()> {
        let (name, key) = self.client_artifact_scope(project, checkout)?;
        self.store.remove_task(&key, id)?;
        self.broadcast_tasks(&name, &key, feature);
        Ok(())
    }

    pub fn reorder_task_for_client(
        &self,
        project: ProjectId,
        checkout: CheckoutId,
        feature: &str,
        id: i64,
        to: i64,
    ) -> anyhow::Result<()> {
        let (name, key) = self.client_artifact_scope(project, checkout)?;
        self.store.reorder_task(&key, id, to)?;
        self.broadcast_tasks(&name, &key, feature);
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
