//! Which workspace is open, daemon-wide.
//!
//! Not a fourth navigation column but a scope above the spine: switching
//! it re-scopes every attached client at once. Panes in the workspaces
//! that are not open keep running, which is why each row carries a rollup
//! — an agent working somewhere you are not looking stays visible.

use super::*;

impl Daemon {
    /// Every workspace with its rollup, open flag included. Ordered as
    /// configured so the picker doesn't reshuffle under the user.
    pub fn workspaces(&self) -> Vec<WorkspaceInfo> {
        let inner = self.inner.lock().unwrap();
        inner
            .workspaces
            .iter()
            .map(|w| {
                let projects: Vec<&Project> = inner
                    .projects
                    .iter()
                    .filter(|p| p.workspace == w.id)
                    .collect();
                WorkspaceInfo {
                    id: w.id,
                    name: w.name.clone(),
                    projects: projects.len(),
                    panes: projects
                        .iter()
                        .flat_map(|p| p.repositories.iter())
                        .flat_map(|r| r.checkouts.iter())
                        .map(|c| c.panes.len())
                        .sum(),
                    open: w.id == inner.open,
                }
            })
            .collect()
    }

    /// Switches which workspace is open, for every connected client at
    /// once. A no-op if it is already open, so a stray keypress doesn't
    /// churn the tree. Remembered on disk for the next daemon.
    pub fn open_workspace(&self, workspace: WorkspaceId) -> anyhow::Result<()> {
        let name = {
            let mut inner = self.inner.lock().unwrap();
            if inner.open == workspace {
                return Ok(());
            }
            let name = inner
                .workspaces
                .iter()
                .find(|w| w.id == workspace)
                .map(|w| w.name.clone())
                .ok_or_else(|| anyhow::anyhow!("no such workspace"))?;
            inner.open = workspace;
            name
        };
        self.save_open_workspace(&name);
        self.broadcast_tree();
        self.broadcast_workspaces();
        Ok(())
    }

    /// Declares a new workspace and opens it. Empty by definition: what
    /// puts projects in it is adding them while it is open, which is how
    /// `add_project` already behaves. Persisted with a `[[workspace]]`
    /// block, because an empty workspace has no project to imply it.
    ///
    /// Reopening rather than rejecting a name that already exists would
    /// make one gesture mean two things; the picker already offers the
    /// existing rows, so the create row only ever means a new name.
    pub fn create_workspace(&self, name: &str) -> anyhow::Result<()> {
        let name = name.trim();
        if name.is_empty() {
            anyhow::bail!("a workspace needs a name");
        }
        {
            let inner = self.inner.lock().unwrap();
            if inner.workspaces.iter().any(|w| w.name == name) {
                anyhow::bail!("workspace already exists: {name}");
            }
        }
        // Written before it exists in memory: a workspace the daemon opens
        // but forgets on restart is worse than one that was never made.
        self.store.add_workspace(name)?;

        {
            let mut inner = self.inner.lock().unwrap();
            let id = WorkspaceId(inner.ids.alloc());
            inner.workspaces.push(Workspace {
                id,
                name: name.to_string(),
            });
            inner.open = id;
        }
        self.save_open_workspace(name);
        self.broadcast_tree();
        self.broadcast_workspaces();
        Ok(())
    }

    /// Best-effort: failing to remember the open workspace is not worth
    /// failing the switch the user just asked for.
    pub(super) fn save_open_workspace(&self, name: &str) {
        if let Err(e) = self.store.set_open_workspace(name) {
            tracing::warn!("could not remember the open workspace: {e}");
        }
    }

    /// The open workspace's id and name — what `add_project` files new
    /// projects under, and what the client shows above the project list.
    pub(super) fn open_workspace_ref(&self) -> (WorkspaceId, String) {
        let inner = self.inner.lock().unwrap();
        let name = inner
            .workspaces
            .iter()
            .find(|w| w.id == inner.open)
            .map(|w| w.name.clone())
            .unwrap_or_else(|| config::DEFAULT_WORKSPACE.to_string());
        (inner.open, name)
    }
}
