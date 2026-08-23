use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use orion_protocol::{
    Cell, CheckoutId, CheckoutInfo, IdGen, PaneId, PaneInfo, PaneKind, PaneStatus, ProjectId,
    ProjectInfo, ServerMsg,
};
use tokio::sync::broadcast;

use crate::config::{self, AgentConfig, ConfigFile};
use crate::pty::{self, PaneRuntime};

struct Pane {
    id: PaneId,
    kind: PaneKind,
    title: String,
    status: PaneStatus,
    runtime: PaneRuntime,
}

struct Checkout {
    id: CheckoutId,
    name: String,
    path: PathBuf,
    panes: Vec<Pane>,
}

struct Project {
    id: ProjectId,
    name: String,
    checkouts: Vec<Checkout>,
}

struct Inner {
    projects: Vec<Project>,
    ids: IdGen,
}

pub struct Daemon {
    inner: StdMutex<Inner>,
    tree_tx: broadcast::Sender<Vec<ProjectInfo>>,
    templates: Vec<AgentConfig>,
}

type PaneSubscription = (u16, u16, Vec<Vec<Cell>>, broadcast::Receiver<ServerMsg>);

impl Daemon {
    pub fn new(config: ConfigFile) -> Arc<Self> {
        let mut ids = IdGen::default();
        let projects = config
            .projects
            .into_iter()
            .map(|p| Project {
                id: ProjectId(ids.alloc()),
                name: p.name,
                checkouts: p
                    .repos
                    .into_iter()
                    .map(|repo| {
                        let path = config::expand_home(&repo);
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or(repo);
                        Checkout {
                            id: CheckoutId(ids.alloc()),
                            name,
                            path,
                            panes: Vec::new(),
                        }
                    })
                    .collect(),
            })
            .collect();

        let templates = if config.agents.is_empty() {
            config::default_agents()
        } else {
            config.agents
        };

        let (tree_tx, _) = broadcast::channel(32);
        Arc::new(Daemon {
            inner: StdMutex::new(Inner { projects, ids }),
            tree_tx,
            templates,
        })
    }

    pub fn template_names(&self) -> Vec<String> {
        self.templates.iter().map(|t| t.name.clone()).collect()
    }

    pub fn snapshot(&self) -> Vec<ProjectInfo> {
        let inner = self.inner.lock().unwrap();
        inner
            .projects
            .iter()
            .map(|p| ProjectInfo {
                id: p.id,
                name: p.name.clone(),
                checkouts: p
                    .checkouts
                    .iter()
                    .map(|c| CheckoutInfo {
                        id: c.id,
                        name: c.name.clone(),
                        path: c.path.to_string_lossy().to_string(),
                        panes: c
                            .panes
                            .iter()
                            .map(|pane| PaneInfo {
                                id: pane.id,
                                kind: pane.kind,
                                title: pane.title.clone(),
                                status: pane.status,
                            })
                            .collect(),
                        git: crate::git::status(&c.path),
                    })
                    .collect(),
            })
            .collect()
    }

    pub fn subscribe_tree(&self) -> broadcast::Receiver<Vec<ProjectInfo>> {
        self.tree_tx.subscribe()
    }

    fn broadcast_tree(&self) {
        let _ = self.tree_tx.send(self.snapshot());
    }

    pub fn spawn_shell(self: &Arc<Self>, checkout: CheckoutId) -> anyhow::Result<PaneId> {
        let path = {
            let inner = self.inner.lock().unwrap();
            find_checkout_ref(&inner.projects, checkout)
                .map(|c| c.path.clone())
                .ok_or_else(|| anyhow::anyhow!("no such checkout"))?
        };

        let id = {
            let mut inner = self.inner.lock().unwrap();
            PaneId(inner.ids.alloc())
        };

        let daemon = self.clone();
        let runtime = PaneRuntime::spawn(id, &path, pty::Spawn::DefaultShell, move |code| {
            daemon.mark_pane_exited(id, code);
        })?;

        {
            let mut inner = self.inner.lock().unwrap();
            if let Some(c) = find_checkout(&mut inner.projects, checkout) {
                c.panes.push(Pane {
                    id,
                    kind: PaneKind::Shell,
                    title: "shell".to_string(),
                    status: PaneStatus::Running,
                    runtime,
                });
            }
        }
        self.broadcast_tree();
        Ok(id)
    }

    pub fn spawn_agent(self: &Arc<Self>, checkout: CheckoutId, template_name: &str) -> anyhow::Result<PaneId> {
        let template = self
            .templates
            .iter()
            .find(|t| t.name == template_name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no such agent template: {template_name}"))?;
        let Some((program, rest)) = template.cmd.split_first() else {
            anyhow::bail!("agent template {template_name} has an empty cmd");
        };

        let path = {
            let inner = self.inner.lock().unwrap();
            find_checkout_ref(&inner.projects, checkout)
                .map(|c| c.path.clone())
                .ok_or_else(|| anyhow::anyhow!("no such checkout"))?
        };

        let id = {
            let mut inner = self.inner.lock().unwrap();
            PaneId(inner.ids.alloc())
        };

        let spec = pty::Spawn::Program {
            program: program.clone(),
            args: rest.to_vec(),
            env: template.env.into_iter().collect(),
        };

        let daemon = self.clone();
        let runtime = PaneRuntime::spawn(id, &path, spec, move |code| {
            daemon.mark_pane_exited(id, code);
        })?;

        {
            let mut inner = self.inner.lock().unwrap();
            if let Some(c) = find_checkout(&mut inner.projects, checkout) {
                c.panes.push(Pane {
                    id,
                    kind: PaneKind::Agent,
                    title: template.name.clone(),
                    status: PaneStatus::Running,
                    runtime,
                });
            }
        }
        self.broadcast_tree();
        Ok(id)
    }

    pub fn mark_pane_exited(&self, pane: PaneId, code: Option<i32>) {
        {
            let mut inner = self.inner.lock().unwrap();
            if let Some(p) = find_pane(&mut inner.projects, pane) {
                p.status = PaneStatus::Exited { code };
            }
        }
        self.broadcast_tree();
    }

    /// Kills the pane's process (best-effort — it may already have exited)
    /// and removes it from the tree entirely, so a closed pane actually
    /// disappears instead of lingering as a dead row the user can't clear.
    pub fn close_pane(&self, pane: PaneId) -> anyhow::Result<()> {
        let removed = {
            let mut inner = self.inner.lock().unwrap();
            remove_pane(&mut inner.projects, pane)
        };
        let removed = removed.ok_or_else(|| anyhow::anyhow!("no such pane"))?;
        let _ = removed.runtime.kill();
        self.broadcast_tree();
        Ok(())
    }

    pub fn write_pane(&self, pane: PaneId, bytes: &[u8]) -> anyhow::Result<()> {
        let mut inner = self.inner.lock().unwrap();
        let p = find_pane(&mut inner.projects, pane).ok_or_else(|| anyhow::anyhow!("no such pane"))?;
        p.runtime.write_input(bytes)
    }

    pub fn resize_pane(&self, pane: PaneId, rows: u16, cols: u16) -> anyhow::Result<()> {
        let inner = self.inner.lock().unwrap();
        let p = find_pane_ref(&inner.projects, pane).ok_or_else(|| anyhow::anyhow!("no such pane"))?;
        p.runtime.resize(rows, cols)?;
        // A subscribed client's cached grid is only ever sized by whatever
        // snapshot it last received; incremental Damage can't grow it.
        // Push a fresh full snapshot at the new size so growing a pane
        // (very common — new panes start at a hardcoded default far
        // smaller than most terminal heights) doesn't leave the newly
        // exposed area permanently blank.
        p.runtime.broadcast_snapshot(pane);
        Ok(())
    }

    pub fn subscribe_pane(&self, pane: PaneId) -> anyhow::Result<PaneSubscription> {
        let inner = self.inner.lock().unwrap();
        let p = find_pane_ref(&inner.projects, pane).ok_or_else(|| anyhow::anyhow!("no such pane"))?;
        let (rows, cols, cells) = p.runtime.full_snapshot();
        let rx = p.runtime.subscribe();
        Ok((rows, cols, cells, rx))
    }

    /// Periodically re-broadcasts the tree so checkout rows pick up git
    /// status changes (a commit, a stash, an agent editing a file) without
    /// needing a pane event to trigger it. `git::status` shells out to
    /// libgit2, so each tick runs on a blocking-pool thread rather than the
    /// async runtime's own workers (see DESIGN.md §8, "never on the input
    /// thread"). A real file watcher is a sharper version of this same idea
    /// for later — this poll is the read-only M2 slice.
    pub fn start_git_poll(self: &Arc<Self>) {
        let daemon = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(2));
            loop {
                interval.tick().await;
                let daemon = daemon.clone();
                let _ = tokio::task::spawn_blocking(move || daemon.broadcast_tree()).await;
            }
        });
    }
}

fn find_checkout(projects: &mut [Project], id: CheckoutId) -> Option<&mut Checkout> {
    projects
        .iter_mut()
        .flat_map(|p| p.checkouts.iter_mut())
        .find(|c| c.id == id)
}

fn find_checkout_ref(projects: &[Project], id: CheckoutId) -> Option<&Checkout> {
    projects
        .iter()
        .flat_map(|p| p.checkouts.iter())
        .find(|c| c.id == id)
}

fn find_pane(projects: &mut [Project], id: PaneId) -> Option<&mut Pane> {
    projects
        .iter_mut()
        .flat_map(|p| p.checkouts.iter_mut())
        .flat_map(|c| c.panes.iter_mut())
        .find(|p| p.id == id)
}

fn find_pane_ref(projects: &[Project], id: PaneId) -> Option<&Pane> {
    projects
        .iter()
        .flat_map(|p| p.checkouts.iter())
        .flat_map(|c| c.panes.iter())
        .find(|p| p.id == id)
}

fn remove_pane(projects: &mut [Project], id: PaneId) -> Option<Pane> {
    for project in projects.iter_mut() {
        for checkout in project.checkouts.iter_mut() {
            if let Some(pos) = checkout.panes.iter().position(|p| p.id == id) {
                return Some(checkout.panes.remove(pos));
            }
        }
    }
    None
}
