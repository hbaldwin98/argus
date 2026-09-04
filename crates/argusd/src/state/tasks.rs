//! The tasks under a feature: what is left to do, as opposed to what is
//! being built (the feature) or why it is built that way (the decisions).
//!
//! Both sides write here, and for once they write the same things. A human
//! populates a list by hand or asks an agent to read it out of whatever
//! tracker the team uses; an agent takes a task up, finishes it, and adds
//! what it found on the way. There is no acceptance step and so no move
//! either side is refused — that ceremony belongs to the feature the tasks
//! are under, which is where a human accepts the work as a whole.

use argus_protocol::{PaneId, ProjectId, Task, TaskList, TaskState, TaskWrite};

use super::agents::AgentScope;
use super::*;

impl Daemon {
    /// The tasks of the feature this checkout is on.
    pub fn tasks_for_agent(&self, pane_id: PaneId) -> anyhow::Result<TaskList> {
        let scope = self.agent_scope(pane_id)?;
        self.task_list(&scope)
    }

    fn task_list(&self, scope: &AgentScope) -> anyhow::Result<TaskList> {
        let feature = self.feature_for_agent(scope)?;
        let tasks = match &feature {
            Some(slug) => self.store.tasks(&scope.project_name, slug)?,
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
    pub fn add_task_for_agent(
        &self,
        pane_id: PaneId,
        session: Option<&str>,
        write: TaskWrite,
    ) -> anyhow::Result<TaskList> {
        let scope = self.agent_scope(pane_id)?;
        let write = write.checked().map_err(|e| anyhow::anyhow!("{e}"))?;
        let Some(slug) = self.feature_for_agent(&scope)? else {
            anyhow::bail!(
                "this checkout is not on a feature yet — open one with \
                 `argus-hook feature open` before adding tasks to it"
            );
        };
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();
        self.store
            .add_task(&scope.project_name, &slug, &write, at, session)?;
        let list = self.task_list(&scope)?;
        self.broadcast_tasks(&scope.project_name, &slug);
        Ok(list)
    }

    pub fn move_task_for_agent(
        &self,
        pane_id: PaneId,
        session: Option<&str>,
        id: i64,
        state: TaskState,
    ) -> anyhow::Result<TaskList> {
        let scope = self.agent_scope(pane_id)?;
        self.guard_task(&scope, id)?;
        self.store
            .move_task(&scope.project_name, id, state, session)?;
        let list = self.task_list(&scope)?;
        if let Some(slug) = &list.feature {
            self.broadcast_tasks(&scope.project_name, slug);
        }
        Ok(list)
    }

    pub fn retitle_task_for_agent(
        &self,
        pane_id: PaneId,
        id: i64,
        title: &str,
    ) -> anyhow::Result<TaskList> {
        let scope = self.agent_scope(pane_id)?;
        self.guard_task(&scope, id)?;
        self.store.retitle_task(&scope.project_name, id, title)?;
        let list = self.task_list(&scope)?;
        if let Some(slug) = &list.feature {
            self.broadcast_tasks(&scope.project_name, slug);
        }
        Ok(list)
    }

    pub fn remove_task_for_agent(&self, pane_id: PaneId, id: i64) -> anyhow::Result<TaskList> {
        let scope = self.agent_scope(pane_id)?;
        self.guard_task(&scope, id)?;
        let feature = self.feature_for_agent(&scope)?;
        self.store.remove_task(&scope.project_name, id)?;
        let list = self.task_list(&scope)?;
        if let Some(slug) = &feature {
            self.broadcast_tasks(&scope.project_name, slug);
        }
        Ok(list)
    }

    /// Refuses a task that is not under the feature this checkout is on.
    ///
    /// Ids are project-wide and an agent numbers its tasks from what it
    /// last read, so a stale id would otherwise let one feature's agent
    /// tick off another's work by arithmetic.
    fn guard_task(&self, scope: &AgentScope, id: i64) -> anyhow::Result<()> {
        let Some(slug) = self.feature_for_agent(scope)? else {
            anyhow::bail!("this checkout is not on a feature yet");
        };
        let mine = self
            .store
            .tasks(&scope.project_name, &slug)?
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
        feature: &str,
    ) -> anyhow::Result<TaskList> {
        let name = self.project_name_of(project)?;
        Ok(TaskList {
            tasks: self.store.tasks(&name, feature)?,
            project_name: name,
            feature: Some(feature.to_string()),
        })
    }

    pub fn add_task_for_client(
        &self,
        project: ProjectId,
        feature: &str,
        write: TaskWrite,
    ) -> anyhow::Result<()> {
        let name = self.project_name_of(project)?;
        let write = write.checked().map_err(|e| anyhow::anyhow!("{e}"))?;
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();
        self.store.add_task(&name, feature, &write, at, None)?;
        self.broadcast_tasks(&name, feature);
        Ok(())
    }

    pub fn move_task_for_client(
        &self,
        project: ProjectId,
        feature: &str,
        id: i64,
        state: TaskState,
    ) -> anyhow::Result<()> {
        let name = self.project_name_of(project)?;
        self.store.move_task(&name, id, state, None)?;
        self.broadcast_tasks(&name, feature);
        Ok(())
    }

    pub fn retitle_task_for_client(
        &self,
        project: ProjectId,
        feature: &str,
        id: i64,
        title: &str,
    ) -> anyhow::Result<()> {
        let name = self.project_name_of(project)?;
        self.store.retitle_task(&name, id, title)?;
        self.broadcast_tasks(&name, feature);
        Ok(())
    }

    pub fn remove_task_for_client(
        &self,
        project: ProjectId,
        feature: &str,
        id: i64,
    ) -> anyhow::Result<()> {
        let name = self.project_name_of(project)?;
        self.store.remove_task(&name, id)?;
        self.broadcast_tasks(&name, feature);
        Ok(())
    }

    pub fn reorder_task_for_client(
        &self,
        project: ProjectId,
        feature: &str,
        id: i64,
        to: i64,
    ) -> anyhow::Result<()> {
        let name = self.project_name_of(project)?;
        self.store.reorder_task(&name, id, to)?;
        self.broadcast_tasks(&name, feature);
        Ok(())
    }

    fn project_name_of(&self, project: ProjectId) -> anyhow::Result<String> {
        let inner = self.inner.lock().unwrap();
        inner
            .projects
            .iter()
            .find(|p| p.id == project)
            .map(|p| p.name.clone())
            .ok_or_else(|| anyhow::anyhow!("no such project"))
    }

    /// Pushes a changed list at every attached client, the way a board is.
    ///
    /// A task list is watched while it is worked — that is most of why it
    /// is on screen — so a client holding one open does not have to ask.
    fn broadcast_tasks(&self, name: &str, feature: &str) {
        let tasks = self.store.tasks(name, feature).unwrap_or_default();
        let _ = self.tasks_tx.send(TaskList {
            project_name: name.to_string(),
            feature: Some(feature.to_string()),
            tasks,
        });
    }
}
