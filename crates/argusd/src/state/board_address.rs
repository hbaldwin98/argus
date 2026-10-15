//! Which board a request addresses, and who hears of it.
//!
//! An agent names its board by the pane it asks through and a client by
//! the project and checkout it is looking at. Either is resolved here, once
//! per request, into the board's store key, the project it is the board
//! of, and the repositories a push of it names; a rule applied to the board
//! afterwards reads all three from the address instead of looking them up
//! again, so a change and its push cannot disagree about which board it was.

use std::path::PathBuf;

use argus_protocol::{ArtifactScope, PaneId, ProjectId, RepositoryId};

use super::*;

/// One board, resolved. The feature on it is not part of the address: an
/// agent's comes from the checkout pointer this carries, and a client names
/// its own.
#[derive(Debug, Clone)]
pub(crate) struct BoardAddress {
    pub scope: ArtifactScope,
    pub project: ProjectId,
    pub project_name: String,
    /// Opaque durable board identity. The separator cannot occur in a Git
    /// path, so repositories cannot alias.
    pub key: String,
    /// The repositories in the tree whose board this is: what a pushed board
    /// names so each client keeps only its own. None for a workspace board,
    /// which is never pushed.
    pub repositories: Vec<RepositoryId>,
    /// The checkout the request comes from, which is what a feature pointer
    /// is kept against. A client's is the one it has selected.
    pub checkout_path: Option<PathBuf>,
}

impl BoardAddress {
    /// Whether a client can be showing this board, and so whether a change
    /// to it is pushed.
    ///
    /// A client holds only repository boards, and a push names the
    /// repositories it is for rather than the board it is: pushed under its
    /// repositories, a workspace board would replace each one's own board
    /// on screen.
    pub fn is_pushed(&self) -> bool {
        self.scope == ArtifactScope::RepositoryBranch
    }

    /// The checkout a pointer change or an origin is recorded against.
    pub fn checkout(&self) -> anyhow::Result<&std::path::Path> {
        self.checkout_path
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("this board was not addressed from a checkout"))
    }
}

/// Where a live agent pane runs, as found under the tree's lock.
struct AgentPlace {
    project: ProjectId,
    project_name: String,
    workspace_name: String,
    checkout_path: PathBuf,
}

fn path_artifact_key(path: &std::path::Path) -> String {
    let git_dir = crate::git::repository_common_dir(path).unwrap_or_else(|| path.to_path_buf());
    format!("repository\0{}", git_dir.to_string_lossy())
}

impl Daemon {
    /// The board a live agent pane addresses: its repository's, or with
    /// `scope` its workspace's.
    ///
    /// Every durable read an agent is allowed to make resolves through
    /// here, so the rule that a caller must be a live agent pane is written
    /// once. A shell pane, an exited one, or an id from another Argus gets
    /// the same refusal whatever it asked for.
    pub(super) fn address_for_agent(
        &self,
        pane: PaneId,
        scope: ArtifactScope,
    ) -> anyhow::Result<BoardAddress> {
        let place = self.agent_place(pane)?;
        let (key, repositories) = match scope {
            ArtifactScope::RepositoryBranch => {
                let key = path_artifact_key(&place.checkout_path);
                let repositories = self.repositories_keyed(&key);
                (key, repositories)
            }
            ArtifactScope::Workspace => (format!("workspace\0{}", place.workspace_name), Vec::new()),
        };
        Ok(BoardAddress {
            scope,
            project: place.project,
            project_name: place.project_name,
            key,
            repositories,
            checkout_path: Some(place.checkout_path),
        })
    }

    /// The checkout a live agent pane works in, named the way the store
    /// files it rather than by an id a restart throws away.
    pub(super) fn agent_checkout(&self, pane: PaneId) -> anyhow::Result<PathBuf> {
        Ok(self.agent_place(pane)?.checkout_path)
    }

    fn agent_place(&self, pane_id: PaneId) -> anyhow::Result<AgentPlace> {
        let inner = self.inner.lock().unwrap();
        for project in &inner.projects {
            for repository in &project.repositories {
                for checkout in &repository.checkouts {
                    let Some(pane) = checkout.panes.iter().find(|pane| pane.id == pane_id) else {
                        continue;
                    };
                    if !pane.is_running_agent() {
                        anyhow::bail!("source must be a live agent pane");
                    }
                    let workspace_name = inner
                        .workspaces
                        .iter()
                        .find(|workspace| workspace.id == project.workspace)
                        .map(|workspace| workspace.name.clone())
                        .ok_or_else(|| anyhow::anyhow!("project has no workspace"))?;
                    return Ok(AgentPlace {
                        project: project.id,
                        project_name: project.name.clone(),
                        workspace_name,
                        checkout_path: checkout.path.clone(),
                    });
                }
            }
        }
        anyhow::bail!("no such source pane")
    }

    /// The board of the repository a client's checkout is in. A client
    /// cannot address a workspace board.
    pub(super) fn address_for_client(
        &self,
        project_id: ProjectId,
        checkout_id: CheckoutId,
    ) -> anyhow::Result<BoardAddress> {
        let (project_name, checkout_path) = {
            let inner = self.inner.lock().unwrap();
            let project = inner
                .projects
                .iter()
                .find(|project| project.id == project_id)
                .ok_or_else(|| anyhow::anyhow!("no such project"))?;
            let checkout = project
                .repositories
                .iter()
                .flat_map(|repository| &repository.checkouts)
                .find(|checkout| checkout.id == checkout_id)
                .ok_or_else(|| anyhow::anyhow!("checkout is not in this project"))?;
            (project.name.clone(), checkout.path.clone())
        };
        let key = path_artifact_key(&checkout_path);
        Ok(BoardAddress {
            scope: ArtifactScope::RepositoryBranch,
            project: project_id,
            project_name,
            repositories: self.repositories_keyed(&key),
            key,
            checkout_path: Some(checkout_path),
        })
    }

    /// The repositories in the tree whose board is the one filed under
    /// `key`. More than one when a repository is listed twice.
    fn repositories_keyed(&self, key: &str) -> Vec<RepositoryId> {
        let paths: Vec<(RepositoryId, PathBuf)> = {
            let inner = self.inner.lock().unwrap();
            inner
                .projects
                .iter()
                .flat_map(|project| &project.repositories)
                .filter_map(|repository| {
                    let checkout = repository
                        .checkouts
                        .iter()
                        .find(|checkout| checkout.primary)
                        .or_else(|| repository.checkouts.first())?;
                    Some((repository.id, checkout.path.clone()))
                })
                .collect()
        };
        paths
            .into_iter()
            .filter(|(_, path)| self.board_key(path) == key)
            .map(|(id, _)| id)
            .collect()
    }

    fn board_key(&self, path: &std::path::Path) -> String {
        if let Some(key) = self.board_keys.lock().unwrap().get(path) {
            return key.clone();
        }
        let key = path_artifact_key(path);
        self.board_keys.lock().unwrap().insert(path.to_path_buf(), key.clone());
        key
    }
}
