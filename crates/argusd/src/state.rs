//! The daemon's tree, and everything the rest of the daemon does to it.
//!
//! One `Daemon` behind a small set of mutexes, in one file per concern:
//! `panes` for a pane's lifecycle, `agents` for what an agent reports
//! about itself, `viewers` for the size each client asks a pty for,
//! `git_ops` for the writes to Git, `sync` for the polls and watchers
//! that keep the tree level with the disk, `panel` for the rows the user
//! adds and removes, `notes` for what is written down against a row,
//! `hook_server` for the loopback receiver, `session` for what survives a
//! restart, and `tree` for finding your way around.
//!
//! The type and its locking are one thing; only which file a concern is
//! read in is split.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use argus_protocol::{
    Cell, CheckoutId, CheckoutInfo, GitStatus, IdGen, PaneId, PaneInfo, PaneKind, PaneStatus,
    ProjectId, ProjectInfo, RepositoryId, RepositoryInfo, ServerMsg, WorkspaceId, WorkspaceInfo,
};
use tokio::sync::broadcast;

mod agents;
mod git_ops;
mod hook_server;
mod notes;
mod panel;
mod panes;
mod session;
mod sync;
mod tree;
mod viewers;

pub use git_ops::BranchDeletion;
pub use viewers::ViewerId;
use viewers::Viewers;
use hook_server::{gen_token, valid_session_id};
use session::{agent_args, nothing_to_resume, Resumed};
use tree::*;

use crate::config::{self, AgentConfig, ConfigFile};
use crate::paths::same_path;
use crate::store::NoteKey;
use crate::pty::{self, PaneRuntime};

struct Pane {
    id: PaneId,
    kind: PaneKind,
    title: String,
    status: PaneStatus,
    /// The agent's own line about why it is where it is. Set alongside a
    /// status report and cleared by the next one that carries none, so it
    /// can never outlive the state it explains.
    note: Option<String>,
    /// The agent template this pane was started from, kept because the
    /// title no longer answers that — an agent may have renamed it.
    template: Option<String>,
    /// The harness this pane actually started under, which is not the same
    /// question as which harness its template names *now*: the config can
    /// be reloaded, and a running agent's hooks on disk were written by the
    /// harness it started with.
    harness: Option<String>,
    /// Stable conversation identity reported by the harness. Also the pane's
    /// owner: reports carrying a different session belong to `children`.
    harness_session_id: Option<String>,
    /// Agents reporting through this pane that do not own it — a CLI started
    /// from inside the pane's own agent inherits the hook environment, so
    /// without this its every turn would rewrite its parent's row.
    children: Vec<ChildAgent>,
    /// A hook won the race with session restoration, so saved metadata must
    /// not overwrite what the newly started process already reported.
    restore_status_reported: bool,
    restore_title_reported: bool,
    /// Set while this pane is a conversation Argus asked a CLI to reopen,
    /// and it is too early to be sure it could. See [`Resumed`].
    resumed: Option<Resumed>,
    runtime: PaneRuntime,
}

/// One agent running underneath a pane's own, tracked only so the operator
/// can see it. Nothing here ever reaches the parent's status or title: the
/// pane belongs to the agent Argus started in it.
struct ChildAgent {
    session_id: String,
    label: Option<String>,
    status: PaneStatus,
    note: Option<String>,
    /// When this child last said anything. A child that finishes usually
    /// says so, and one whose parent finishes is cleared with the turn —
    /// this is for the child that dies without either happening.
    at: std::time::Instant,
}

/// Hook state reported after a process starts but before its pane enters the
/// tree. Each kind keeps only its latest value, bounding this race mailbox
/// regardless of how many hooks the starting process fires.
#[derive(Default)]
struct PendingStart {
    harness_session_id: Option<String>,
    status: Option<(PaneStatus, Option<String>)>,
    title: Option<String>,
    children: Vec<ChildAgent>,
}

/// How many children a pane lists. A parent fanning out dozens of one-shot
/// CLIs should not push its own row off the column.
const MAX_CHILDREN: usize = 8;

/// How long a child may go without reporting before it stops being listed.
/// Long enough that a single slow tool call is not mistaken for death —
/// the common endings are handled without waiting for it, so this is only
/// the backstop for a child that vanishes silently.
const CHILD_SILENCE: Duration = Duration::from_secs(600);

/// Whether a spawn opens a new conversation or continues the one the pane
/// had before the daemon stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Start {
    Fresh,
    Resuming,
}

struct Checkout {
    id: CheckoutId,
    name: String,
    path: PathBuf,
    /// True for a repo's configured working directory, false for a linked
    /// worktree created via `create_worktree`. Gates removal (§4 Level 2).
    primary: bool,
    panes: Vec<Pane>,
    /// Last git status, or `None` if this checkout is not a repository.
    /// HEAD is filled at daemon construction so the first client sees branch
    /// names; dirty counts arrive on the first poll. Cached rather than read
    /// on demand because `snapshot` is taken under the daemon's one lock,
    /// and `git::status` is milliseconds of blocking I/O per checkout —
    /// long enough to be felt as typing lag, since every keystroke needs
    /// that same lock to find its pane (§4 Level 2).
    git: Option<GitStatus>,
}

struct Repository {
    id: RepositoryId,
    name: String,
    /// True if a scan of the project's root turned this up, false if the
    /// config named it outright. Only the former can be taken away again by
    /// a later scan: a repository the user wrote down stays whether or not
    /// it is currently a Git repository, or currently exists.
    discovered: bool,
    checkouts: Vec<Checkout>,
    /// Local branches none of `checkouts` is sitting on, from the last
    /// poll. Cached for the same reason a checkout's git status is: a
    /// snapshot is taken under the lock keystrokes need, and reading refs
    /// there would put git I/O in front of the next key.
    branches: Vec<String>,
    /// What `origin/HEAD` points at, cached alongside them and for the same
    /// reason. `None` until the first refresh, and for a repository with no
    /// remote and no conventionally-named branch.
    default_branch: Option<String>,
    /// Remote-tracking branches with no local branch of the same name, from
    /// the same refresh. Only a fetch changes these, but reading them costs
    /// what reading the local ones costs, so they ride along.
    remote_branches: Vec<String>,
}

struct Project {
    id: ProjectId,
    name: String,
    /// Which workspace this project is filed under. The tree a client sees
    /// is scoped to whichever workspace is open (DESIGN.md §11).
    workspace: WorkspaceId,
    /// The directory whose repositories make up this project, if it has
    /// one. Kept so reconciliation can look again and find what has been
    /// cloned into it, or removed from it, since.
    root: Option<PathBuf>,
    repositories: Vec<Repository>,
    /// Where this project's worktrees are created, if it says. See
    /// `worktree_dir`.
    worktree_root: Option<PathBuf>,
    /// Commands run in a worktree this project has just created, in order.
    setup: Vec<String>,
    /// Whether a checkout here may hold only one agent at a time.
    exclusive: bool,
    /// What this project's root scan may and may not walk into.
    scan: crate::git::Scan,
}

struct Workspace {
    id: WorkspaceId,
    name: String,
}

struct Inner {
    workspaces: Vec<Workspace>,
    projects: Vec<Project>,
    ids: IdGen,
    /// Exactly one workspace is open at a time, daemon-global. Panes in the
    /// others keep running; they are just not in the tree clients render.
    open: WorkspaceId,
    /// Repository paths the user has removed from the panel. A scan finds
    /// them again every ten seconds, so without this the row would come
    /// straight back (see [`crate::store::Store::excluded_repos`]).
    excluded: Vec<PathBuf>,
}

pub struct Daemon {
    inner: StdMutex<Inner>,
    /// Hooks can fire after the child is spawned but before its pane is
    /// inserted into `inner`. Keep their latest bounded state until insertion.
    starting_agents: StdMutex<HashMap<PaneId, PendingStart>>,
    tree_tx: broadcast::Sender<Vec<ProjectInfo>>,
    workspaces_tx: broadcast::Sender<Vec<WorkspaceInfo>>,
    /// Agent templates, replaceable: `reload_config` swaps them, and every
    /// start looks its template up by name at the time it runs.
    templates: StdMutex<Vec<AgentConfig>>,
    /// Every harness this run knows about, built-in or configured.
    harnesses: Vec<crate::harness::Harness>,
    /// Set once `start_hook_server` binds; 0 until then. Read by
    /// `spawn_agent` when installing hooks — a spawn racing the bind (only
    /// possible in the instant between daemon startup and the bind
    /// completing, since both run synchronously before the socket accepts
    /// any client) just skips hook installation rather than failing.
    hook_port: std::sync::atomic::AtomicU16,
    /// Per-boot bearer token the hook receiver checks. Not cryptographic —
    /// the server only ever binds to loopback — just enough that a stray
    /// local process can't spoof pane status.
    hook_token: String,
    /// True while `restore_session` is spawning, so the panes it makes
    /// don't each rewrite the file it is reading from.
    restoring: std::sync::atomic::AtomicBool,
    /// Runtime state that outlives this run. Which store a daemon holds is
    /// what decides whether it persists at all: `new` gives it one that
    /// lives and dies with the process, so a daemon built in a test cannot
    /// write over the real user's state, and only `with_store` reaches disk.
    store: crate::store::Store,
    /// How many times a template has been restarted in a checkout lately,
    /// and when that run of restarts began. Keeps a CLI that dies on
    /// every start from being restarted forever.
    restart_attempts: StdMutex<HashMap<(CheckoutId, String), (u32, std::time::Instant)>>,
    /// Every attached client's requested size for the panes it is showing.
    /// A pty has one size and clients do not have to agree on it, so the
    /// sizes are collected here and reconciled rather than applied as they
    /// arrive.
    viewers: StdMutex<Viewers>,
    next_viewer: std::sync::atomic::AtomicU64,
}

type PaneSubscription = (
    u16,
    u16,
    Vec<Vec<Cell>>,
    argus_protocol::Cursor,
    argus_protocol::MouseTracking,
    bool,
    broadcast::Receiver<ServerMsg>,
);

impl Daemon {
    /// A daemon that remembers nothing past this process, for tests.
    ///
    /// Persistence is a store you hand a daemon, not a flag it can be
    /// talked into setting, and this is what keeps the several hundred
    /// tests that build one off the real user's disk without any of them
    /// having to remember to ask.
    #[cfg(test)]
    pub fn new(config: ConfigFile) -> Arc<Self> {
        let store = crate::store::Store::in_memory()
            .expect("an in-memory runtime store needs nothing that can fail");
        Self::with_store(config, store)
    }

    pub fn with_store(config: ConfigFile, store: crate::store::Store) -> Arc<Self> {
        let mut ids = IdGen::default();

        // What the user declared, plus what they did to the panel while it
        // was running. A store that cannot be read costs the overlays, not
        // the config: showing the declared projects beats showing nothing.
        let overlays = store.overlays().unwrap_or_else(|e| {
            tracing::warn!("could not read runtime state: {e}");
            Default::default()
        });
        let config = config::with_overlays(config, &overlays);

        // Workspaces come from three places, in this order: the built-in
        // default (always present, so a config that predates workspaces
        // keeps working), any `[[workspace]]` blocks, and any name a
        // project refers to without declaring. Declaring is therefore
        // optional — `workspace = "x"` on a project is enough to create it.
        let mut workspaces: Vec<Workspace> = Vec::new();
        let intern = intern_workspace;
        let default_ws = intern(&mut workspaces, &mut ids, config::DEFAULT_WORKSPACE);
        for w in &config.workspaces {
            intern(&mut workspaces, &mut ids, &w.name);
        }

        let excluded = overlays.excluded.clone();
        let projects = config
            .projects
            .into_iter()
            .map(|p| {
                let root = p.root.as_deref().map(config::expand_home);
                // An excluded path is out whether the scan turned it up or
                // the config named it outright: "remove this row" means the
                // same thing either way.
                let mut repositories: Vec<Repository> = p
                    .repos
                    .iter()
                    .map(|repo| config::expand_home(repo))
                    .filter(|path| !is_excluded(&excluded, path))
                    .map(|path| new_repository(&mut ids, path, false))
                    .collect();
                // A root is scanned once here so the tree is complete the
                // moment the first client attaches, rather than filling in
                // a tick later. Reconciliation keeps it current after that.
                let scan = crate::git::Scan {
                    exclude: p.exclude.clone(),
                    include: p.include.clone(),
                };
                if let Some(root) = &root {
                    let found = retain_included(
                        &excluded,
                        crate::git::discover_repositories_within(root, &scan),
                    );
                    install_discovered(&mut ids, &mut repositories, &found);
                }
                Project {
                    id: ProjectId(ids.alloc()),
                    workspace: match p.workspace.as_deref() {
                        Some(name) => intern(&mut workspaces, &mut ids, name),
                        None => default_ws,
                    },
                    name: p.name,
                    root,
                    repositories,
                    worktree_root: p.worktree_root.as_deref().map(config::expand_home),
                    setup: p.setup,
                    exclusive: p.exclusive,
                    scan: crate::git::Scan {
                        exclude: p.exclude,
                        include: p.include,
                    },
                }
            })
            .collect();

        let templates = if config.agents.is_empty() {
            config::default_agents()
        } else {
            config.agents
        };
        let harnesses = config::harnesses(config.harnesses);

        // Reopen whatever was open last time, if that workspace still
        // exists — a name can disappear from the config between runs.
        let open = overlays
            .open_workspace
            .as_deref()
            .and_then(|name| workspaces.iter().find(|w| w.name == name).map(|w| w.id))
            .unwrap_or(default_ws);

        let (tree_tx, _) = broadcast::channel(32);
        let (workspaces_tx, _) = broadcast::channel(32);
        let daemon = Arc::new(Daemon {
            inner: StdMutex::new(Inner {
                workspaces,
                projects,
                ids,
                open,
                excluded,
            }),
            starting_agents: StdMutex::new(HashMap::new()),
            workspaces_tx,
            tree_tx,
            templates: StdMutex::new(templates),
            harnesses,
            hook_port: std::sync::atomic::AtomicU16::new(0),
            hook_token: gen_token(),
            restoring: std::sync::atomic::AtomicBool::new(false),
            store,
            restart_attempts: StdMutex::new(HashMap::new()),
            viewers: StdMutex::new(Viewers::default()),
            next_viewer: std::sync::atomic::AtomicU64::new(0),
        });
        // Checkout rows are named after the branch occupying them, and that
        // name now comes from the cache. Reading HEAD is enough for a name;
        // a full `git::status` walks every workdir, which on a cold disk
        // across a project root of many repositories is seconds in front of
        // the first client. The first poll tick fills dirty counts.
        daemon.refresh_git_status_with(crate::git::head);
        daemon
    }

    /// Clears any managed agent hooks left in a configured checkout by a
    /// previous daemon. They name that daemon's ephemeral port and per-boot
    /// token, so every one of them is stale by definition the moment this
    /// process starts — and a stale block fires on every turn of any agent
    /// the user later runs in that directory by hand. Best-effort per
    /// checkout: an unreadable or read-only one must not stop startup.
    pub fn sweep_stale_hooks(&self) {
        for path in self.checkout_paths() {
            for h in &self.harnesses {
                if let Err(e) = h.uninstall(&path) {
                    tracing::warn!(
                        "failed to clear stale {} hooks in {}: {e}",
                        h.name,
                        path.display()
                    );
                }
            }
        }
    }

    fn checkout_paths(&self) -> Vec<PathBuf> {
        let inner = self.inner.lock().unwrap();
        checkouts(&inner.projects).map(|c| c.path.clone()).collect()
    }

    /// The harness an agent template speaks. A template that names none
    /// falls back to one matching its own name, so `name = "claude"` needs
    /// no extra key; anything unrecognized gets [`Harness::generic`], which
    /// installs nothing but still hands the pane the environment.
    fn harness_for(&self, template: &AgentConfig) -> crate::harness::Harness {
        let wanted = template.harness.as_deref().unwrap_or(&template.name);
        self.harnesses
            .iter()
            .find(|h| h.name == wanted)
            .cloned()
            .unwrap_or_else(crate::harness::Harness::generic)
    }

    pub fn template_names(&self) -> Vec<String> {
        self.templates
            .lock()
            .unwrap()
            .iter()
            .map(|t| t.name.clone())
            .collect()
    }

    /// The tree as clients see it: only the open workspace's projects.
    /// Panes in the other workspaces are still alive and still updating —
    /// their rollups show up in [`Daemon::workspaces`] so a working agent
    /// somewhere you are not looking is still visible.
    pub fn snapshot(&self) -> Vec<ProjectInfo> {
        // Read before the tree lock: this touches the store, and holding
        // the tree while doing so would put SQLite on the path of every
        // pane update.
        let notes = self.note_summaries();
        let inner = self.inner.lock().unwrap();
        let open = inner.open;
        inner
            .projects
            .iter()
            .filter(|p| p.workspace == open)
            .map(|p| ProjectInfo {
                id: p.id,
                name: p.name.clone(),
                repositories: p
                    .repositories
                    .iter()
                    .map(|r| RepositoryInfo {
                        id: r.id,
                        name: r.name.clone(),
                        branches: r.branches.clone(),
                        default_branch: r.default_branch.clone(),
                        remote_branches: r.remote_branches.clone(),
                        checkouts: r
                            .checkouts
                            .iter()
                            .map(|c| {
                                let git = c.git.clone();
                                CheckoutInfo {
                                    id: c.id,
                                    // A checkout names the branch currently occupying it,
                                    // including when a process switched outside Argus.
                                    name: git
                                        .as_ref()
                                        .and_then(|status| status.branch.clone())
                                        .unwrap_or_else(|| c.name.clone()),
                                    path: c.path.to_string_lossy().to_string(),
                                    panes: c
                                        .panes
                                        .iter()
                                        .map(|pane| PaneInfo {
                                            id: pane.id,
                                            kind: pane.kind,
                                            title: pane.title.clone(),
                                            status: pane.status,
                                            note: pane.note.clone(),
                                            template: pane.template.clone(),
                                            children: pane
                                                .children
                                                .iter()
                                                .map(|c| argus_protocol::ChildAgentInfo {
                                                    label: c
                                                        .label
                                                        .clone()
                                                        .unwrap_or_else(|| "agent".to_string()),
                                                    status: c.status,
                                                    note: c.note.clone(),
                                                })
                                                .collect(),
                                        })
                                        .collect(),
                                    git,
                                    primary: c.primary,
                                    notes: note_of(&notes, &NoteKey::checkout(&c.path)).0,
                                    has_note: note_of(&notes, &NoteKey::checkout(&c.path)).1,
                                }
                            })
                            .collect(),
                    })
                    .collect(),
                notes: note_of(&notes, &NoteKey::Project(p.name.clone())).0,
                has_note: note_of(&notes, &NoteKey::Project(p.name.clone())).1,
            })
            .collect()
    }

    pub fn subscribe_tree(&self) -> broadcast::Receiver<Vec<ProjectInfo>> {
        self.tree_tx.subscribe()
    }

    pub fn subscribe_workspaces(&self) -> broadcast::Receiver<Vec<WorkspaceInfo>> {
        self.workspaces_tx.subscribe()
    }

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
    fn save_open_workspace(&self, name: &str) {
        if let Err(e) = self.store.set_open_workspace(name) {
            tracing::warn!("could not remember the open workspace: {e}");
        }
    }

    /// The open workspace's id and name — what `add_project` files new
    /// projects under, and what the client shows above the project list.
    fn open_workspace_ref(&self) -> (WorkspaceId, String) {
        let inner = self.inner.lock().unwrap();
        let name = inner
            .workspaces
            .iter()
            .find(|w| w.id == inner.open)
            .map(|w| w.name.clone())
            .unwrap_or_else(|| config::DEFAULT_WORKSPACE.to_string());
        (inner.open, name)
    }

    fn broadcast_tree(&self) {
        // Structural changes only — pane output never reaches here — so
        // recording the session on the same edge is cheap and means the
        // file is never more than one spawn or close out of date.
        self.record_session();
        let _ = self.tree_tx.send(self.snapshot());
    }

    /// The checkout at `path`, whatever workspace it is in.
    fn checkout_at(&self, path: &std::path::Path) -> Option<CheckoutId> {
        let inner = self.inner.lock().unwrap();
        let found = checkouts(&inner.projects)
            .find(|c| same_path(&c.path, path))
            .map(|c| c.id);
        found
    }

    fn broadcast_workspaces(&self) {
        let _ = self.workspaces_tx.send(self.workspaces());
    }
}

/// A row's note summary, or nothing on both counts when it has no note.
fn note_of(
    notes: &HashMap<NoteKey, notes::NoteSummary>,
    key: &NoteKey,
) -> notes::NoteSummary {
    notes.get(key).copied().unwrap_or_default()
}

#[cfg(test)]
mod tests;
