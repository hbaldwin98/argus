use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use argus_protocol::{
    Cell, CheckoutId, CheckoutInfo, GitStatus, IdGen, PaneId, PaneInfo, PaneKind, PaneStatus,
    ProjectId, ProjectInfo, RepositoryId, RepositoryInfo, ServerMsg, WorkspaceId, WorkspaceInfo,
};
use tokio::sync::broadcast;

mod git_ops;
mod hook;
mod notes;
mod panes;
mod session;
mod sync;
mod tree;

use hook::{gen_token, valid_session_id};
use session::{agent_args, nothing_to_resume, Resumed};
use tree::*;

use crate::config::{self, AgentConfig, ConfigFile};
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

/// Identifies one attached client for as long as its connection lasts.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ViewerId(u64);

/// The requested pane sizes of every attached client, and what was last
/// actually applied to each pty.
#[derive(Default)]
struct Viewers {
    wanted: HashMap<PaneId, HashMap<ViewerId, (u16, u16)>>,
    applied: HashMap<PaneId, (u16, u16)>,
}

impl Viewers {
    /// The size a pane should be given the clients currently showing it:
    /// the smallest request in each dimension, so no client is ever sent a
    /// grid with more rows or columns than it has room to draw. A client
    /// with a bigger window pads; the alternative — sizing to the largest —
    /// truncates content out of the smaller one entirely.
    fn effective(&self, pane: PaneId) -> Option<(u16, u16)> {
        let wanted = self.wanted.get(&pane)?;
        wanted
            .values()
            .copied()
            .reduce(|(ar, ac), (br, bc)| (ar.min(br), ac.min(bc)))
    }

    /// The size to apply for `pane`, or `None` when nothing would change.
    /// A pane no client is showing keeps the size it has: reflowing a
    /// running program's output for an audience of nobody only destroys it.
    fn pending(&self, pane: PaneId) -> Option<(u16, u16)> {
        let size = self.effective(pane)?;
        (self.applied.get(&pane) != Some(&size)).then_some(size)
    }
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
        inner
            .projects
            .iter()
            .flat_map(|p| p.repositories.iter())
            .flat_map(|r| r.checkouts.iter())
            .map(|c| c.path.clone())
            .collect()
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
        inner
            .projects
            .iter()
            .flat_map(|p| p.repositories.iter())
            .flat_map(|r| r.checkouts.iter())
            .find(|c| c.path == path || c.path.canonicalize().ok() == path.canonicalize().ok())
            .map(|c| c.id)
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

/// Whether a hook's report should be dropped rather than applied, for
/// either of two reasons. The pane has already exited, and nothing said
/// afterwards — a `Stop` racing a crash, say — should resurrect its row. Or
/// `Idle` is arriving over a state that is still holding something for the
/// operator, where "my turn ended" is not news that clears "blocked on the
/// db password".
fn is_stale_report(current: PaneStatus, reported: PaneStatus) -> bool {
    matches!(current, PaneStatus::Exited { .. })
        || reported == PaneStatus::Idle
            && matches!(
                current,
                PaneStatus::Waiting
                    | PaneStatus::NeedsReview
                    | PaneStatus::Done
                    | PaneStatus::Failed
            )
}

/// Reads one HTTP/1.1 request, checks the bearer token, and applies the pane
/// operation its path encodes. Hand-rolled rather than pulling in an HTTP server crate:
/// the request shape is entirely our own (we generate every hook command
/// that ever calls this), so there's nothing to be robust against beyond
/// "well-formed or ignored".
/// A title has to survive being drawn in a one-line row, and arrives from a
/// model, so it is flattened to a single line and cut to something a column
/// can hold.
fn clean_title(raw: &str) -> String {
    const MAX: usize = 48;
    let flat = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.char_indices().nth(MAX) {
        Some((i, _)) => format!("{}…", flat[..i].trim_end()),
        None => flat,
    }
}

/// What one branch refresh learned about a repository, carried across the
/// gap where the daemon's lock is down.
struct BranchState {
    id: RepositoryId,
    /// Local branches no checkout of it is sitting on.
    free: Vec<String>,
    default: Option<String>,
    remote: Vec<String>,
}

/// Runs git in `dir` and turns a non-zero exit into git's own message.
/// Anything the user is going to have to act on is already in there.
async fn run_git(dir: &std::path::Path, args: &[&str]) -> anyhow::Result<()> {
    let output = crate::command::git()
        .args(args)
        .current_dir(dir)
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let message = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        anyhow::bail!("{message}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::session::RESUME_GRACE;
    use super::*;
    use crate::config::ProjectConfig;
    use argus_protocol::{
        parse_pane_path, Endpoint, NoteCounts, NoteTarget, ReviewAnchor, ReviewBase, TodoState,
        MAX_NOTE_BYTES,
    };

    /// A daemon with one project whose primary checkout is `primary`, and no
    /// panes. Nothing here touches disk: `Daemon::new` only expands paths,
    /// and every test below injects its own worktree listing.
    fn daemon_with_primary(primary: &str) -> Arc<Daemon> {
        Daemon::new(ConfigFile {
            workspaces: Vec::new(),
            projects: vec![ProjectConfig {
                name: "proj".to_string(),
                root: None,
                repos: vec![primary.to_string()],
                workspace: None,
                ..Default::default()
            }],
            agents: Vec::new(),
            harnesses: Vec::new(),
        })
    }

    fn daemon_with_repositories(repositories: &[&str]) -> Arc<Daemon> {
        Daemon::new(ConfigFile {
            workspaces: Vec::new(),
            projects: vec![ProjectConfig {
                name: "proj".to_string(),
                root: None,
                repos: repositories.iter().map(|path| path.to_string()).collect(),
                workspace: None,
                ..Default::default()
            }],
            agents: Vec::new(),
            harnesses: Vec::new(),
        })
    }

    fn checkout_paths(d: &Daemon) -> Vec<String> {
        d.snapshot()
            .into_iter()
            .flat_map(|p| p.repositories)
            .flat_map(|r| r.checkouts)
            .map(|c| c.path)
            .collect()
    }

    // ---- notes -------------------------------------------------------

    #[test]
    fn a_checkout_note_round_trips_and_its_counts_reach_the_tree() {
        let d = daemon_with_primary("/repo");
        let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;
        let target = NoteTarget::Checkout(checkout);

        assert_eq!(d.note(target).unwrap().body, "", "an unwritten note is empty");
        assert!(!d.snapshot()[0].repositories[0].checkouts[0].has_note);

        d.set_note(target, "- [ ] one
- [x] two
- [!] three
".to_string())
            .unwrap();

        let checkout = &d.snapshot()[0].repositories[0].checkouts[0];
        assert!(checkout.has_note);
        assert_eq!(
            checkout.notes,
            NoteCounts {
                open: 1,
                done: 1,
                pinned: 1
            }
        );
        assert_eq!(d.note(target).unwrap().todos.len(), 3);
    }

    #[test]
    fn a_project_note_is_separate_from_its_checkouts() {
        let d = daemon_with_primary("/repo");
        let project = d.snapshot()[0].id;
        let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;

        d.set_note(NoteTarget::Project(project), "- [ ] project work".to_string())
            .unwrap();
        d.set_note(
            NoteTarget::Checkout(checkout),
            "- [ ] checkout work
- [ ] more".to_string(),
        )
        .unwrap();

        let tree = d.snapshot();
        assert_eq!(tree[0].notes.open, 1);
        assert_eq!(tree[0].repositories[0].checkouts[0].notes.open, 2);
    }

    #[test]
    fn counts_roll_up_from_every_checkout_to_the_project() {
        let d = daemon_with_primary("/repo");
        d.reconcile_worktrees_with(|_| listing(&["/repo", "/repo/wt"]));
        let tree = d.snapshot();
        let project = tree[0].id;
        for checkout in &tree[0].repositories[0].checkouts {
            d.set_note(
                NoteTarget::Checkout(checkout.id),
                "- [ ] a
- [ ] b
- [x] c
".to_string(),
            )
            .unwrap();
        }
        d.set_note(NoteTarget::Project(project), "- [!] read me first".to_string())
            .unwrap();

        let tree = d.snapshot();
        assert_eq!(
            tree[0].repositories[0].note_rollup(),
            NoteCounts {
                open: 4,
                done: 2,
                pinned: 0
            },
            "the repository sums its checkouts and holds no note of its own"
        );
        assert_eq!(
            tree[0].note_rollup(),
            NoteCounts {
                open: 4,
                done: 2,
                pinned: 1
            },
            "the project adds its own note to what is beneath it"
        );
    }

    #[test]
    fn emptying_a_note_clears_the_row() {
        let d = daemon_with_primary("/repo");
        let target = NoteTarget::Checkout(d.snapshot()[0].repositories[0].checkouts[0].id);
        d.set_note(target, "- [ ] something".to_string()).unwrap();
        assert!(d.snapshot()[0].repositories[0].checkouts[0].has_note);

        d.set_note(target, String::new()).unwrap();

        let checkout = &d.snapshot()[0].repositories[0].checkouts[0];
        assert!(!checkout.has_note);
        assert!(checkout.notes.is_empty());
    }

    #[test]
    fn toggling_a_checkbox_leaves_the_rest_of_the_note_alone() {
        let d = daemon_with_primary("/repo");
        let target = NoteTarget::Checkout(d.snapshot()[0].repositories[0].checkouts[0].id);
        d.set_note(target, "# Plan

- [ ] first
- [ ] second
".to_string())
            .unwrap();

        let note = d.set_todo(target, 2, TodoState::Done).unwrap();

        assert_eq!(note.body, "# Plan

- [x] first
- [ ] second
");
        assert_eq!(note.counts(), NoteCounts { open: 1, done: 1, pinned: 0 });
    }

    #[test]
    fn toggling_a_line_that_is_not_a_checkbox_is_refused() {
        let d = daemon_with_primary("/repo");
        let target = NoteTarget::Checkout(d.snapshot()[0].repositories[0].checkouts[0].id);
        d.set_note(target, "# Plan
- [ ] first
".to_string())
            .unwrap();

        let err = d.set_todo(target, 0, TodoState::Done).unwrap_err().to_string();

        assert!(err.contains("not a checkbox"), "{err}");
        assert_eq!(d.note(target).unwrap().body, "# Plan
- [ ] first
");
    }

    #[test]
    fn a_note_on_a_stale_id_is_refused_rather_than_filed_somewhere_else() {
        let d = daemon_with_primary("/repo");
        let err = d
            .set_note(NoteTarget::Checkout(CheckoutId(9999)), "x".to_string())
            .unwrap_err()
            .to_string();
        assert!(err.contains("no such checkout"), "{err}");
    }

    #[test]
    fn a_note_too_large_to_carry_is_refused() {
        let d = daemon_with_primary("/repo");
        let target = NoteTarget::Checkout(d.snapshot()[0].repositories[0].checkouts[0].id);
        let err = d
            .set_note(target, "x".repeat(MAX_NOTE_BYTES + 1))
            .unwrap_err()
            .to_string();
        assert!(err.contains("exceeds"), "{err}");
    }

    fn listing(paths: &[&str]) -> Vec<PathBuf> {
        paths.iter().map(PathBuf::from).collect()
    }

    // --- the pane API ------------------------------------------------------

    #[test]
    fn a_title_from_a_model_is_flattened_and_cut_to_fit_a_row() {
        assert_eq!(clean_title("  fixing\n the   pty  "), "fixing the pty");
        let long = clean_title(&"x".repeat(200));
        assert!(
            long.chars().count() <= 49,
            "got {} chars",
            long.chars().count()
        );
        assert!(long.ends_with('…'));
        assert_eq!(clean_title("   "), "");
    }

    #[test]
    fn session_ids_are_validated_without_restricting_cli_specific_syntax() {
        assert_eq!(
            valid_session_id("  thread/abc:123  ").as_deref(),
            Some("thread/abc:123")
        );
        assert!(valid_session_id("").is_none());
        assert!(valid_session_id("bad\nid").is_none());
        assert!(valid_session_id(&"x".repeat(513)).is_none());
    }

    /// A daemon holding one live agent pane, and that pane's id.
    async fn daemon_with_an_agent(dir: &std::path::Path) -> (Arc<Daemon>, PaneId) {
        let d = daemon_with_fake_claude(dir);
        let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();
        (d, pane)
    }

    fn pane_info(d: &Daemon, pane: PaneId) -> PaneInfo {
        d.snapshot()
            .into_iter()
            .flat_map(|p| p.repositories)
            .flat_map(|r| r.checkouts)
            .flat_map(|c| c.panes)
            .find(|p| p.id == pane)
            .expect("pane should still be in the tree")
    }

    #[tokio::test]
    async fn an_agent_can_rename_its_own_row() {
        // The feature: four rows all reading "claude" say nothing about
        // which one is worth looking at.
        let dir = tempfile::tempdir().unwrap();
        let (d, pane) = daemon_with_an_agent(dir.path()).await;
        assert_eq!(pane_info(&d, pane).title, "claude");

        d.set_pane_title(pane, "fixing the pty deadlock");
        assert_eq!(pane_info(&d, pane).title, "fixing the pty deadlock");

        d.close_pane(pane).unwrap();
    }

    #[tokio::test]
    async fn an_agent_spawned_inside_a_pane_reports_as_a_child_of_it() {
        // The bug: a CLI started from inside a pane inherits that pane's
        // hook URL and token, so every turn it takes used to overwrite the
        // row belonging to the agent that started it.
        let dir = tempfile::tempdir().unwrap();
        let (d, pane) = daemon_with_an_agent(dir.path()).await;
        d.set_pane_session_id(pane, "parent-session");
        d.set_pane_title(pane, "fixing the pty deadlock");
        d.report_pane_status(pane, Some("parent-session"), PaneStatus::Working, None);

        d.report_pane_title(pane, Some("child-session"), "reading the hook table");
        d.report_pane_status(
            pane,
            Some("child-session"),
            PaneStatus::Waiting,
            Some("needs a password".into()),
        );

        let info = pane_info(&d, pane);
        assert_eq!(info.title, "fixing the pty deadlock", "the parent's row");
        assert_eq!(info.status, PaneStatus::Working);
        assert_eq!(info.note, None);
        assert_eq!(info.children.len(), 1);
        assert_eq!(info.children[0].label, "reading the hook table");
        assert_eq!(info.children[0].status, PaneStatus::Waiting);
        assert_eq!(info.children[0].note.as_deref(), Some("needs a password"));

        d.close_pane(pane).unwrap();
    }

    #[tokio::test]
    async fn a_child_that_has_finished_stops_being_listed() {
        let dir = tempfile::tempdir().unwrap();
        let (d, pane) = daemon_with_an_agent(dir.path()).await;
        d.set_pane_session_id(pane, "parent-session");
        d.report_pane_status(pane, Some("child-session"), PaneStatus::Working, None);
        assert_eq!(pane_info(&d, pane).children.len(), 1);

        d.report_pane_status(pane, Some("child-session"), PaneStatus::Idle, None);
        assert!(pane_info(&d, pane).children.is_empty());

        d.close_pane(pane).unwrap();
    }

    #[tokio::test]
    async fn an_exited_parent_forgets_its_children() {
        let dir = tempfile::tempdir().unwrap();
        let (d, pane) = daemon_with_an_agent(dir.path()).await;
        d.set_pane_session_id(pane, "parent-session");
        d.report_pane_status(pane, Some("child-session"), PaneStatus::Waiting, None);
        assert_eq!(pane_info(&d, pane).children.len(), 1);

        d.clone().mark_pane_exited(pane, Some(1));

        let info = pane_info(&d, pane);
        assert_eq!(info.status, PaneStatus::Exited { code: Some(1) });
        assert!(info.children.is_empty());
        d.close_pane(pane).unwrap();
    }
    #[tokio::test]
    async fn a_parent_going_idle_forgets_what_ran_under_it() {
        // Most children never report finishing: a subagent's harness fires
        // the parent's hooks, not its own. The turn ending is what says
        // they are done, so the row must not keep claiming they are working.
        let dir = tempfile::tempdir().unwrap();
        let (d, pane) = daemon_with_an_agent(dir.path()).await;
        d.set_pane_session_id(pane, "parent-session");
        d.report_pane_status(pane, Some("parent-session"), PaneStatus::Working, None);
        d.report_pane_status(pane, Some("child-session"), PaneStatus::Working, None);
        assert_eq!(pane_info(&d, pane).children.len(), 1);

        d.report_pane_status(pane, Some("parent-session"), PaneStatus::Idle, None);
        assert!(pane_info(&d, pane).children.is_empty());

        // A background agent that outlives the turn is not lost by this:
        // the next thing it reports lists it again.
        d.report_pane_status(pane, Some("child-session"), PaneStatus::Working, None);
        assert_eq!(pane_info(&d, pane).children.len(), 1);

        d.close_pane(pane).unwrap();
    }

    #[tokio::test]
    async fn a_child_that_goes_quiet_stops_being_listed() {
        // The backstop for a child that is killed or crashes mid-turn:
        // nothing reports its ending, and the parent keeps working, so
        // without this its row would sit there indefinitely.
        let dir = tempfile::tempdir().unwrap();
        let (d, pane) = daemon_with_an_agent(dir.path()).await;
        d.set_pane_session_id(pane, "parent-session");
        d.report_pane_status(pane, Some("parent-session"), PaneStatus::Working, None);
        d.report_pane_status(pane, Some("live-child"), PaneStatus::Working, None);
        d.report_pane_status(pane, Some("dead-child"), PaneStatus::Working, None);

        // Age one of them past the silence the sweep allows.
        {
            let mut inner = d.inner.lock().unwrap();
            let p = find_pane(&mut inner.projects, pane).unwrap();
            let child = p
                .children
                .iter_mut()
                .find(|c| c.session_id == "dead-child")
                .unwrap();
            child.at = std::time::Instant::now() - CHILD_SILENCE - Duration::from_secs(1);
        }
        d.drop_silent_children();

        let listed = pane_info(&d, pane).children;
        assert_eq!(listed.len(), 1, "the quiet one is gone");
        assert_eq!(listed[0].label, "agent");

        d.close_pane(pane).unwrap();
    }

    fn pane_size(d: &Daemon, pane: PaneId) -> (u16, u16) {
        let (rows, cols, _, _, _, _, _) = d.subscribe_pane(pane).unwrap();
        (rows, cols)
    }

    #[tokio::test]
    async fn one_client_sizes_a_pane_to_exactly_what_it_asked_for() {
        let dir = tempfile::tempdir().unwrap();
        let d = daemon_with_fake_claude(dir.path());
        let pane = d.spawn_shell(only_checkout(&d)).unwrap();
        let alone = d.new_viewer();

        d.resize_pane(alone, pane, 40, 120).unwrap();

        assert_eq!(pane_size(&d, pane), (40, 120));
        let _ = d.close_pane(pane);
    }

    #[tokio::test]
    async fn two_clients_get_a_pane_that_fits_in_both_of_their_windows() {
        // Sizing to the later request instead would leave the client that
        // asked first drawing a grid wider or taller than its own box.
        let dir = tempfile::tempdir().unwrap();
        let d = daemon_with_fake_claude(dir.path());
        let pane = d.spawn_shell(only_checkout(&d)).unwrap();
        let (tall, wide) = (d.new_viewer(), d.new_viewer());

        d.resize_pane(tall, pane, 60, 80).unwrap();
        d.resize_pane(wide, pane, 30, 200).unwrap();

        assert_eq!(pane_size(&d, pane), (30, 80));
        let _ = d.close_pane(pane);
    }

    #[tokio::test]
    async fn a_pane_grows_back_when_the_client_holding_it_small_leaves() {
        let dir = tempfile::tempdir().unwrap();
        let d = daemon_with_fake_claude(dir.path());
        let pane = d.spawn_shell(only_checkout(&d)).unwrap();
        let (big, small) = (d.new_viewer(), d.new_viewer());
        d.resize_pane(big, pane, 60, 200).unwrap();
        d.resize_pane(small, pane, 20, 80).unwrap();

        d.release_viewer(small);

        assert_eq!(pane_size(&d, pane), (60, 200));
        let _ = d.close_pane(pane);
    }

    #[tokio::test]
    async fn a_pane_the_last_client_stopped_showing_keeps_its_size() {
        // Nobody is watching, so there is no size that would be better —
        // and reflowing a running program's output for no reader only
        // damages what it has already drawn.
        let dir = tempfile::tempdir().unwrap();
        let d = daemon_with_fake_claude(dir.path());
        let pane = d.spawn_shell(only_checkout(&d)).unwrap();
        let only = d.new_viewer();
        d.resize_pane(only, pane, 44, 111).unwrap();

        d.release_pane_size(only, pane);

        assert_eq!(pane_size(&d, pane), (44, 111));
        let _ = d.close_pane(pane);
    }

    #[test]
    fn a_size_already_applied_is_not_applied_again() {
        // Every applied size costs every subscriber a full-grid snapshot,
        // so a second client agreeing with the first must be free.
        let mut viewers = Viewers::default();
        let pane = PaneId(1);
        let (first, second) = (ViewerId(0), ViewerId(1));
        viewers
            .wanted
            .entry(pane)
            .or_default()
            .insert(first, (30, 80));
        assert_eq!(viewers.pending(pane), Some((30, 80)));
        viewers.applied.insert(pane, (30, 80));

        viewers
            .wanted
            .entry(pane)
            .or_default()
            .insert(second, (30, 80));

        assert_eq!(viewers.pending(pane), None);
    }

    #[tokio::test]
    async fn a_pane_lists_only_so_many_children() {
        let dir = tempfile::tempdir().unwrap();
        let (d, pane) = daemon_with_an_agent(dir.path()).await;
        d.set_pane_session_id(pane, "parent-session");
        for i in 0..MAX_CHILDREN + 4 {
            d.report_pane_status(pane, Some(&format!("child-{i}")), PaneStatus::Working, None);
        }
        assert_eq!(pane_info(&d, pane).children.len(), MAX_CHILDREN);
        d.close_pane(pane).unwrap();
    }

    #[tokio::test]
    async fn a_session_started_mid_turn_cannot_take_over_the_row() {
        // A nested CLI announces its own session start while its parent is
        // working; letting that claim stick would leave the row resuming
        // the wrong conversation.
        let dir = tempfile::tempdir().unwrap();
        let (d, pane) = daemon_with_an_agent(dir.path()).await;
        d.set_pane_session_id(pane, "parent-session");
        d.report_pane_status(pane, Some("parent-session"), PaneStatus::Working, None);

        d.set_pane_session_id(pane, "nested-session");

        assert_eq!(
            d.session_panes()[0].harness_session_id.as_deref(),
            Some("parent-session")
        );
        assert_eq!(pane_info(&d, pane).children.len(), 1);

        d.close_pane(pane).unwrap();
    }

    #[tokio::test]
    async fn the_panes_own_agent_can_still_start_a_new_conversation() {
        // `/clear` gives the pane's agent a new session id, and it arrives
        // while the pane is idle. That is the row's own agent, so it keeps
        // the row — and whatever ran under the old conversation is gone.
        let dir = tempfile::tempdir().unwrap();
        let (d, pane) = daemon_with_an_agent(dir.path()).await;
        d.set_pane_session_id(pane, "first-session");
        d.report_pane_status(pane, Some("child-session"), PaneStatus::Working, None);

        d.set_pane_session_id(pane, "second-session");

        assert_eq!(
            d.session_panes()[0].harness_session_id.as_deref(),
            Some("second-session")
        );
        assert!(pane_info(&d, pane).children.is_empty());

        d.close_pane(pane).unwrap();
    }

    #[tokio::test]
    async fn a_report_with_no_session_at_all_is_the_panes_own() {
        // `argus-hook status` typed by an agent carries no session id, and
        // must keep working as the pane's own voice.
        let dir = tempfile::tempdir().unwrap();
        let (d, pane) = daemon_with_an_agent(dir.path()).await;
        d.set_pane_session_id(pane, "parent-session");

        d.report_pane_title(pane, None, "renamed by hand");
        d.report_pane_status(pane, None, PaneStatus::NeedsReview, None);

        let info = pane_info(&d, pane);
        assert_eq!(info.title, "renamed by hand");
        assert_eq!(info.status, PaneStatus::NeedsReview);
        assert!(info.children.is_empty());

        d.close_pane(pane).unwrap();
    }

    #[tokio::test]
    async fn an_empty_rename_leaves_the_row_alone() {
        // Better the agent's name than a blank row.
        let dir = tempfile::tempdir().unwrap();
        let (d, pane) = daemon_with_an_agent(dir.path()).await;
        d.set_pane_title(pane, "   \n  ");
        assert_eq!(pane_info(&d, pane).title, "claude");
        d.close_pane(pane).unwrap();
    }

    #[tokio::test]
    async fn a_stalled_pane_says_what_it_is_stalled_on() {
        // The reason to have a note at all: knowing a pane needs you is
        // only half of it.
        let dir = tempfile::tempdir().unwrap();
        let (d, pane) = daemon_with_an_agent(dir.path()).await;

        d.set_pane_hook_status(
            pane,
            PaneStatus::Waiting,
            Some("needs the staging password".to_string()),
        );
        let info = pane_info(&d, pane);
        assert_eq!(info.status, PaneStatus::Waiting);
        assert_eq!(info.note.as_deref(), Some("needs the staging password"));

        d.close_pane(pane).unwrap();
    }

    #[tokio::test]
    async fn the_note_goes_away_with_the_state_it_explained() {
        // A stale "waiting on a password" under a working row is worse than
        // no note at all.
        let dir = tempfile::tempdir().unwrap();
        let (d, pane) = daemon_with_an_agent(dir.path()).await;

        d.set_pane_hook_status(pane, PaneStatus::Waiting, Some("needs a password".into()));
        d.set_pane_hook_status(pane, PaneStatus::Working, None);

        let info = pane_info(&d, pane);
        assert_eq!(info.status, PaneStatus::Working);
        assert_eq!(info.note, None);

        d.close_pane(pane).unwrap();
    }

    #[tokio::test]
    async fn a_failure_keeps_the_pane_alive_and_says_why() {
        // Distinct from an exit: the process is still there to answer.
        let dir = tempfile::tempdir().unwrap();
        let (d, pane) = daemon_with_an_agent(dir.path()).await;

        d.set_pane_hook_status(
            pane,
            PaneStatus::Failed,
            Some("cargo test won't build".into()),
        );
        let info = pane_info(&d, pane);
        assert_eq!(info.status, PaneStatus::Failed);
        assert!(info.note.is_some());

        d.close_pane(pane).unwrap();
    }

    #[tokio::test]
    async fn automatic_idle_does_not_erase_an_explicit_completion_state() {
        let dir = tempfile::tempdir().unwrap();
        let (d, pane) = daemon_with_an_agent(dir.path()).await;

        for status in [
            PaneStatus::Waiting,
            PaneStatus::NeedsReview,
            PaneStatus::Done,
            PaneStatus::Failed,
        ] {
            d.set_pane_hook_status(pane, status, Some("still relevant".into()));
            d.set_pane_hook_status(pane, PaneStatus::Idle, None);
            let info = pane_info(&d, pane);
            assert_eq!(info.status, status);
            assert_eq!(info.note.as_deref(), Some("still relevant"));
            d.set_pane_hook_status(pane, PaneStatus::Working, None);
        }

        d.close_pane(pane).unwrap();
    }

    /// A daemon whose one agent template has a restart policy.
    fn daemon_with_a_restarting_agent(
        dir: &std::path::Path,
        restart: crate::config::Restart,
    ) -> Arc<Daemon> {
        Daemon::new(ConfigFile {
            workspaces: Vec::new(),
            projects: vec![ProjectConfig {
                name: "proj".to_string(),
                repos: vec![dir.to_string_lossy().to_string()],
                ..Default::default()
            }],
            agents: vec![AgentConfig {
                name: "claude".to_string(),
                cmd: vec!["echo".to_string(), "hi".to_string()],
                env: Default::default(),
                harness: None,
                restart,
            }],
            harnesses: Vec::new(),
        })
    }

    fn panes_of(d: &Daemon) -> Vec<PaneInfo> {
        d.snapshot()
            .remove(0)
            .repositories
            .remove(0)
            .checkouts
            .remove(0)
            .panes
    }

    #[tokio::test]
    async fn an_agent_that_exits_leaves_its_row_alone_by_default() {
        let dir = tempfile::tempdir().unwrap();
        let (d, pane) = daemon_with_an_agent(dir.path()).await;

        d.mark_pane_exited(pane, Some(1));

        let panes = panes_of(&d);
        assert_eq!(panes.len(), 1);
        assert_eq!(panes[0].id, pane, "the same dead row, for reading");
        assert_eq!(panes[0].status, PaneStatus::Exited { code: Some(1) });
    }

    #[tokio::test]
    async fn on_failure_starts_the_agent_again_and_a_clean_exit_does_not() {
        let dir = tempfile::tempdir().unwrap();
        let d = daemon_with_a_restarting_agent(dir.path(), crate::config::Restart::OnFailure);
        let checkout = only_checkout(&d);
        let first = d.spawn_agent(checkout, "claude").unwrap();

        d.mark_pane_exited(first, Some(1));

        let panes = panes_of(&d);
        assert_eq!(panes.len(), 1, "the dead row is replaced, not joined");
        let second = panes[0].id;
        assert_ne!(second, first, "a new pane is running");
        assert_ne!(panes[0].status, PaneStatus::Exited { code: Some(1) });

        d.mark_pane_exited(second, Some(0));

        let panes = panes_of(&d);
        assert_eq!(panes[0].id, second, "a clean exit is the agent finishing");
        assert_eq!(panes[0].status, PaneStatus::Exited { code: Some(0) });
        close_all(&d);
    }

    #[tokio::test]
    async fn always_starts_the_agent_again_however_it_ended() {
        let dir = tempfile::tempdir().unwrap();
        let d = daemon_with_a_restarting_agent(dir.path(), crate::config::Restart::Always);
        let checkout = only_checkout(&d);
        let first = d.spawn_agent(checkout, "claude").unwrap();

        d.mark_pane_exited(first, Some(0));

        let panes = panes_of(&d);
        assert_eq!(panes.len(), 1);
        assert_ne!(panes[0].id, first);
        close_all(&d);
    }

    #[tokio::test]
    async fn a_cli_that_dies_on_every_start_is_left_where_the_operator_can_read_it() {
        // Restarting forever spends the machine on a row nobody ever sees.
        let dir = tempfile::tempdir().unwrap();
        let d = daemon_with_a_restarting_agent(dir.path(), crate::config::Restart::Always);
        let checkout = only_checkout(&d);
        let mut pane = d.spawn_agent(checkout, "claude").unwrap();

        for _ in 0..6 {
            d.mark_pane_exited(pane, Some(1));
            pane = panes_of(&d)[0].id;
        }

        let panes = panes_of(&d);
        assert_eq!(panes.len(), 1);
        assert_eq!(
            panes[0].status,
            PaneStatus::Exited { code: Some(1) },
            "it gave up and left the exit visible"
        );
        close_all(&d);
    }

    #[tokio::test]
    async fn closing_a_pane_is_never_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let d = daemon_with_a_restarting_agent(dir.path(), crate::config::Restart::Always);
        let checkout = only_checkout(&d);
        let pane = d.spawn_agent(checkout, "claude").unwrap();

        d.close_pane(pane).unwrap();
        // Whatever the process does on its way out arrives after the row
        // has already gone.
        d.mark_pane_exited(pane, Some(1));

        assert!(panes_of(&d).is_empty(), "closing means closing");
    }

    #[tokio::test]
    async fn a_report_never_resurrects_an_exited_pane() {
        // A Stop hook racing a crash must not relabel a dead row as idle.
        let dir = tempfile::tempdir().unwrap();
        let (d, pane) = daemon_with_an_agent(dir.path()).await;
        d.mark_pane_exited(pane, Some(1));

        d.set_pane_hook_status(pane, PaneStatus::Idle, Some("all done".into()));

        let info = pane_info(&d, pane);
        assert_eq!(info.status, PaneStatus::Exited { code: Some(1) });
        assert_eq!(info.note, None, "an exited pane explains nothing");

        d.close_pane(pane).unwrap();
    }

    #[tokio::test]
    async fn a_rename_will_not_relabel_a_dead_row() {
        let dir = tempfile::tempdir().unwrap();
        let (d, pane) = daemon_with_an_agent(dir.path()).await;
        d.mark_pane_exited(pane, Some(0));

        d.set_pane_title(pane, "still working on it");
        assert_eq!(pane_info(&d, pane).title, "claude");

        d.close_pane(pane).unwrap();
    }

    #[tokio::test]
    async fn every_agent_pane_is_handed_the_hook_environment() {
        // The universal floor: a harness Argus knows nothing about can still
        // report, because the variables are always there.
        let dir = tempfile::tempdir().unwrap();
        let d = daemon_with_fake_claude(dir.path());
        d.start_hook_server().unwrap();
        let port = d.hook_port.load(std::sync::atomic::Ordering::Relaxed);
        assert_ne!(port, 0);

        let env = crate::harness::env(PaneId(1), port, &d.hook_token);
        let url = env
            .iter()
            .find(|(k, _)| k == crate::harness::URL_VAR)
            .map(|(_, v)| v.clone())
            .unwrap();
        assert!(url.contains(&port.to_string()));
        assert!(parse_pane_path("/pane/1/title").is_some());
    }

    fn daemon_with_two_agent_checkouts(
        first: &std::path::Path,
        second: &std::path::Path,
    ) -> Arc<Daemon> {
        Daemon::new(ConfigFile {
            workspaces: Vec::new(),
            projects: vec![ProjectConfig {
                name: "proj".to_string(),
                root: None,
                repos: vec![
                    first.to_string_lossy().to_string(),
                    second.to_string_lossy().to_string(),
                ],
                workspace: None,
                ..Default::default()
            }],
            agents: vec![AgentConfig {
                name: "claude".to_string(),
                cmd: vec![if cfg!(windows) { "cmd" } else { "sh" }.to_string()],
                env: Default::default(),
                harness: None,
                restart: Default::default(),
            }],
            harnesses: Vec::new(),
        })
    }

    #[tokio::test]
    async fn an_agent_can_move_its_live_pane_to_another_checkout() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let d = daemon_with_two_agent_checkouts(first.path(), second.path());
        d.start_hook_server().unwrap();
        let source = d.snapshot()[0].repositories[0].checkouts[0].id;
        let pane = d.spawn_agent(source, "claude").unwrap();

        d.move_agent_to_checkout(pane, second.path()).unwrap();

        let tree = d.snapshot();
        assert!(tree[0].repositories[0].checkouts[0].panes.is_empty());
        assert_eq!(tree[0].repositories[1].checkouts[0].panes[0].id, pane);
        assert_eq!(d.session_panes()[0].checkout_path, second.path());
        d.close_pane(pane).unwrap();
    }

    #[tokio::test]
    async fn an_authorized_checkout_hook_moves_the_agent_pane() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let d = daemon_with_two_agent_checkouts(first.path(), second.path());
        d.start_hook_server().unwrap();
        let source = d.snapshot()[0].repositories[0].checkouts[0].id;
        let pane = d.spawn_agent(source, "claude").unwrap();
        let body = second.path().to_string_lossy();
        let request = format!(
            "POST /pane/{}/checkout HTTP/1.1\r\nAuthorization: Bearer {}\r\nContent-Length: {}\r\n\r\n{}",
            pane.0,
            d.hook_token,
            body.len(),
            body
        );

        let port = d.hook_port.load(std::sync::atomic::Ordering::Relaxed);
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();

        assert!(response.starts_with(b"HTTP/1.1 200 OK"));
        assert_eq!(
            d.snapshot()[0].repositories[1].checkouts[0].panes[0].id,
            pane
        );
        d.close_pane(pane).unwrap();
    }

    #[tokio::test]
    async fn an_authorized_session_hook_records_exact_identity() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let dir = tempfile::tempdir().unwrap();
        let d = daemon_with_claude_aliases(dir.path(), &["claude"]);
        d.start_hook_server().unwrap();
        let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();
        let body = "session-123";
        let request = format!(
            "POST /pane/{}/session HTTP/1.1\r\nAuthorization: Bearer {}\r\nContent-Length: {}\r\n\r\n{}",
            pane.0,
            d.hook_token,
            body.len(),
            body
        );
        let port = d.hook_port.load(std::sync::atomic::Ordering::Relaxed);
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();

        assert!(response.starts_with(b"HTTP/1.1 200 OK"));
        assert_eq!(
            d.session_panes()[0].harness_session_id.as_deref(),
            Some(body)
        );
        d.close_pane(pane).unwrap();
    }

    #[test]
    fn session_identity_arriving_before_pane_registration_is_retained() {
        let d = daemon_with_primary("/repo");
        let pane = PaneId(42);
        d.starting_agents
            .lock()
            .unwrap()
            .insert(pane, PendingStart::default());

        d.set_pane_session_id(pane, "session-early");

        assert_eq!(
            d.starting_agents
                .lock()
                .unwrap()
                .get(&pane)
                .and_then(|pending| pending.harness_session_id.as_deref()),
            Some("session-early")
        );
    }

    #[test]
    fn status_arriving_before_pane_registration_is_retained() {
        let d = daemon_with_primary("/repo");
        let pane = PaneId(42);
        d.starting_agents
            .lock()
            .unwrap()
            .insert(pane, PendingStart::default());

        d.set_pane_hook_status(pane, PaneStatus::Working, None);
        d.set_pane_hook_status(
            pane,
            PaneStatus::Waiting,
            Some(" needs the database password ".to_string()),
        );

        assert_eq!(
            d.starting_agents.lock().unwrap()[&pane].status,
            Some((
                PaneStatus::Waiting,
                Some("needs the database password".to_string())
            ))
        );
    }

    #[test]
    fn title_arriving_before_pane_registration_is_retained() {
        let d = daemon_with_primary("/repo");
        let pane = PaneId(42);
        d.starting_agents
            .lock()
            .unwrap()
            .insert(pane, PendingStart::default());

        d.set_pane_title(pane, "starting up");
        d.set_pane_title(pane, " fixing the pty deadlock ");

        assert_eq!(
            d.starting_agents.lock().unwrap()[&pane].title.as_deref(),
            Some("fixing the pty deadlock")
        );
    }

    #[test]
    fn child_reports_arriving_before_pane_registration_are_retained() {
        let d = daemon_with_primary("/repo");
        let pane = PaneId(42);
        d.starting_agents.lock().unwrap().insert(
            pane,
            PendingStart {
                harness_session_id: Some("parent-session".to_string()),
                ..PendingStart::default()
            },
        );

        d.report_pane_status(
            pane,
            Some("child-session"),
            PaneStatus::Waiting,
            Some("needs permission".to_string()),
        );
        d.report_pane_title(pane, Some("child-session"), "test runner");

        let starting = d.starting_agents.lock().unwrap();
        let children = &starting[&pane].children;
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].label.as_deref(), Some("test runner"));
        assert_eq!(children[0].status, PaneStatus::Waiting);
        assert_eq!(children[0].note.as_deref(), Some("needs permission"));
    }

    #[test]
    fn a_working_pending_parent_keeps_ownership_from_a_child() {
        let d = daemon_with_primary("/repo");
        let pane = PaneId(42);
        d.starting_agents.lock().unwrap().insert(
            pane,
            PendingStart {
                harness_session_id: Some("parent-session".to_string()),
                status: Some((PaneStatus::Working, None)),
                ..PendingStart::default()
            },
        );

        d.set_pane_session_id(pane, "child-session");

        let starting = d.starting_agents.lock().unwrap();
        let pending = &starting[&pane];
        assert_eq!(
            pending.harness_session_id.as_deref(),
            Some("parent-session")
        );
        assert_eq!(pending.children.len(), 1);
        assert_eq!(pending.children[0].session_id, "child-session");
    }

    #[test]
    fn pending_parent_lifecycle_changes_clear_children() {
        let d = daemon_with_primary("/repo");
        let pane = PaneId(42);
        d.starting_agents.lock().unwrap().insert(
            pane,
            PendingStart {
                harness_session_id: Some("parent-session".to_string()),
                ..PendingStart::default()
            },
        );
        d.report_pane_status(pane, Some("child-session"), PaneStatus::Working, None);

        d.report_pane_status(pane, Some("parent-session"), PaneStatus::Idle, None);
        assert!(d.starting_agents.lock().unwrap()[&pane].children.is_empty());

        d.report_pane_status(pane, Some("child-session"), PaneStatus::Working, None);
        d.set_pane_session_id(pane, "replacement-session");
        let starting = d.starting_agents.lock().unwrap();
        let pending = &starting[&pane];
        assert_eq!(
            pending.harness_session_id.as_deref(),
            Some("replacement-session")
        );
        assert!(pending.children.is_empty());
    }

    #[tokio::test]
    async fn startup_reports_outrank_saved_restore_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let (d, pane) = daemon_with_an_agent(dir.path()).await;
        d.restoring
            .store(true, std::sync::atomic::Ordering::Relaxed);
        d.set_pane_hook_status(pane, PaneStatus::Working, Some("new turn".to_string()));
        d.set_pane_title(pane, "current task");

        d.restore_pane_metadata(
            pane,
            &crate::store::SessionPane {
                checkout_path: dir.path().to_path_buf(),
                kind: PaneKind::Agent,
                title: "previous task".to_string(),
                template: Some("claude".to_string()),
                status: PaneStatus::NeedsReview,
                note: Some("old review".to_string()),
                harness_session_id: None,
                harness: None,
            },
        );
        d.restoring
            .store(false, std::sync::atomic::Ordering::Relaxed);

        let info = pane_info(&d, pane);
        assert_eq!(info.title, "current task");
        assert_eq!(info.status, PaneStatus::Working);
        assert_eq!(info.note.as_deref(), Some("new turn"));
        d.close_pane(pane).unwrap();
    }

    #[tokio::test]
    async fn saved_restore_metadata_does_not_resurrect_an_exited_pane() {
        let dir = tempfile::tempdir().unwrap();
        let (d, pane) = daemon_with_an_agent(dir.path()).await;
        d.mark_pane_exited(pane, Some(1));

        d.restore_pane_metadata(
            pane,
            &crate::store::SessionPane {
                checkout_path: dir.path().to_path_buf(),
                kind: PaneKind::Agent,
                title: "previous task".to_string(),
                template: Some("claude".to_string()),
                status: PaneStatus::Working,
                note: Some("old turn".to_string()),
                harness_session_id: None,
                harness: None,
            },
        );

        let info = pane_info(&d, pane);
        assert_eq!(info.status, PaneStatus::Exited { code: Some(1) });
        assert_eq!(info.note, None);
        d.close_pane(pane).unwrap();
    }
    #[test]
    fn session_identity_for_an_unknown_pane_is_ignored() {
        let d = daemon_with_primary("/repo");

        d.set_pane_session_id(PaneId(42), "session-unowned");

        assert!(d.starting_agents.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn moving_the_last_agent_moves_managed_hook_routing_too() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let d = daemon_with_two_agent_checkouts(first.path(), second.path());
        d.start_hook_server().unwrap();
        let source = d.snapshot()[0].repositories[0].checkouts[0].id;
        let pane = d.spawn_agent(source, "claude").unwrap();
        assert!(settings_of(first.path()).exists());

        d.move_agent_to_checkout(pane, second.path()).unwrap();

        assert!(!settings_of(first.path()).exists());
        assert!(settings_of(second.path()).exists());
        d.close_pane(pane).unwrap();
    }

    #[tokio::test]
    async fn a_pane_cannot_move_to_an_unknown_directory() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let unknown = tempfile::tempdir().unwrap();
        let d = daemon_with_two_agent_checkouts(first.path(), second.path());
        let source = d.snapshot()[0].repositories[0].checkouts[0].id;
        let pane = d.spawn_agent(source, "claude").unwrap();

        assert!(d.move_agent_to_checkout(pane, unknown.path()).is_err());
        assert_eq!(
            d.snapshot()[0].repositories[0].checkouts[0].panes[0].id,
            pane
        );
        d.close_pane(pane).unwrap();
    }

    // --- worktree reconciliation -------------------------------------------

    #[test]
    fn snapshot_keeps_configured_repositories_separate() {
        let d = daemon_with_repositories(&["/first", "/second"]);

        let tree = d.snapshot();
        assert_eq!(tree[0].repositories.len(), 2);
        assert_eq!(tree[0].repositories[0].name, "first");
        assert_eq!(tree[0].repositories[0].checkouts.len(), 1);
        assert_eq!(tree[0].repositories[0].checkouts[0].path, "/first");
        assert_eq!(tree[0].repositories[1].name, "second");
        assert_eq!(tree[0].repositories[1].checkouts.len(), 1);
        assert_eq!(tree[0].repositories[1].checkouts[0].path, "/second");
    }

    #[test]
    fn reconciliation_is_isolated_per_repository() {
        let d = daemon_with_repositories(&["/first", "/second"]);
        d.reconcile_worktrees_with(|primary| match primary.to_string_lossy().as_ref() {
            "/first" => listing(&["/first", "/first/wt-a"]),
            "/second" => listing(&["/second", "/second/wt-b"]),
            _ => Vec::new(),
        });

        let tree = d.snapshot();
        let first = &tree[0].repositories[0].checkouts;
        let second = &tree[0].repositories[1].checkouts;
        assert_eq!(first.len(), 2);
        assert!(first.iter().any(|c| c.path == "/first/wt-a"));
        assert!(!first.iter().any(|c| c.path == "/second/wt-b"));
        assert_eq!(second.len(), 2);
        assert!(second.iter().any(|c| c.path == "/second/wt-b"));

        d.reconcile_worktrees_with(|primary| match primary.to_string_lossy().as_ref() {
            "/first" => listing(&["/first"]),
            "/second" => listing(&["/second", "/second/wt-b"]),
            _ => Vec::new(),
        });
        let tree = d.snapshot();
        assert_eq!(tree[0].repositories[0].checkouts.len(), 1);
        assert_eq!(tree[0].repositories[1].checkouts.len(), 2);
    }

    #[test]
    fn reconcile_adds_a_worktree_created_outside_argus() {
        let d = daemon_with_primary("/repo");
        d.reconcile_worktrees_with(|_| listing(&["/repo", "/repo/.argus/worktrees/feat"]));

        let paths = checkout_paths(&d);
        assert_eq!(
            paths.len(),
            2,
            "discovered worktree should join the tree: {paths:?}"
        );
        assert!(paths.iter().any(|p| p.ends_with("feat")));
    }

    #[test]
    fn discovered_worktree_is_not_primary_and_primary_stays_primary() {
        let d = daemon_with_primary("/repo");
        d.reconcile_worktrees_with(|_| listing(&["/repo", "/repo/wt"]));

        let checkouts: Vec<_> = d
            .snapshot()
            .into_iter()
            .flat_map(|p| p.repositories)
            .flat_map(|r| r.checkouts)
            .collect();
        let primary = checkouts.iter().find(|c| c.path == "/repo").unwrap();
        let linked = checkouts.iter().find(|c| c.path == "/repo/wt").unwrap();
        assert!(primary.primary, "the configured checkout stays primary");
        assert!(
            !linked.primary,
            "a discovered worktree is removable, not primary"
        );
    }

    #[test]
    fn reconcile_is_idempotent() {
        let d = daemon_with_primary("/repo");
        for _ in 0..3 {
            d.reconcile_worktrees_with(|_| listing(&["/repo", "/repo/wt"]));
        }
        assert_eq!(
            checkout_paths(&d).len(),
            2,
            "repeated ticks must not duplicate rows"
        );
    }

    #[test]
    fn reconcile_drops_a_worktree_removed_outside_argus() {
        let d = daemon_with_primary("/repo");
        d.reconcile_worktrees_with(|_| listing(&["/repo", "/repo/wt"]));
        assert_eq!(checkout_paths(&d).len(), 2);

        d.reconcile_worktrees_with(|_| listing(&["/repo"]));
        assert_eq!(checkout_paths(&d), vec!["/repo".to_string()]);
    }

    #[test]
    fn an_empty_listing_never_wipes_the_tree() {
        // `git::list_worktrees` returns empty when the path isn't a repo or
        // the `git` binary is missing — that must mean "nothing to
        // reconcile", never "every worktree was removed".
        let d = daemon_with_primary("/repo");
        d.reconcile_worktrees_with(|_| listing(&["/repo", "/repo/wt"]));

        d.reconcile_worktrees_with(|_| Vec::new());
        assert_eq!(checkout_paths(&d).len(), 2, "empty listing must be a no-op");
    }

    #[test]
    fn reconcile_never_removes_the_primary_checkout() {
        // Even if git somehow stops listing it — a moved/renamed repo dir —
        // the configured checkout is the user's, not ours to drop.
        let d = daemon_with_primary("/repo");
        d.reconcile_worktrees_with(|_| listing(&["/somewhere/else"]));
        assert!(
            checkout_paths(&d).contains(&"/repo".to_string()),
            "primary must survive a listing that omits it"
        );
    }

    #[test]
    fn discovered_checkouts_get_distinct_ids() {
        let d = daemon_with_primary("/repo");
        d.reconcile_worktrees_with(|_| listing(&["/repo", "/repo/a", "/repo/b"]));

        let ids: Vec<_> = d
            .snapshot()
            .into_iter()
            .flat_map(|p| p.repositories)
            .flat_map(|r| r.checkouts)
            .map(|c| c.id)
            .collect();
        let mut uniq = ids.clone();
        uniq.sort_by_key(|i| i.0);
        uniq.dedup();
        assert_eq!(uniq.len(), ids.len(), "ids must be unique: {ids:?}");
    }

    // --- display naming ----------------------------------------------------

    #[test]
    fn worktree_display_name_falls_back_to_the_directory_name() {
        // Non-repo path: no branch to read, so the leaf directory names it.
        assert_eq!(
            worktree_display_name(std::path::Path::new("/repo/wt/feat-x"), false),
            "feat-x"
        );
        assert_eq!(
            worktree_display_name(std::path::Path::new("/repo"), true),
            "repo"
        );
    }

    fn status_on(branch: &str) -> GitStatus {
        GitStatus {
            branch: Some(branch.to_string()),
            dirty: false,
            changed_files: 0,
            ahead: 0,
            behind: 0,
        }
    }

    /// The bug this guards: `git switch` in another terminal rewrites HEAD,
    /// and a status read landing in that window used to come back as a
    /// checkout on no branch. That threw the row's name back to the
    /// directory the worktree was created as and left the branch it was
    /// really on looking free, so the column rearranged itself under the
    /// user for a tick.
    #[test]
    fn a_status_read_that_failed_does_not_erase_the_branch_we_knew() {
        let d = daemon_with_primary("/repo");
        d.reconcile_worktrees_with(|_| listing(&["/repo", "/repo/wt"]));
        d.refresh_git_status_with(|path| {
            Some(status_on(if path.ends_with("wt") { "dev" } else { "main" }))
        });

        let worktree = d.snapshot()[0].repositories[0]
            .checkouts
            .iter()
            .find(|c| !c.primary)
            .map(|c| c.id)
            .unwrap();
        assert_eq!(
            d.snapshot()[0].repositories[0]
                .checkouts
                .iter()
                .find(|c| c.id == worktree)
                .unwrap()
                .name,
            "dev"
        );

        // git is mid-switch and cannot answer.
        d.refresh_git_status_with(|_| None);

        let c = d.snapshot()[0].repositories[0]
            .checkouts
            .iter()
            .find(|c| c.id == worktree)
            .unwrap()
            .clone();
        assert_eq!(
            c.name, "dev",
            "the row must not rename itself on a failed read"
        );
        assert_eq!(
            c.git.and_then(|g| g.branch),
            Some("dev".to_string()),
            "and must still count as the occupant of the branch"
        );
    }

    /// A switch made outside Argus is news, not a failure: the row follows
    /// the branch that now occupies it.
    #[test]
    fn a_branch_switched_outside_argus_renames_the_row_it_happened_in() {
        let d = daemon_with_primary("/repo");
        d.reconcile_worktrees_with(|_| listing(&["/repo", "/repo/wt"]));
        d.refresh_git_status_with(|path| {
            Some(status_on(if path.ends_with("wt") { "dev" } else { "main" }))
        });
        d.refresh_git_status_with(|path| {
            Some(status_on(if path.ends_with("wt") {
                "spike"
            } else {
                "main"
            }))
        });

        let names: Vec<String> = d.snapshot()[0].repositories[0]
            .checkouts
            .iter()
            .map(|c| c.name.clone())
            .collect();
        assert!(
            names.contains(&"spike".to_string()) && !names.contains(&"dev".to_string()),
            "the row should follow the branch: {names:?}"
        );
    }

    // --- tree broadcast ----------------------------------------------------

    #[test]
    fn reconcile_result_is_visible_to_tree_subscribers() {
        let d = daemon_with_primary("/repo");
        let mut rx = d.subscribe_tree();
        d.reconcile_worktrees_with(|_| listing(&["/repo", "/repo/wt"]));
        d.broadcast_tree();

        let tree = rx
            .try_recv()
            .expect("a tree snapshot should have been broadcast");
        assert_eq!(tree[0].repositories[0].checkouts.len(), 2);
    }

    #[test]
    fn default_agent_templates_are_offered_when_config_has_none() {
        let d = daemon_with_primary("/repo");
        assert_eq!(
            d.template_names(),
            vec!["claude", "codex", "opencode", "agy", "agent"]
        );
    }

    #[test]
    fn every_built_in_template_gets_a_harness_that_can_report() {
        // Regression: `opencode` shipped as a template with no harness of
        // the same name, so it fell through to `generic` — which installs
        // nothing — and its rows never left Idle however hard it worked.
        let d = daemon_with_primary("/repo");
        for name in d.template_names() {
            let template = AgentConfig {
                name: name.clone(),
                cmd: vec![name.clone()],
                env: Default::default(),
                harness: None,
                restart: Default::default(),
            };
            let h = d.harness_for(&template);
            assert_ne!(
                h.name, "generic",
                "{name} has no harness, so its pane can never report"
            );
        }
    }

    #[test]
    fn gen_token_is_not_a_fixed_string() {
        assert_eq!(gen_token().len(), 32);
        assert_ne!(gen_token(), gen_token());
    }

    // --- managed hook lifecycle ---------------------------------------------

    /// A daemon whose single checkout is `dir`, with a "claude" template
    /// that is really just `echo` — enough to exercise the hook-install path
    /// (which keys off the template *name*) without launching a real agent.
    fn daemon_with_fake_claude(dir: &std::path::Path) -> Arc<Daemon> {
        Daemon::new(fake_claude_config(dir))
    }

    fn fake_claude_config(dir: &std::path::Path) -> ConfigFile {
        ConfigFile {
            workspaces: Vec::new(),
            projects: vec![ProjectConfig {
                name: "proj".to_string(),
                root: None,
                repos: vec![dir.to_string_lossy().to_string()],
                workspace: None,
                ..Default::default()
            }],
            agents: vec![AgentConfig {
                name: "claude".to_string(),
                cmd: vec!["echo".to_string(), "hi".to_string()],
                env: Default::default(),
                harness: None,
                restart: Default::default(),
            }],
            harnesses: Vec::new(),
        }
    }

    /// The same fake agent, in a project that allows one per checkout.
    fn daemon_with_an_exclusive_project(dir: &std::path::Path) -> Arc<Daemon> {
        Daemon::new(ConfigFile {
            workspaces: Vec::new(),
            projects: vec![ProjectConfig {
                name: "proj".to_string(),
                repos: vec![dir.to_string_lossy().to_string()],
                exclusive: true,
                ..Default::default()
            }],
            agents: vec![AgentConfig {
                name: "claude".to_string(),
                cmd: vec!["echo".to_string(), "hi".to_string()],
                env: Default::default(),
                harness: None,
                restart: Default::default(),
            }],
            harnesses: Vec::new(),
        })
    }

    #[tokio::test]
    async fn sharing_a_checkout_is_allowed_unless_the_project_says_otherwise() {
        let dir = tempfile::tempdir().unwrap();
        let d = daemon_with_fake_claude(dir.path());
        let checkout = only_checkout(&d);

        let first = d.spawn_agent(checkout, "claude").unwrap();
        let second = d.spawn_agent(checkout, "claude").unwrap();

        assert_eq!(
            d.snapshot()[0].repositories[0].checkouts[0].panes.len(),
            2,
            "two agents in one checkout is shown, not refused"
        );
        let _ = d.close_pane(first);
        let _ = d.close_pane(second);
    }

    #[tokio::test]
    async fn an_exclusive_project_refuses_a_second_agent_in_one_checkout() {
        let dir = tempfile::tempdir().unwrap();
        let d = daemon_with_an_exclusive_project(dir.path());
        let checkout = only_checkout(&d);
        let first = d.spawn_agent(checkout, "claude").unwrap();

        let err = d.spawn_agent(checkout, "claude").unwrap_err().to_string();

        assert!(err.contains("worktree"), "say what to do instead: {err:?}");
        assert_eq!(d.snapshot()[0].repositories[0].checkouts[0].panes.len(), 1);
        let _ = d.close_pane(first);
    }

    #[tokio::test]
    async fn exclusivity_is_about_agents_not_shells() {
        let dir = tempfile::tempdir().unwrap();
        let d = daemon_with_an_exclusive_project(dir.path());
        let checkout = only_checkout(&d);
        let agent = d.spawn_agent(checkout, "claude").unwrap();

        let shell = d.spawn_shell(checkout).expect("a shell is not an agent");

        let _ = d.close_pane(shell);
        let _ = d.close_pane(agent);
    }

    #[tokio::test]
    async fn an_exclusive_checkout_takes_an_agent_again_once_the_first_is_gone() {
        let dir = tempfile::tempdir().unwrap();
        let d = daemon_with_an_exclusive_project(dir.path());
        let checkout = only_checkout(&d);
        let first = d.spawn_agent(checkout, "claude").unwrap();
        d.close_pane(first).unwrap();

        let second = d.spawn_agent(checkout, "claude").unwrap();

        let _ = d.close_pane(second);
    }

    fn review_anchor(line: u32) -> ReviewAnchor {
        ReviewAnchor {
            commit: None,
            base: ReviewBase::Unstaged,
            path: "src/main.rs".to_string(),
            old_path: None,
            old_start: None,
            old_end: None,
            new_start: Some(line),
            new_end: Some(line),
            text: vec!["+changed".to_string()],
        }
    }

    #[tokio::test]
    async fn a_review_comment_is_saved_before_it_is_sent_to_an_agent() {
        let dir = tempfile::tempdir().unwrap();
        let d = daemon_with_claude_aliases(dir.path(), &["claude"]);
        let checkout = only_checkout(&d);
        let agent = d.spawn_agent(checkout, "claude").unwrap();

        let (id, delivered) = d
            .submit_review_comment(checkout, agent, review_anchor(8), "fix this".to_string())
            .unwrap();

        assert!(delivered);
        let comments = d.review_comments_for_agent(agent).unwrap();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].id, id);
        assert_eq!(comments[0].body, "fix this");
        close_all(&d);
    }

    #[tokio::test]
    async fn review_comments_require_a_live_agent_in_the_reviewed_checkout() {
        let dir = tempfile::tempdir().unwrap();
        let d = daemon_with_claude_aliases(dir.path(), &["claude"]);
        let checkout = only_checkout(&d);
        let shell = d.spawn_shell(checkout).unwrap();

        assert!(d
            .submit_review_comment(checkout, shell, review_anchor(1), "fix".to_string())
            .is_err());
        assert!(d.review_comments_for_agent(shell).is_err());
        close_all(&d);
    }

    async fn post_agent_hook(
        d: &Arc<Daemon>,
        source: PaneId,
        endpoint: argus_protocol::Endpoint,
        body: &str,
    ) -> Vec<u8> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let request = format!(
            "POST {} HTTP/1.1\r\nAuthorization: Bearer {}\r\nContent-Length: {}\r\n\r\n{}",
            argus_protocol::pane_path(source, endpoint),
            d.hook_token,
            body.len(),
            body
        );
        let port = d.hook_port.load(std::sync::atomic::Ordering::Relaxed);
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        response
    }

    #[tokio::test]
    async fn an_authorized_comments_hook_returns_checkout_feedback() {
        let dir = tempfile::tempdir().unwrap();
        let d = daemon_with_claude_aliases(dir.path(), &["claude"]);
        d.start_hook_server().unwrap();
        let checkout = only_checkout(&d);
        let source = d.spawn_agent(checkout, "claude").unwrap();
        d.submit_review_comment(
            checkout,
            source,
            review_anchor(6),
            "consider this".to_string(),
        )
        .unwrap();

        let response = post_agent_hook(&d, source, Endpoint::Comments, "").await;

        assert!(response.starts_with(b"HTTP/1.1 200 OK"));
        let body = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|index| &response[index + 4..])
            .unwrap();
        let comments: Vec<argus_protocol::ReviewComment> = serde_json::from_slice(body).unwrap();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].body, "consider this");
        close_all(&d);
    }

    #[tokio::test]
    async fn hook_rejects_a_body_over_the_shared_limit() {
        let dir = tempfile::tempdir().unwrap();
        let d = daemon_with_claude_aliases(dir.path(), &["claude"]);
        d.start_hook_server().unwrap();
        let source = d.spawn_agent(only_checkout(&d), "claude").unwrap();

        let response = post_agent_hook(&d, source, Endpoint::Title, &"x".repeat(4097)).await;

        assert!(response.starts_with(b"HTTP/1.1 413 Content Too Large"));
        close_all(&d);
    }

    fn settings_of(dir: &std::path::Path) -> std::path::PathBuf {
        dir.join(".claude").join("settings.local.json")
    }

    fn only_checkout(d: &Daemon) -> CheckoutId {
        d.snapshot()[0].repositories[0].checkouts[0].id
    }

    #[test]
    fn startup_sweeps_hooks_left_by_a_previous_daemon() {
        // Regression: a daemon's ephemeral port dies with it, so hooks left
        // in a checkout fire against nobody — and break every later agent
        // run in that directory, Argus-managed or not.
        let dir = tempfile::tempdir().unwrap();
        crate::harness::Harness::claude()
            .install(dir.path(), PaneId(4), 65140, "old")
            .unwrap();
        assert!(settings_of(dir.path()).exists());

        let d = daemon_with_fake_claude(dir.path());
        d.sweep_stale_hooks();
        assert!(
            !settings_of(dir.path()).exists(),
            "a previous boot's hooks must not survive startup"
        );
    }

    #[test]
    fn sweeping_a_checkout_that_never_hosted_an_agent_is_harmless() {
        let dir = tempfile::tempdir().unwrap();
        let d = daemon_with_fake_claude(dir.path());
        d.sweep_stale_hooks();
        assert!(
            !dir.path().join(".claude").exists(),
            "must not create anything"
        );
    }

    #[tokio::test]
    async fn closing_the_last_agent_pane_takes_its_hooks_out() {
        let dir = tempfile::tempdir().unwrap();
        let d = daemon_with_fake_claude(dir.path());
        d.start_hook_server().unwrap();

        let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();
        assert!(settings_of(dir.path()).exists(), "spawning installs hooks");

        d.close_pane(pane).unwrap();
        assert!(
            !settings_of(dir.path()).exists(),
            "the last agent leaving takes the hooks with it"
        );
    }

    #[tokio::test]
    async fn closing_one_of_two_agent_panes_leaves_the_hooks_alone() {
        // Hooks belong to the checkout, not the pane — pulling them while a
        // second agent is still running there would blind it.
        let dir = tempfile::tempdir().unwrap();
        let d = daemon_with_fake_claude(dir.path());
        d.start_hook_server().unwrap();
        let checkout = only_checkout(&d);

        let first = d.spawn_agent(checkout, "claude").unwrap();
        let _second = d.spawn_agent(checkout, "claude").unwrap();

        d.close_pane(first).unwrap();
        assert!(
            settings_of(dir.path()).exists(),
            "the surviving agent still needs its status hooks"
        );
    }

    #[tokio::test]
    async fn closing_a_shell_pane_does_not_disturb_an_agents_hooks() {
        let dir = tempfile::tempdir().unwrap();
        let d = daemon_with_fake_claude(dir.path());
        d.start_hook_server().unwrap();
        let checkout = only_checkout(&d);

        let _agent = d.spawn_agent(checkout, "claude").unwrap();
        let shell = d.spawn_shell(checkout).unwrap();

        d.close_pane(shell).unwrap();
        assert!(settings_of(dir.path()).exists());
    }

    // --- reconciliation against a real repo ---------------------------------

    /// Builds a real repo with one commit, so `git::list_worktrees` has
    /// something truthful to return. Mirrors `git::tests::repo_with_a_commit`.
    fn real_repo(dir: &std::path::Path) -> git2::Repository {
        let repo = git2::Repository::init(dir).unwrap();
        std::fs::write(dir.join("a.txt"), "hello").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("a.txt")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = git2::Signature::now("t", "t@example.com").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
        drop(tree);
        repo
    }

    #[test]
    fn reconciling_a_real_repo_does_not_duplicate_the_primary_checkout() {
        // The listing and the configured path are produced by different
        // things — libgit2's workdir (canonicalized, trailing separator,
        // native separators) versus whatever the user wrote in the config.
        // If those two ever stop comparing equal, every poll tick decides
        // the primary is a newly-discovered worktree and adds another row.
        let dir = tempfile::tempdir().unwrap();
        let _repo = real_repo(dir.path());
        // Deliberately configured with forward slashes, the way the config
        // file and `add_project` write them on Windows.
        let configured = dir.path().to_string_lossy().replace('\\', "/");

        let d = Daemon::new(ConfigFile {
            workspaces: Vec::new(),
            projects: vec![ProjectConfig {
                name: "proj".to_string(),
                root: None,
                repos: vec![configured],
                workspace: None,
                ..Default::default()
            }],
            agents: Vec::new(),
            harnesses: Vec::new(),
        });

        for _ in 0..3 {
            d.reconcile_worktrees();
        }
        let checkouts = &d.snapshot()[0].repositories[0].checkouts;
        assert_eq!(
            checkouts.len(),
            1,
            "the primary must match its own listing, not clone itself: {:?}",
            checkouts.iter().map(|c| &c.path).collect::<Vec<_>>()
        );
        assert!(checkouts[0].primary);
    }

    #[test]
    fn a_worktree_made_outside_argus_is_discovered_against_a_real_repo() {
        let dir = tempfile::tempdir().unwrap();
        let repo = real_repo(dir.path());

        let d = Daemon::new(ConfigFile {
            workspaces: Vec::new(),
            projects: vec![ProjectConfig {
                name: "proj".to_string(),
                root: None,
                repos: vec![dir.path().to_string_lossy().to_string()],
                workspace: None,
                ..Default::default()
            }],
            agents: Vec::new(),
            harnesses: Vec::new(),
        });
        d.reconcile_worktrees();
        assert_eq!(d.snapshot()[0].repositories[0].checkouts.len(), 1);

        // Someone runs `git worktree add` in a shell.
        repo.worktree("feature", &dir.path().join("wt-feature"), None)
            .unwrap();

        d.reconcile_worktrees();
        let checkouts = &d.snapshot()[0].repositories[0].checkouts;
        assert_eq!(
            checkouts.len(),
            2,
            "the new worktree should appear: {checkouts:?}"
        );
        assert!(
            checkouts.iter().any(|c| !c.primary),
            "and be removable, not marked primary"
        );
    }

    // --- project roots ------------------------------------------------------

    /// A project that finds its repositories under `root`, with `repos`
    /// naming any that are also written down outright.
    fn daemon_rooted_at(root: &std::path::Path, repos: &[&str]) -> Arc<Daemon> {
        Daemon::new(ConfigFile {
            workspaces: Vec::new(),
            projects: vec![ProjectConfig {
                name: "proj".to_string(),
                root: Some(root.to_string_lossy().to_string()),
                repos: repos.iter().map(|p| p.to_string()).collect(),
                workspace: None,
                ..Default::default()
            }],
            agents: Vec::new(),
            harnesses: Vec::new(),
        })
    }

    /// Whether `projects.toml` declares any project, ignoring the
    /// commented-out examples the default config ships with — which is what
    /// a bare `contains` would count.
    fn declares_a_project(cfg: &std::path::Path) -> bool {
        declares(cfg, "[[project]]")
    }

    fn declares_a_workspace(cfg: &std::path::Path) -> bool {
        declares(cfg, "[[workspace]]")
    }

    fn declares(cfg: &std::path::Path, header: &str) -> bool {
        std::fs::read_to_string(cfg.join("projects.toml"))
            .unwrap_or_default()
            .lines()
            .any(|line| line.trim_start().starts_with(header))
    }

    fn repository_names(d: &Daemon) -> Vec<String> {
        d.snapshot()
            .into_iter()
            .flat_map(|p| p.repositories)
            .map(|r| r.name)
            .collect()
    }

    #[test]
    fn a_root_brings_in_every_repository_under_it() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["orion", "notes"] {
            let child = dir.path().join(name);
            std::fs::create_dir(&child).unwrap();
            let _repo = real_repo(&child);
        }

        let d = daemon_rooted_at(dir.path(), &[]);
        assert_eq!(repository_names(&d), vec!["notes", "orion"]);
    }

    #[test]
    fn repositories_written_down_outright_still_mean_exactly_what_they_did() {
        // The schema every existing config is written in. A path here is
        // taken at its word — this one is not a Git repository at all, and
        // still has to be a row with a checkout to open panes in.
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("scratch");
        std::fs::create_dir(&plain).unwrap();

        let d = daemon_with_repositories(&[&plain.to_string_lossy()]);
        assert_eq!(repository_names(&d), vec!["scratch"]);
        assert_eq!(checkout_paths(&d).len(), 1);
    }

    #[test]
    fn a_root_and_the_repositories_named_outright_combine_without_duplicating() {
        // The same repository reached both ways is one row, and the row is
        // the explicit one, so a scan can never take it away.
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join("orion");
        std::fs::create_dir(&shared).unwrap();
        let _repo = real_repo(&shared);
        let outside = tempfile::tempdir().unwrap();
        let _elsewhere = real_repo(outside.path());

        let d = daemon_rooted_at(
            dir.path(),
            &[&shared.to_string_lossy(), &outside.path().to_string_lossy()],
        );

        // Order is part of the contract: what the config names comes first,
        // in the order it names it, and what a scan turns up follows.
        assert_eq!(
            repository_names(&d),
            vec![
                "orion".to_string(),
                outside
                    .path()
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string(),
            ]
        );
    }

    #[test]
    fn a_repository_cloned_into_a_root_arrives_on_the_next_scan() {
        // The reason the root is remembered at all rather than resolved once
        // and thrown away.
        let dir = tempfile::tempdir().unwrap();
        let d = daemon_rooted_at(dir.path(), &[]);
        assert!(repository_names(&d).is_empty(), "nothing there yet");

        let cloned = dir.path().join("orion");
        assert!(
            d.reconcile_repositories_with(|_, _| listing(&[&cloned.to_string_lossy()])),
            "the tree changed, so clients need telling"
        );

        assert_eq!(repository_names(&d), vec!["orion"]);
    }

    #[test]
    fn an_empty_root_is_a_project_in_its_own_right() {
        // Pressing `n` on a directory you are about to clone into should
        // leave you with the project, not with an error.
        let dir = tempfile::tempdir().unwrap();
        let d = daemon_rooted_at(dir.path(), &[]);

        let projects = d.snapshot();
        assert_eq!(projects.len(), 1);
        assert!(projects[0].repositories.is_empty());
    }

    #[tokio::test]
    async fn a_scan_leaves_the_repositories_it_already_knew_about_alone() {
        // Ids reach clients as selection state and reach panes as their
        // place in the tree. Rebuilding a repository that merely turned up
        // in a scan again would move the user's cursor and orphan its panes.
        let dir = tempfile::tempdir().unwrap();
        let child = dir.path().join("orion");
        std::fs::create_dir(&child).unwrap();
        let _repo = real_repo(&child);

        let d = daemon_rooted_at(dir.path(), &[]);
        let before = d.snapshot();
        let repository = before[0].repositories[0].clone();
        let pane = d.spawn_shell(repository.checkouts[0].id).unwrap();

        assert!(
            !d.reconcile_repositories_with(|_, _| listing(&[&child.to_string_lossy()])),
            "nothing changed, so nothing should be broadcast"
        );

        let after = &d.snapshot()[0].repositories[0];
        assert_eq!(after.id, repository.id);
        assert_eq!(after.checkouts[0].id, repository.checkouts[0].id);
        assert_eq!(after.checkouts[0].panes.len(), 1);

        let _ = d.close_pane(pane);
    }

    #[test]
    fn a_discovered_repository_that_leaves_the_root_leaves_the_project() {
        let dir = tempfile::tempdir().unwrap();
        let child = dir.path().join("orion");
        std::fs::create_dir(&child).unwrap();
        let _repo = real_repo(&child);
        let d = daemon_rooted_at(dir.path(), &[]);

        assert!(d.reconcile_repositories_with(|_, _| Vec::new()));
        assert!(repository_names(&d).is_empty());
    }

    #[tokio::test]
    async fn a_repository_holding_panes_survives_a_scan_that_cannot_find_it() {
        // A directory can go missing for reasons that have nothing to do
        // with the user's intent. Killing a running agent over it is not a
        // trade worth making, so the row waits until it is empty.
        let dir = tempfile::tempdir().unwrap();
        let child = dir.path().join("orion");
        std::fs::create_dir(&child).unwrap();
        let _repo = real_repo(&child);
        let d = daemon_rooted_at(dir.path(), &[]);
        let pane = d
            .spawn_shell(d.snapshot()[0].repositories[0].checkouts[0].id)
            .unwrap();

        assert!(!d.reconcile_repositories_with(|_, _| Vec::new()));
        assert_eq!(
            repository_names(&d),
            vec!["orion"],
            "still there, with its pane"
        );

        d.close_pane(pane).unwrap();
        assert!(d.reconcile_repositories_with(|_, _| Vec::new()));
        assert!(repository_names(&d).is_empty(), "and gone once it is empty");
    }

    #[test]
    fn a_repository_named_outright_survives_a_scan_that_cannot_find_it() {
        // Explicit configuration is the user speaking. A scan of the root
        // has no standing to contradict it — and it may not be a Git
        // repository for a scan to find in the first place.
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("scratch");
        std::fs::create_dir(&plain).unwrap();

        let d = daemon_rooted_at(dir.path(), &[&plain.to_string_lossy()]);
        assert!(!d.reconcile_repositories_with(|_, _| Vec::new()));
        assert_eq!(repository_names(&d), vec!["scratch"]);
    }

    #[test]
    fn a_project_without_a_root_is_never_scanned() {
        let d = daemon_with_repositories(&["/configured"]);
        assert!(
            !d.reconcile_repositories_with(|_, _| panic!("a rootless project has nothing to scan")),
            "and nothing changed"
        );
    }

    #[test]
    fn adding_a_directory_of_repositories_adds_every_repository_under_it() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["orion", "notes"] {
            let child = dir.path().join(name);
            std::fs::create_dir(&child).unwrap();
            let _repo = real_repo(&child);
        }

        with_temp_config(|_| {
            let d = Daemon::new(ConfigFile::default());
            d.add_project(&dir.path().to_string_lossy()).unwrap();
            assert_eq!(repository_names(&d), vec!["notes", "orion"]);
        });
    }

    #[test]
    fn adding_a_repository_adds_that_one_repository() {
        // The oldest meaning of `n`, and the one that must not change.
        let dir = tempfile::tempdir().unwrap();
        let _repo = real_repo(dir.path());

        with_temp_config(|_| {
            let d = Daemon::new(ConfigFile::default());
            d.add_project(&dir.path().to_string_lossy()).unwrap();

            let name = dir
                .path()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string();
            assert_eq!(repository_names(&d), vec![name]);
        });
    }

    #[test]
    fn an_added_project_persists_the_root_it_was_given() {
        // Not the repositories found under it: writing those down would
        // freeze the project as it looked the day it was added.
        let dir = tempfile::tempdir().unwrap();
        let child = dir.path().join("orion");
        std::fs::create_dir(&child).unwrap();
        let _repo = real_repo(&child);

        with_temp_config(|_| {
            let d = persistent(ConfigFile::default());
            d.add_project(&dir.path().to_string_lossy()).unwrap();

            let recorded = crate::store::Store::open().unwrap().overlays().unwrap();
            assert_eq!(recorded.projects.len(), 1);
            let (project, repos) = &recorded.projects[0];
            assert_eq!(
                project.root,
                dir.path(),
                "the root is what gets scanned again next time"
            );
            assert!(
                repos.is_empty(),
                "and what it found is not frozen alongside it: {repos:?}"
            );
        });
    }

    #[test]
    fn a_project_added_at_runtime_comes_back_the_same_after_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let child = dir.path().join("orion");
        std::fs::create_dir(&child).unwrap();
        let _repo = real_repo(&child);

        with_temp_config(|_| {
            let d = persistent(ConfigFile::default());
            d.add_project(&dir.path().to_string_lossy()).unwrap();
            let before = repository_names(&d);

            let restarted = persistent(crate::config::load().unwrap());
            assert_eq!(repository_names(&restarted), before);
        });
    }

    // --- adding one repository to a project ---------------------------------

    /// A project rooted at `dir`, added the way the TUI adds it, plus a
    /// repository sitting somewhere the root will never scan.
    fn project_and_an_outside_repository() -> (tempfile::TempDir, tempfile::TempDir, Arc<Daemon>) {
        let root = tempfile::tempdir().unwrap();
        let child = root.path().join("orion");
        std::fs::create_dir(&child).unwrap();
        let _repo = real_repo(&child);

        let outside = tempfile::tempdir().unwrap();
        let elsewhere = outside.path().join("notes");
        std::fs::create_dir(&elsewhere).unwrap();
        let _other = real_repo(&elsewhere);

        let d = persistent(ConfigFile::default());
        d.add_project(&root.path().to_string_lossy()).unwrap();
        (root, outside, d)
    }

    #[test]
    fn a_repository_can_be_added_to_a_project_from_outside_its_root() {
        with_temp_config(|_| {
            let (_root, outside, d) = project_and_an_outside_repository();
            let project = d.snapshot()[0].id;

            d.add_repository(project, &outside.path().join("notes").to_string_lossy())
                .unwrap();

            assert_eq!(repository_names(&d), vec!["orion", "notes"]);
        });
    }

    #[test]
    fn a_repository_added_by_path_is_still_there_after_a_restart() {
        with_temp_config(|_| {
            let (_root, outside, d) = project_and_an_outside_repository();
            let project = d.snapshot()[0].id;
            d.add_repository(project, &outside.path().join("notes").to_string_lossy())
                .unwrap();

            // Named repositories are built before the root is scanned, so
            // the row order changes across a restart even though the set
            // does not.
            let restarted = persistent(crate::config::load().unwrap());
            assert_eq!(repository_names(&restarted), vec!["notes", "orion"]);
        });
    }

    #[test]
    fn a_repository_named_by_hand_is_not_a_scan_result_and_no_scan_removes_it() {
        with_temp_config(|_| {
            let (_root, outside, d) = project_and_an_outside_repository();
            let project = d.snapshot()[0].id;
            d.add_repository(project, &outside.path().join("notes").to_string_lossy())
                .unwrap();

            // A scan of the root finds only what is under it, which the
            // added repository never was.
            assert!(!d.reconcile_repositories_with(crate::git::discover_repositories_within));
            assert_eq!(repository_names(&d), vec!["orion", "notes"]);
        });
    }

    #[test]
    fn adding_a_repository_the_project_already_has_is_refused() {
        with_temp_config(|_| {
            let (root, _outside, d) = project_and_an_outside_repository();
            let project = d.snapshot()[0].id;

            let err = d
                .add_repository(project, &root.path().join("orion").to_string_lossy())
                .unwrap_err()
                .to_string();
            assert!(err.contains("already has"), "{err}");
            assert_eq!(repository_names(&d), vec!["orion"]);
        });
    }

    #[test]
    fn adding_something_that_is_not_a_directory_is_refused() {
        with_temp_config(|_| {
            let (root, _outside, d) = project_and_an_outside_repository();
            let project = d.snapshot()[0].id;

            let err = d
                .add_repository(project, &root.path().join("nope").to_string_lossy())
                .unwrap_err()
                .to_string();
            assert!(err.contains("not a directory"), "{err}");
        });
    }

    #[test]
    fn adding_a_repository_back_undoes_having_removed_it() {
        with_temp_config(|_| {
            let (root, _outside, d) = project_and_an_outside_repository();
            let project = d.snapshot()[0].id;
            let repository = d.snapshot()[0].repositories[0].id;

            d.remove_repository(repository).unwrap();
            assert!(repository_names(&d).is_empty());

            d.add_repository(project, &root.path().join("orion").to_string_lossy())
                .unwrap();
            assert_eq!(repository_names(&d), vec!["orion"]);

            // The exclusion is gone too, or a restart would drop it again.
            let restarted = persistent(crate::config::load().unwrap());
            assert_eq!(repository_names(&restarted), vec!["orion"]);
        });
    }

    // --- removing what was added --------------------------------------------

    /// A project rooted at a temp directory holding one repository per
    /// name, added through `add_project` so it is written to the config the
    /// way the TUI writes it.
    /// A daemon backed by the store in the temp config directory. What
    /// every test about surviving a restart needs: `Daemon::new` hands out
    /// a store that dies with the process, which is exactly what makes it
    /// safe everywhere else.
    fn persistent(config: ConfigFile) -> Arc<Daemon> {
        Daemon::with_store(config, crate::store::Store::open().unwrap())
    }

    fn added_project_with(names: &[&str]) -> (tempfile::TempDir, Arc<Daemon>) {
        let dir = tempfile::tempdir().unwrap();
        for name in names {
            let child = dir.path().join(name);
            std::fs::create_dir(&child).unwrap();
            let _repo = real_repo(&child);
        }
        let d = persistent(ConfigFile::default());
        d.add_project(&dir.path().to_string_lossy()).unwrap();
        (dir, d)
    }

    #[test]
    fn a_removed_project_leaves_the_tree_the_config_and_the_disk_alone() {
        with_temp_config(|cfg| {
            let (dir, d) = added_project_with(&["orion"]);
            let project = d.snapshot()[0].id;

            d.remove_project(project).unwrap();

            assert!(d.snapshot().is_empty(), "gone from the tree");
            assert!(
                persistent(crate::config::load().unwrap())
                    .snapshot()
                    .is_empty(),
                "and gone for good, not just for this run"
            );
            assert!(
                !declares_a_project(cfg),
                "without Argus having written to the user's config"
            );
            assert!(
                dir.path().join("orion").is_dir(),
                "removing is not deleting — the repository is still on disk"
            );
        });
    }

    #[test]
    fn removing_a_declared_project_hides_it_without_touching_the_file() {
        // The config is hand-edited and full of comments, and taking a row
        // out of the panel is not permission to edit it. The removal is
        // recorded beside the file instead, and outlasts a restart all the
        // same.
        with_temp_config(|cfg| {
            let cfg_path = cfg.join("projects.toml");
            std::fs::write(
                &cfg_path,
                r#"# what these are
[[project]]
name = "keep-me"
repos = ["/keep"]

# the one going away
[[project]]
name = "doomed"
repos = ["/doomed"]

[[project]]
name = "also-keep"
repos = ["/also"]
"#,
            )
            .unwrap();

            let d = persistent(crate::config::load().unwrap());
            let doomed = d
                .snapshot()
                .into_iter()
                .find(|p| p.name == "doomed")
                .unwrap()
                .id;
            d.remove_project(doomed).unwrap();

            assert_eq!(
                names_of(&d),
                vec!["keep-me", "also-keep"],
                "gone from the panel"
            );
            assert_eq!(
                names_of(&persistent(crate::config::load().unwrap())),
                vec!["keep-me", "also-keep"],
                "and still gone after a restart"
            );

            let before = r#"# what these are
[[project]]
name = "keep-me"
repos = ["/keep"]

# the one going away
[[project]]
name = "doomed"
repos = ["/doomed"]

[[project]]
name = "also-keep"
repos = ["/also"]
"#;
            assert_eq!(
                std::fs::read_to_string(&cfg_path).unwrap(),
                before,
                "the user's file is untouched, comments and all"
            );
        });
    }

    #[test]
    fn adding_a_repository_extends_that_projects_list_and_leaves_the_file_alone() {
        with_temp_config(|cfg| {
            let cfg_path = cfg.join("projects.toml");
            std::fs::write(
                &cfg_path,
                r#"# hand written
[[project]]
name = "first"
repos = [
  "/one",
]

[[project]]
name = "second"
root = "/somewhere"
"#,
            )
            .unwrap();
            let added = tempfile::tempdir().unwrap();

            let d = persistent(crate::config::load().unwrap());
            let first = d
                .snapshot()
                .into_iter()
                .find(|p| p.name == "first")
                .unwrap();
            d.add_repository(first.id, &added.path().to_string_lossy())
                .unwrap();

            let merged = crate::config::with_overlays(
                crate::config::load().unwrap(),
                &crate::store::Store::open().unwrap().overlays().unwrap(),
            );
            assert_eq!(
                merged.projects[0].repos,
                vec![
                    "/one".to_string(),
                    added.path().to_string_lossy().replace('\\', "/")
                ],
                "the new path joins the ones the config already lists"
            );
            assert_eq!(
                std::fs::read_to_string(&cfg_path).unwrap(),
                r#"# hand written
[[project]]
name = "first"
repos = [
  "/one",
]

[[project]]
name = "second"
root = "/somewhere"
"#,
                "and the file itself never moved"
            );
        });
    }

    #[test]
    fn a_project_that_lists_no_repositories_yet_gains_the_key() {
        with_temp_config(|_| {
            let (_dir, d) = added_project_with(&["orion"]);
            let added = tempfile::tempdir().unwrap();

            // `add_project` writes a block with a root and no `repos`.
            d.add_repository(d.snapshot()[0].id, &added.path().to_string_lossy())
                .unwrap();

            let restarted = persistent(crate::config::load().unwrap());
            let names = repository_names(&restarted);
            assert!(
                names.contains(&"orion".to_string()) && names.len() == 2,
                "{names:?}"
            );
        });
    }

    #[tokio::test]
    async fn a_project_still_holding_panes_is_not_removed() {
        with_temp_config(|_| {
            let (_dir, d) = added_project_with(&["orion"]);
            let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;
            let pane = d.spawn_shell(checkout).unwrap();

            let err = d.remove_project(d.snapshot()[0].id).unwrap_err();
            assert!(err.to_string().contains("panes"), "{err}");
            assert_eq!(d.snapshot().len(), 1, "and it stays put");

            d.close_pane(pane).unwrap();
            d.remove_project(d.snapshot()[0].id).unwrap();
        });
    }

    #[test]
    fn a_removed_repository_does_not_come_back_on_the_next_scan() {
        // The project's root is scanned every ten seconds, so an exclusion
        // that only lived in memory would be undone by the next tick.
        with_temp_config(|_| {
            let (_dir, d) = added_project_with(&["orion", "notes"]);
            let doomed = d.snapshot()[0]
                .repositories
                .iter()
                .find(|r| r.name == "notes")
                .unwrap()
                .id;

            d.remove_repository(doomed).unwrap();
            assert_eq!(repository_names(&d), vec!["orion"]);

            assert!(
                !d.reconcile_repositories(),
                "a scan that finds it again changes nothing"
            );
            assert_eq!(repository_names(&d), vec!["orion"]);
        });
    }

    #[test]
    fn a_removed_repository_is_still_gone_after_a_restart() {
        with_temp_config(|_| {
            let (_dir, d) = added_project_with(&["orion", "notes"]);
            let doomed = d.snapshot()[0].repositories[0].id;
            let kept: Vec<String> = repository_names(&d).into_iter().skip(1).collect();

            d.remove_repository(doomed).unwrap();

            let restarted = persistent(crate::config::load().unwrap());
            assert_eq!(repository_names(&restarted), kept);
        });
    }

    #[tokio::test]
    async fn a_repository_still_holding_panes_is_not_removed() {
        with_temp_config(|_| {
            let (_dir, d) = added_project_with(&["orion"]);
            let repository = d.snapshot()[0].repositories[0].id;
            let pane = d
                .spawn_shell(d.snapshot()[0].repositories[0].checkouts[0].id)
                .unwrap();

            let err = d.remove_repository(repository).unwrap_err();
            assert!(err.to_string().contains("panes"), "{err}");
            assert_eq!(repository_names(&d), vec!["orion"]);

            d.close_pane(pane).unwrap();
            d.remove_repository(repository).unwrap();
        });
    }

    #[test]
    fn re_adding_a_project_brings_back_the_repositories_it_had_lost() {
        // An exclusion describes a project's scan. Once the project is
        // gone, keeping it would mean adding the same directory back and
        // silently getting less than is in it.
        with_temp_config(|_| {
            let (dir, d) = added_project_with(&["orion", "notes"]);
            let doomed = d.snapshot()[0].repositories[0].id;
            d.remove_repository(doomed).unwrap();
            d.remove_project(d.snapshot()[0].id).unwrap();

            let restarted = persistent(crate::config::load().unwrap());
            restarted
                .add_project(&dir.path().to_string_lossy())
                .unwrap();
            assert_eq!(repository_names(&restarted), vec!["notes", "orion"]);
        });
    }

    // --- workspaces ---------------------------------------------------------

    /// `ARGUS_CONFIG_DIR` is process-global, so tests that read or write
    /// config take this lock and restore the variable afterwards.
    static CONFIG_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Runs `f` with the config directory pointed at a fresh temp dir, so
    /// nothing here can see — or corrupt — the real user's config.
    fn with_temp_config<T>(f: impl FnOnce(&std::path::Path) -> T) -> T {
        let _guard = CONFIG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let previous = std::env::var_os("ARGUS_CONFIG_DIR");
        std::env::set_var("ARGUS_CONFIG_DIR", dir.path());
        let out = f(dir.path());
        match previous {
            Some(v) => std::env::set_var("ARGUS_CONFIG_DIR", v),
            None => std::env::remove_var("ARGUS_CONFIG_DIR"),
        }
        out
    }

    #[test]
    fn a_projects_own_scan_rules_decide_what_its_root_turns_up() {
        let root = tempfile::tempdir().unwrap();
        let kept = root.path().join("kept");
        let vendored = root.path().join("vendor").join("thing");
        std::fs::create_dir_all(&kept).unwrap();
        std::fs::create_dir_all(&vendored).unwrap();
        init_repo(&kept);
        init_repo(&vendored);

        let d = Daemon::new(ConfigFile {
            workspaces: Vec::new(),
            projects: vec![ProjectConfig {
                name: "proj".to_string(),
                root: Some(root.path().to_string_lossy().to_string()),
                exclude: vec!["vendor".to_string()],
                ..Default::default()
            }],
            agents: Vec::new(),
            harnesses: Vec::new(),
        });

        assert_eq!(repository_names(&d), vec!["kept".to_string()]);
    }

    // --- reloading the config -----------------------------------------------

    #[test]
    fn a_project_added_to_the_file_arrives_on_reload() {
        with_temp_config(|dir| {
            std::fs::write(
                dir.join("projects.toml"),
                "[[project]]\nname = \"one\"\nrepos = [\"/one\"]\n",
            )
            .unwrap();
            let d = Daemon::new(config::load().unwrap());
            assert_eq!(d.snapshot().len(), 1);

            std::fs::write(
                dir.join("projects.toml"),
                "[[project]]\nname = \"one\"\nrepos = [\"/one\"]\n\n[[project]]\nname = \"two\"\nrepos = [\"/two\"]\n",
            )
            .unwrap();
            d.reload_config().unwrap();

            let names: Vec<String> = d.snapshot().into_iter().map(|p| p.name).collect();
            assert_eq!(names, vec!["one".to_string(), "two".to_string()]);
        });
    }

    #[tokio::test]
    async fn reloading_keeps_the_panes_and_ids_of_everything_still_configured() {
        let repo = tempfile::tempdir().unwrap();
        let repo_path = repo.path().to_string_lossy().replace('\\', "/");
        with_temp_config(|dir| {
            std::fs::write(
                dir.join("projects.toml"),
                format!("[[project]]\nname = \"one\"\nrepos = [\"{repo_path}\"]\n"),
            )
            .unwrap();
            let d = Daemon::new(config::load().unwrap());
            let checkout = only_checkout(&d);
            let pane = d.spawn_shell(checkout).unwrap();

            std::fs::write(
                dir.join("projects.toml"),
                format!(
                    "[[project]]\nname = \"one\"\nrepos = [\"{repo_path}\"]\nexclusive = true\n"
                ),
            )
            .unwrap();
            d.reload_config().unwrap();

            let snapshot = d.snapshot();
            assert_eq!(
                snapshot[0].repositories[0].checkouts[0].id, checkout,
                "the same checkout, not a rebuilt one"
            );
            assert_eq!(
                snapshot[0].repositories[0].checkouts[0].panes.len(),
                1,
                "the shell kept running"
            );
            let _ = d.close_pane(pane);
        });
    }

    #[test]
    fn a_repository_the_file_stopped_naming_leaves_only_when_it_is_empty() {
        let repo = tempfile::tempdir().unwrap();
        let repo_path = repo.path().to_string_lossy().replace('\\', "/");
        with_temp_config(|dir| {
            std::fs::write(
                dir.join("projects.toml"),
                format!("[[project]]\nname = \"one\"\nrepos = [\"{repo_path}\", \"/second\"]\n"),
            )
            .unwrap();
            let d = Daemon::new(config::load().unwrap());
            assert_eq!(d.snapshot()[0].repositories.len(), 2);

            std::fs::write(
                dir.join("projects.toml"),
                format!("[[project]]\nname = \"one\"\nrepos = [\"{repo_path}\"]\n"),
            )
            .unwrap();
            d.reload_config().unwrap();

            assert_eq!(
                d.snapshot()[0].repositories.len(),
                1,
                "the repository with nothing running in it goes"
            );
        });
    }

    #[tokio::test]
    async fn a_project_removed_from_the_file_stays_while_an_agent_is_working_in_it() {
        // The config file does not get to end somebody's work in progress.
        let repo = tempfile::tempdir().unwrap();
        let repo_path = repo.path().to_string_lossy().replace('\\', "/");
        with_temp_config(|dir| {
            std::fs::write(
                dir.join("projects.toml"),
                format!("[[project]]\nname = \"one\"\nrepos = [\"{repo_path}\"]\n"),
            )
            .unwrap();
            let d = Daemon::new(config::load().unwrap());
            let pane = d.spawn_shell(only_checkout(&d)).unwrap();

            std::fs::write(dir.join("projects.toml"), "").unwrap();
            d.reload_config().unwrap();

            assert_eq!(d.snapshot().len(), 1, "still there, still running");

            d.close_pane(pane).unwrap();
            d.reload_config().unwrap();
            assert!(d.snapshot().is_empty(), "and gone once it is empty");
        });
    }

    #[test]
    fn reloading_replaces_the_agent_templates() {
        with_temp_config(|dir| {
            std::fs::write(
                dir.join("projects.toml"),
                "[[agent]]\nname = \"old\"\ncmd = [\"x\"]\n",
            )
            .unwrap();
            let d = Daemon::new(config::load().unwrap());
            assert_eq!(d.template_names(), vec!["old".to_string()]);

            std::fs::write(
                dir.join("projects.toml"),
                "[[agent]]\nname = \"new\"\ncmd = [\"y\"]\n",
            )
            .unwrap();
            d.reload_config().unwrap();

            assert_eq!(d.template_names(), vec!["new".to_string()]);
        });
    }

    #[tokio::test]
    async fn a_project_that_becomes_exclusive_starts_refusing_a_second_agent() {
        let repo = tempfile::tempdir().unwrap();
        let repo_path = repo.path().to_string_lossy().replace('\\', "/");
        with_temp_config(|dir| {
            let agent = "[[agent]]\nname = \"claude\"\ncmd = [\"echo\", \"hi\"]\n";
            std::fs::write(
                dir.join("projects.toml"),
                format!("[[project]]\nname = \"one\"\nrepos = [\"{repo_path}\"]\n{agent}"),
            )
            .unwrap();
            let d = Daemon::new(config::load().unwrap());
            let checkout = only_checkout(&d);
            let first = d.spawn_agent(checkout, "claude").unwrap();

            std::fs::write(
                dir.join("projects.toml"),
                format!(
                    "[[project]]\nname = \"one\"\nrepos = [\"{repo_path}\"]\nexclusive = true\n{agent}"
                ),
            )
            .unwrap();
            d.reload_config().unwrap();

            assert!(
                d.spawn_agent(checkout, "claude").is_err(),
                "the setting applies to the checkout that was already there"
            );
            let _ = d.close_pane(first);
        });
    }

    fn config_with_workspaces() -> ConfigFile {
        ConfigFile {
            workspaces: vec![crate::config::WorkspaceConfig {
                name: "work".to_string(),
            }],
            projects: vec![
                ProjectConfig {
                    name: "home-thing".to_string(),
                    root: None,
                    repos: vec!["/home-thing".to_string()],
                    workspace: None,
                    ..Default::default()
                },
                ProjectConfig {
                    name: "day-job".to_string(),
                    root: None,
                    repos: vec!["/day-job".to_string()],
                    workspace: Some("work".to_string()),
                    ..Default::default()
                },
                ProjectConfig {
                    name: "side".to_string(),
                    root: None,
                    repos: vec!["/side".to_string()],
                    workspace: Some("weekend".to_string()),
                    ..Default::default()
                },
            ],
            agents: Vec::new(),
            harnesses: Vec::new(),
        }
    }

    fn names_of(d: &Daemon) -> Vec<String> {
        d.snapshot().into_iter().map(|p| p.name).collect()
    }

    fn workspace_named(d: &Daemon, name: &str) -> WorkspaceId {
        d.workspaces()
            .into_iter()
            .find(|w| w.name == name)
            .unwrap_or_else(|| panic!("no workspace {name:?}"))
            .id
    }

    #[test]
    fn a_config_that_never_heard_of_workspaces_still_works() {
        // Every existing projects.toml has no workspace keys at all; those
        // projects must land somewhere visible, not vanish.
        with_temp_config(|_| {
            let d = daemon_with_primary("/repo");
            assert_eq!(d.workspaces().len(), 1, "just the built-in default");
            assert!(d.workspaces()[0].open);
            assert_eq!(d.workspaces()[0].name, crate::config::DEFAULT_WORKSPACE);
            assert_eq!(names_of(&d).len(), 1, "and its project is visible");
        });
    }

    #[test]
    fn workspaces_come_from_declarations_and_from_project_references() {
        with_temp_config(|_| {
            let d = persistent(config_with_workspaces());
            let names: Vec<String> = d.workspaces().into_iter().map(|w| w.name).collect();
            assert_eq!(
                names,
                vec!["default", "work", "weekend"],
                "declared and implied alike, in config order"
            );
        });
    }

    #[test]
    fn the_tree_is_scoped_to_the_open_workspace() {
        with_temp_config(|_| {
            let d = persistent(config_with_workspaces());
            assert_eq!(
                names_of(&d),
                vec!["home-thing"],
                "only the default workspace"
            );

            d.open_workspace(workspace_named(&d, "work")).unwrap();
            assert_eq!(names_of(&d), vec!["day-job"]);

            d.open_workspace(workspace_named(&d, "weekend")).unwrap();
            assert_eq!(names_of(&d), vec!["side"]);
        });
    }

    #[test]
    fn switching_workspace_pushes_a_new_tree_and_workspace_list() {
        with_temp_config(|_| {
            let d = persistent(config_with_workspaces());
            let mut tree_rx = d.subscribe_tree();
            let mut ws_rx = d.subscribe_workspaces();

            d.open_workspace(workspace_named(&d, "work")).unwrap();

            let tree = tree_rx.try_recv().expect("clients need the re-scoped tree");
            assert_eq!(tree[0].name, "day-job");
            let ws = ws_rx.try_recv().expect("and the new open flag");
            assert!(ws.iter().find(|w| w.name == "work").unwrap().open);
            assert!(!ws.iter().find(|w| w.name == "default").unwrap().open);
        });
    }

    #[test]
    fn exactly_one_workspace_is_open_at_a_time() {
        with_temp_config(|_| {
            let d = persistent(config_with_workspaces());
            d.open_workspace(workspace_named(&d, "work")).unwrap();
            assert_eq!(d.workspaces().iter().filter(|w| w.open).count(), 1);
        });
    }

    #[test]
    fn reopening_the_already_open_workspace_changes_nothing() {
        with_temp_config(|_| {
            let d = persistent(config_with_workspaces());
            let mut tree_rx = d.subscribe_tree();
            let open = d.workspaces().into_iter().find(|w| w.open).unwrap().id;

            d.open_workspace(open).unwrap();
            assert!(
                tree_rx.try_recv().is_err(),
                "a no-op switch must not churn every client's tree"
            );
        });
    }

    #[test]
    fn switching_to_a_workspace_that_does_not_exist_is_an_error() {
        with_temp_config(|_| {
            let d = persistent(config_with_workspaces());
            assert!(d.open_workspace(WorkspaceId(9999)).is_err());
        });
    }

    #[test]
    fn the_open_workspace_is_remembered_for_the_next_daemon() {
        with_temp_config(|_| {
            let d = persistent(config_with_workspaces());
            d.open_workspace(workspace_named(&d, "work")).unwrap();
            drop(d);

            let next = persistent(config_with_workspaces());
            assert_eq!(
                next.workspaces().into_iter().find(|w| w.open).unwrap().name,
                "work",
                "restarting should land you back where you were"
            );
        });
    }

    #[test]
    fn a_remembered_workspace_that_no_longer_exists_falls_back_to_default() {
        with_temp_config(|_| {
            let d = persistent(config_with_workspaces());
            d.open_workspace(workspace_named(&d, "weekend")).unwrap();
            drop(d);

            // The user deletes that workspace's project from their config.
            let mut cfg = config_with_workspaces();
            cfg.projects
                .retain(|p| p.workspace.as_deref() != Some("weekend"));
            let next = Daemon::new(cfg);
            assert_eq!(
                next.workspaces().into_iter().find(|w| w.open).unwrap().name,
                crate::config::DEFAULT_WORKSPACE,
                "a dangling name must not leave every client staring at nothing"
            );
        });
    }

    #[test]
    fn workspace_rollups_count_projects_and_panes_across_the_whole_workspace() {
        with_temp_config(|_| {
            let d = persistent(config_with_workspaces());
            let ws = d.workspaces();
            let default = ws.iter().find(|w| w.name == "default").unwrap();
            assert_eq!(default.projects, 1);
            assert_eq!(default.panes, 0);
        });
    }

    #[tokio::test]
    async fn panes_in_a_closed_workspace_keep_running_and_stay_counted() {
        // The whole point of scoping rather than unloading: an agent in a
        // workspace you are not looking at is still working, and you should
        // still be able to see that it is.
        let dir = tempfile::tempdir().unwrap();
        with_temp_config(|_| {
            let d = Daemon::new(ConfigFile {
                workspaces: Vec::new(),
                projects: vec![
                    ProjectConfig {
                        name: "here".to_string(),
                        root: None,
                        repos: vec![dir.path().to_string_lossy().to_string()],
                        workspace: None,
                        ..Default::default()
                    },
                    ProjectConfig {
                        name: "elsewhere".to_string(),
                        root: None,
                        repos: vec![dir.path().to_string_lossy().to_string()],
                        workspace: Some("other".to_string()),
                        ..Default::default()
                    },
                ],
                agents: Vec::new(),
                harnesses: Vec::new(),
            });

            // Spawn a pane in the *other* workspace, then look away.
            let other = workspace_named(&d, "other");
            d.open_workspace(other).unwrap();
            let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;
            let pane = d.spawn_shell(checkout).unwrap();

            d.open_workspace(workspace_named(&d, "default")).unwrap();
            assert_eq!(names_of(&d), vec!["here"], "the tree re-scoped");

            let rollup = d.workspaces();
            let other_ws = rollup.iter().find(|w| w.name == "other").unwrap();
            assert_eq!(
                other_ws.panes, 1,
                "the pane is still running and still counted"
            );

            let _ = d.close_pane(pane);
        });
    }

    #[test]
    fn adding_a_project_files_it_under_the_open_workspace() {
        let repo = tempfile::tempdir().unwrap();
        with_temp_config(|_| {
            let d = persistent(config_with_workspaces());
            d.open_workspace(workspace_named(&d, "work")).unwrap();

            d.add_project(&repo.path().to_string_lossy()).unwrap();

            assert!(
                names_of(&d)
                    .iter()
                    .any(|n| n == repo.path().file_name().unwrap().to_str().unwrap()),
                "a project added while looking at a workspace belongs to it"
            );
            let work = d
                .workspaces()
                .into_iter()
                .find(|w| w.name == "work")
                .unwrap();
            assert_eq!(work.projects, 2);
        });
    }

    #[test]
    fn an_added_projects_workspace_is_persisted_so_it_survives_a_restart() {
        let repo = tempfile::tempdir().unwrap();
        with_temp_config(|_| {
            let d = persistent(config_with_workspaces());
            d.open_workspace(workspace_named(&d, "work")).unwrap();
            d.add_project(&repo.path().to_string_lossy()).unwrap();

            let restarted = persistent(config_with_workspaces());
            let work = workspace_named(&restarted, "work");
            assert!(
                restarted
                    .snapshot()
                    .iter()
                    .any(|p| p.name == repo.path().file_name().unwrap().to_string_lossy()),
                "the added project should be in the open workspace"
            );
            assert_eq!(
                restarted
                    .workspaces()
                    .into_iter()
                    .find(|w| w.open)
                    .unwrap()
                    .id,
                work,
                "which is still the one it was added to"
            );
        });
    }

    #[test]
    fn a_created_workspace_is_declared_on_disk_and_opened() {
        with_temp_config(|dir| {
            let d = persistent(config_with_workspaces());
            d.create_workspace("side").unwrap();

            let ws = d.workspaces();
            let side = ws.iter().find(|w| w.name == "side").expect("it exists");
            assert!(side.open, "you land in what you just made");
            assert_eq!(side.projects, 0, "and it starts empty");
            assert_eq!(names_of(&d).len(), 0, "so the tree is empty too");

            // Declared, not implied: an empty workspace has nothing in it
            // to imply it, so without a record of its own it would not
            // survive a restart.
            assert!(
                crate::store::Store::open()
                    .unwrap()
                    .workspace_overlays()
                    .unwrap()
                    .contains(&"side".to_string()),
                "the declaration must be recorded, not just held in memory"
            );
            assert!(
                !declares_a_workspace(dir),
                "and recorded beside the user's config, not in it"
            );
        });
    }

    #[test]
    fn a_workspace_created_then_given_a_project_is_how_grouping_starts() {
        // The whole point: reaching a second workspace without editing
        // projects.toml by hand.
        let repo = tempfile::tempdir().unwrap();
        with_temp_config(|_| {
            let d = daemon_with_primary("/repo");
            d.create_workspace("side").unwrap();
            d.add_project(&repo.path().to_string_lossy()).unwrap();

            let side = d
                .workspaces()
                .into_iter()
                .find(|w| w.name == "side")
                .unwrap();
            assert_eq!(side.projects, 1, "added into what was open");
            assert_eq!(names_of(&d).len(), 1);
        });
    }

    #[test]
    fn a_created_workspace_survives_a_restart() {
        with_temp_config(|_| {
            let d = persistent(config_with_workspaces());
            d.create_workspace("side").unwrap();
            drop(d);

            let reloaded = persistent(crate::config::load().unwrap());
            let names: Vec<String> = reloaded.workspaces().into_iter().map(|w| w.name).collect();
            assert!(names.contains(&"side".to_string()), "{names:?}");
            assert!(
                reloaded
                    .workspaces()
                    .iter()
                    .find(|w| w.name == "side")
                    .unwrap()
                    .open,
                "and it is still the one open"
            );
        });
    }

    #[test]
    fn a_workspace_that_already_exists_is_refused_rather_than_reopened() {
        // The picker already lists the existing rows; one gesture meaning
        // both "go there" and "make it" is how duplicates get made.
        with_temp_config(|dir| {
            let d = persistent(config_with_workspaces());
            assert!(d.create_workspace("work").is_err());
            assert!(d.create_workspace("   ").is_err(), "nor an empty name");

            assert_eq!(d.workspaces().len(), 3, "nothing was added");
            let written = std::fs::read_to_string(dir.join("projects.toml")).unwrap_or_default();
            assert!(
                !written.contains("[[workspace]]"),
                "and nothing was written:\n{written}"
            );
        });
    }

    #[test]
    fn creating_a_workspace_pushes_a_new_tree_and_workspace_list() {
        with_temp_config(|_| {
            let d = persistent(config_with_workspaces());
            let mut tree_rx = d.subscribe_tree();
            let mut ws_rx = d.subscribe_workspaces();

            d.create_workspace("side").unwrap();

            let tree = tree_rx.try_recv().expect("clients need the empty tree");
            assert!(tree.is_empty());
            let ws = ws_rx.try_recv().expect("and the new row");
            assert!(ws.iter().any(|w| w.name == "side" && w.open));
        });
    }

    #[test]
    fn an_editor_pane_will_not_open_a_path_outside_the_checkout() {
        // `path` comes from a client and lands on a command line.
        let dir = tempfile::tempdir().unwrap();
        let d = daemon_with_primary(&dir.path().to_string_lossy());
        let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;

        for bad in [
            "",
            "../elsewhere.rs",
            "sub/../../elsewhere.rs",
            "/etc/passwd",
            r"\\server\share\x",
            r"C:\Windows\x",
        ] {
            assert!(
                d.spawn_editor(checkout, bad, None, false, None).is_err(),
                "{bad:?} should be refused"
            );
        }
    }

    // --- branches -----------------------------------------------------------

    /// A real repo with one commit, and a daemon whose only checkout is it.
    fn daemon_on_a_repo() -> (tempfile::TempDir, Arc<Daemon>) {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        let d = daemon_with_primary(&dir.path().to_string_lossy());
        (dir, d)
    }

    /// The same repo, in a project that says where worktrees go and what to
    /// run in one.
    fn daemon_on_a_repo_with(
        worktree_root: Option<&str>,
        setup: &[&str],
    ) -> (tempfile::TempDir, Arc<Daemon>) {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        let d = Daemon::new(ConfigFile {
            workspaces: Vec::new(),
            projects: vec![ProjectConfig {
                name: "proj".to_string(),
                repos: vec![dir.path().to_string_lossy().to_string()],
                worktree_root: worktree_root.map(str::to_string),
                setup: setup.iter().map(|s| s.to_string()).collect(),
                ..Default::default()
            }],
            agents: Vec::new(),
            harnesses: Vec::new(),
        });
        (dir, d)
    }

    fn init_repo(dir: &std::path::Path) {
        let repo = git2::Repository::init(dir).unwrap();
        std::fs::write(dir.join("a.txt"), "one\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("a.txt")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = git2::Signature::now("t", "t@example.com").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "first", &tree, &[])
            .unwrap();
        drop(tree);
        drop(index);
        drop(repo);
    }

    fn head_of(path: &std::path::Path) -> String {
        git2::Repository::open(path)
            .unwrap()
            .head()
            .unwrap()
            .shorthand()
            .unwrap()
            .to_string()
    }

    #[tokio::test]
    async fn creating_a_worktree_adds_it_to_its_repository() {
        let (dir, d) = daemon_on_a_repo();
        let base = d.snapshot()[0].repositories[0].checkouts[0].id;

        d.create_worktree(base, "feature-x".to_string())
            .await
            .unwrap();

        let snapshot = d.snapshot();
        let checkouts = &snapshot[0].repositories[0].checkouts;
        assert_eq!(checkouts.len(), 2);
        assert_eq!(checkouts[1].name, "feature-x");
        assert!(!checkouts[1].primary);
        let path = std::path::Path::new(&checkouts[1].path);
        assert_eq!(head_of(path), "feature-x");
        assert!(path.starts_with(dir.path()));
    }

    /// A branch on the repo's current commit, holding nothing of its own.
    fn branch_off_head(dir: &std::path::Path, name: &str) {
        let repo = git2::Repository::open(dir).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch(name, &head, false).unwrap();
    }

    /// A second repository standing in for a remote, and the repo under
    /// test wired to it. Git takes a path as a URL, forward slashes and all.
    fn remote_holding(branch: &str) -> (tempfile::TempDir, String) {
        let upstream = tempfile::tempdir().unwrap();
        init_repo(upstream.path());
        branch_off_head(upstream.path(), branch);
        let url = upstream.path().to_string_lossy().replace('\\', "/");
        (upstream, url)
    }

    #[tokio::test]
    async fn a_fetch_brings_the_remote_only_branches_into_the_tree() {
        let (dir, d) = daemon_on_a_repo();
        let (_upstream, url) = remote_holding("from-elsewhere");
        git2::Repository::open(dir.path())
            .unwrap()
            .remote("origin", &url)
            .unwrap();

        d.fetch(only_checkout(&d)).await.unwrap();

        let remote = &d.snapshot()[0].repositories[0].remote_branches;
        assert!(
            remote.iter().any(|b| b == "origin/from-elsewhere"),
            "the fetch is what makes the row appear; got {remote:?}"
        );
    }

    #[tokio::test]
    async fn a_worktree_for_a_remote_only_branch_starts_from_the_remote() {
        // Otherwise the row said `origin/x` and gave you a branch of that
        // name off this checkout's HEAD, which is not the work you asked
        // for. The two repositories share no history, so the commit id is
        // proof of where the branch came from.
        let (dir, d) = daemon_on_a_repo();
        let (upstream, url) = remote_holding("from-elsewhere");
        git2::Repository::open(dir.path())
            .unwrap()
            .remote("origin", &url)
            .unwrap();
        d.fetch(only_checkout(&d)).await.unwrap();

        d.create_worktree(only_checkout(&d), "from-elsewhere".to_string())
            .await
            .unwrap();

        let made = PathBuf::from(&d.snapshot()[0].repositories[0].checkouts[1].path);
        assert_eq!(head_of(&made), "from-elsewhere");
        let there = git2::Repository::open(upstream.path())
            .unwrap()
            .find_branch("from-elsewhere", git2::BranchType::Local)
            .unwrap()
            .get()
            .peel_to_commit()
            .unwrap()
            .id();
        let here = git2::Repository::open(&made)
            .unwrap()
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id();
        assert_eq!(here, there, "the worktree should hold the remote's work");
    }

    #[tokio::test]
    async fn a_pull_fast_forwards_the_checkout_onto_its_upstream() {
        let upstream = tempfile::tempdir().unwrap();
        init_repo(upstream.path());
        let url = upstream.path().to_string_lossy().replace('\\', "/");
        let clone = tempfile::tempdir().unwrap();
        let local = clone.path().join("work");
        git2::build::RepoBuilder::new().clone(&url, &local).unwrap();
        let d = daemon_with_primary(&local.to_string_lossy());

        // Work lands upstream after the clone was taken.
        let repo = git2::Repository::open(upstream.path()).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let tree = head.tree().unwrap();
        let sig = git2::Signature::now("t", "t@example.com").unwrap();
        let moved = repo
            .commit(Some("HEAD"), &sig, &sig, "later", &tree, &[&head])
            .unwrap();

        d.pull(only_checkout(&d)).await.unwrap();

        let here = git2::Repository::open(&local)
            .unwrap()
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id();
        assert_eq!(here, moved);
    }

    #[tokio::test]
    async fn deleting_a_branch_takes_it_off_the_repository() {
        let (dir, d) = daemon_on_a_repo();
        branch_off_head(dir.path(), "doomed");
        d.refresh_branches();
        assert!(
            d.snapshot()[0].repositories[0]
                .branches
                .iter()
                .any(|b| b == "doomed"),
            "the branch has to be there to be deleted"
        );

        d.delete_branch(only_checkout(&d), "doomed").await.unwrap();

        assert!(
            !d.snapshot()[0].repositories[0]
                .branches
                .iter()
                .any(|b| b == "doomed"),
            "and the row goes with it, without waiting for the next poll"
        );
    }

    #[tokio::test]
    async fn a_branch_holding_commits_nothing_else_has_is_refused() {
        // `-d`, never `-D`: the row you delete from says nothing about
        // whether those commits survive anywhere, so git's refusal stands.
        let (dir, d) = daemon_on_a_repo();
        let repo = git2::Repository::open(dir.path()).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let tree = head.tree().unwrap();
        let sig = git2::Signature::now("t", "t@example.com").unwrap();
        repo.commit(
            Some("refs/heads/spike"),
            &sig,
            &sig,
            "work",
            &tree,
            &[&head],
        )
        .unwrap();

        let err = d
            .delete_branch(only_checkout(&d), "spike")
            .await
            .unwrap_err()
            .to_string();

        assert!(err.contains("not fully merged"), "got {err:?}");
        assert!(
            git2::Repository::open(dir.path())
                .unwrap()
                .find_branch("spike", git2::BranchType::Local)
                .is_ok(),
            "a refused deletion leaves the branch alone"
        );
    }

    #[tokio::test]
    async fn the_main_branch_is_not_deletable_from_its_own_row() {
        let (dir, d) = daemon_on_a_repo();
        branch_off_head(dir.path(), "main");

        let err = d
            .delete_branch(only_checkout(&d), "main")
            .await
            .unwrap_err()
            .to_string();

        assert!(err.contains("main branch"), "got {err:?}");
    }

    /// The daemon, plus the id of a linked worktree it just made.
    async fn daemon_with_a_worktree(name: &str) -> (tempfile::TempDir, Arc<Daemon>, CheckoutId) {
        let (dir, d) = daemon_on_a_repo();
        d.create_worktree(only_checkout(&d), name.to_string())
            .await
            .unwrap();
        let id = d.snapshot()[0].repositories[0].checkouts[1].id;
        (dir, d, id)
    }

    #[tokio::test]
    async fn removing_a_worktree_takes_its_directory_and_its_row_with_it() {
        let (_dir, d, worktree) = daemon_with_a_worktree("doomed").await;
        let path = PathBuf::from(&d.snapshot()[0].repositories[0].checkouts[1].path);

        d.remove_checkout(worktree).await.unwrap();

        assert_eq!(d.snapshot()[0].repositories[0].checkouts.len(), 1);
        assert!(!path.exists(), "the working directory should be gone");
    }

    #[tokio::test]
    async fn a_removal_git_would_refuse_keeps_the_panes_it_would_have_killed() {
        // The point of checking before killing: a locked worktree is a
        // refusal git only reports once it runs, and by then the agents that
        // were working in it are already dead.
        let (dir, d, worktree) = daemon_with_a_worktree("locked-up").await;
        let pane = d.spawn_shell(worktree).unwrap();
        git2::Repository::open(dir.path())
            .unwrap()
            .find_worktree("locked-up")
            .unwrap()
            .lock(Some("held by hand"))
            .unwrap();

        let err = d.remove_checkout(worktree).await.unwrap_err().to_string();

        assert!(err.contains("locked"), "got {err:?}");
        let snapshot = d.snapshot();
        let checkouts = &snapshot[0].repositories[0].checkouts;
        assert_eq!(checkouts.len(), 2, "the checkout stays");
        assert_eq!(checkouts[1].panes.len(), 1, "and so does what was running");
        d.close_pane(pane).unwrap();
    }

    #[tokio::test]
    async fn removing_a_checkout_whose_directory_is_already_gone_clears_it() {
        // `git worktree remove` refuses a path it cannot find, which would
        // strand the row for a directory the user deleted by hand.
        let (dir, d, worktree) = daemon_with_a_worktree("deleted").await;
        let path = PathBuf::from(&d.snapshot()[0].repositories[0].checkouts[1].path);
        std::fs::remove_dir_all(&path).unwrap();

        d.remove_checkout(worktree).await.unwrap();

        assert_eq!(d.snapshot()[0].repositories[0].checkouts.len(), 1);
        let repo = git2::Repository::open(dir.path()).unwrap();
        assert_eq!(
            repo.worktrees().unwrap().len(),
            0,
            "the registration should have been pruned too"
        );
    }

    #[tokio::test]
    async fn the_primary_checkout_is_never_removable() {
        let (dir, d) = daemon_on_a_repo();

        assert!(d.remove_checkout(only_checkout(&d)).await.is_err());

        assert!(dir.path().join("a.txt").exists());
        assert_eq!(d.snapshot()[0].repositories[0].checkouts.len(), 1);
    }

    #[tokio::test]
    async fn creating_a_branch_moves_this_checkout_onto_it() {
        // Unlike `create_worktree`, which puts the branch in a directory of
        // its own and leaves this checkout where it was.
        let (dir, d) = daemon_on_a_repo();
        let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;

        d.create_branch(checkout, "feature/x").await.unwrap();

        assert_eq!(head_of(dir.path()), "feature/x");
        assert_eq!(
            d.snapshot()[0].repositories[0].checkouts.len(),
            1,
            "no new checkout — that is what a worktree is for"
        );
    }

    #[tokio::test]
    async fn the_checkouts_name_follows_the_branch_it_moves_to() {
        let (_dir, d) = daemon_on_a_repo();
        let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;

        d.create_branch(checkout, "feature/x").await.unwrap();

        assert_eq!(
            d.snapshot()[0].repositories[0].checkouts[0].name,
            "feature/x"
        );
    }

    #[test]
    fn the_checkouts_name_follows_a_branch_switch_made_outside_argus() {
        let (dir, d) = daemon_on_a_repo();
        let repo = git2::Repository::open(dir.path()).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("outside", &head, false).unwrap();
        repo.set_head("refs/heads/outside").unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .unwrap();

        // Nothing told the daemon, so the poll is what finds it — the same
        // step `start_git_poll` runs every two seconds. `snapshot` reads the
        // cache that poll fills and never git itself, because it is taken
        // under the lock keystrokes need.
        d.refresh_git_status();

        assert_eq!(d.snapshot()[0].repositories[0].checkouts[0].name, "outside");
    }

    #[test]
    fn a_tree_snapshot_reads_no_git_of_its_own() {
        // The guarantee the status cache exists for: `snapshot` runs under
        // the daemon's one lock, and `write_pane` needs that same lock to
        // find the pty a keystroke belongs to. Reading git there put several
        // milliseconds of blocking I/O per checkout in front of the next
        // key. Asserted by moving the repo out from under the daemon: a
        // snapshot that still consulted git would lose the branch name.
        let (dir, d) = daemon_on_a_repo();
        d.refresh_git_status();
        let named = d.snapshot()[0].repositories[0].checkouts[0].name.clone();

        std::fs::remove_dir_all(dir.path().join(".git")).unwrap();

        assert_eq!(
            d.snapshot()[0].repositories[0].checkouts[0].name,
            named,
            "the snapshot went back to git instead of using the cache"
        );
    }

    #[test]
    fn startup_names_checkouts_from_head_without_walking_the_workdir() {
        // Daemon construction used to run a full `git::status` on every
        // checkout before the process listened. Untracked files made that
        // a workdir walk of every repository under a project root. HEAD is
        // enough to name the row; dirty counts arrive on the first poll.
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        std::fs::write(dir.path().join("untracked.txt"), "x").unwrap();
        let d = daemon_with_primary(&dir.path().to_string_lossy());

        let checkout = &d.snapshot()[0].repositories[0].checkouts[0];
        assert_eq!(checkout.name, head_of(dir.path()));
        assert_eq!(
            checkout.git.as_ref().map(|g| g.dirty),
            Some(false),
            "startup must not walk the workdir for untracked files"
        );

        d.refresh_git_status();
        assert!(
            d.snapshot()[0].repositories[0].checkouts[0]
                .git
                .as_ref()
                .is_some_and(|g| g.dirty),
            "the poll still sees the untracked file"
        );
    }

    #[tokio::test]
    async fn switching_moves_between_branches_that_already_exist() {
        let (dir, d) = daemon_on_a_repo();
        let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;
        let start = head_of(dir.path());
        d.create_branch(checkout, "other").await.unwrap();

        d.switch_branch(checkout, &start).await.unwrap();

        assert_eq!(head_of(dir.path()), start);
        assert_eq!(d.snapshot()[0].repositories[0].checkouts[0].name, start);
    }

    #[tokio::test]
    async fn switching_pushes_a_new_tree_so_every_client_sees_the_move() {
        let (_dir, d) = daemon_on_a_repo();
        let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;
        let mut rx = d.subscribe_tree();

        d.create_branch(checkout, "feature/x").await.unwrap();

        let tree = rx.try_recv().expect("clients need to be told");
        assert_eq!(tree[0].repositories[0].checkouts[0].name, "feature/x");
    }

    #[tokio::test]
    async fn switching_to_a_branch_that_does_not_exist_reports_gits_own_words() {
        let (_dir, d) = daemon_on_a_repo();
        let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;

        let err = d
            .switch_branch(checkout, "no-such-branch")
            .await
            .unwrap_err()
            .to_string();

        assert!(
            !err.is_empty(),
            "git's refusal is what the user needs to read"
        );
    }

    #[tokio::test]
    async fn creating_a_branch_that_already_exists_is_refused() {
        let (_dir, d) = daemon_on_a_repo();
        let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;
        d.create_branch(checkout, "taken").await.unwrap();

        assert!(d.create_branch(checkout, "taken").await.is_err());
    }

    #[tokio::test]
    async fn an_empty_or_flag_like_branch_name_never_reaches_git() {
        // A leading dash would be parsed as an option rather than a name.
        let (_dir, d) = daemon_on_a_repo();
        let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;

        for bad in ["", "   ", "--force", "-b"] {
            assert!(
                d.create_branch(checkout, bad).await.is_err(),
                "{bad:?} should be refused"
            );
        }
    }

    #[tokio::test]
    async fn a_worktree_branch_name_is_checked_as_strictly_as_a_branch_switch() {
        // The name is both a git argument and the directory Argus builds
        // from it, so a rooted or climbing one would put the worktree
        // wherever it said rather than under the worktrees root.
        let (dir, d) = daemon_on_a_repo();
        let base = only_checkout(&d);
        let escaped = dir.path().parent().unwrap().join("escaped");

        for bad in [
            "",
            "   ",
            "-b",
            "--force",
            "..",
            "../escaped",
            r"..\escaped",
            "/escaped",
            r"C:\escaped",
        ] {
            assert!(
                d.create_worktree(base, bad.to_string()).await.is_err(),
                "{bad:?} should be refused"
            );
        }

        assert!(!escaped.exists(), "a worktree landed outside the root");
        assert_eq!(
            d.snapshot()[0].repositories[0].checkouts.len(),
            1,
            "nothing should have been added"
        );
    }

    #[tokio::test]
    async fn a_branch_name_with_a_slash_still_nests_under_the_worktrees_root() {
        let (dir, d) = daemon_on_a_repo();

        d.create_worktree(only_checkout(&d), "feat/nested".to_string())
            .await
            .unwrap();

        let path = PathBuf::from(&d.snapshot()[0].repositories[0].checkouts[1].path);
        assert!(path.starts_with(dir.path().join(".argus").join("worktrees")));
        assert_eq!(head_of(&path), "feat/nested");
    }

    #[tokio::test]
    async fn a_branch_switch_made_outside_argus_reaches_clients_without_waiting_for_the_poll() {
        // The poll would find this too, two seconds later. The watch is
        // what makes an agent's commit or switch show up as it happens.
        let (dir, d) = daemon_on_a_repo();
        d.refresh_git_status();
        let mut tree = d.subscribe_tree();
        d.start_git_watch();
        // The first sync of the watched set happens on the interval's
        // immediate first tick; give it the scheduler slot it needs.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let repo = git2::Repository::open(dir.path()).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("from-a-shell", &head, false).unwrap();
        repo.set_head("refs/heads/from-a-shell").unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .unwrap();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let named = loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            assert!(!left.is_zero(), "the watch never reported the switch");
            let Ok(Ok(projects)) = tokio::time::timeout(left, tree.recv()).await else {
                panic!("the watch never reported the switch");
            };
            let name = projects[0].repositories[0].checkouts[0].name.clone();
            if name == "from-a-shell" {
                break name;
            }
        };

        assert_eq!(named, "from-a-shell");
    }

    #[tokio::test]
    async fn a_configured_worktree_root_is_where_worktrees_go() {
        let elsewhere = tempfile::tempdir().unwrap();
        let (dir, d) = daemon_on_a_repo_with(Some(&elsewhere.path().to_string_lossy()), &[]);

        d.create_worktree(only_checkout(&d), "over-there".to_string())
            .await
            .unwrap();

        let made = PathBuf::from(&d.snapshot()[0].repositories[0].checkouts[1].path);
        let repo_name = dir.path().file_name().unwrap();
        assert_eq!(
            made,
            elsewhere.path().join(repo_name).join("over-there"),
            "one directory per repository under the root, so two repos can share a branch name"
        );
        assert!(!dir.path().join(".argus").exists(), "not the default root");
    }

    #[tokio::test]
    async fn setup_commands_run_in_the_worktree_that_was_just_made() {
        // `git tag` is a command every machine running these tests has, and
        // it leaves something a test can read back.
        let (_dir, d) = daemon_on_a_repo_with(None, &["git tag setup-ran"]);

        d.create_worktree(only_checkout(&d), "with-setup".to_string())
            .await
            .unwrap();

        let made = PathBuf::from(&d.snapshot()[0].repositories[0].checkouts[1].path);
        let repo = git2::Repository::open(&made).unwrap();
        let tags = repo.tag_names(None).unwrap();
        assert!(
            tags.iter().flatten().any(|t| t == "setup-ran"),
            "the setup command should have run in {}",
            made.display()
        );
    }

    #[tokio::test]
    async fn a_setup_command_that_fails_is_reported_without_taking_the_worktree_with_it() {
        let (_dir, d) = daemon_on_a_repo_with(None, &["git not-a-git-command"]);

        let err = d
            .create_worktree(only_checkout(&d), "half-set-up".to_string())
            .await
            .unwrap_err()
            .to_string();

        assert!(err.contains("not-a-git-command"), "got {err:?}");
        let snapshot = d.snapshot();
        let checkouts = &snapshot[0].repositories[0].checkouts;
        assert_eq!(checkouts.len(), 2, "the worktree is still there to fix");
        assert!(PathBuf::from(&checkouts[1].path).is_dir());
    }

    #[tokio::test]
    async fn a_branch_with_no_checkout_is_listed_on_its_repository() {
        let (dir, d) = daemon_on_a_repo();
        let checkout = only_checkout(&d);
        let on_it = head_of(dir.path());
        d.create_branch(checkout, "parked").await.unwrap();
        d.switch_branch(checkout, &on_it).await.unwrap();

        d.refresh_git_status();
        d.refresh_branches();

        assert_eq!(
            d.snapshot()[0].repositories[0].branches,
            vec!["parked".to_string()],
            "the branch nothing is sitting on is the one to offer"
        );
    }

    #[tokio::test]
    async fn a_branch_a_checkout_is_sitting_on_is_not_offered_as_one_to_go_to() {
        let (_dir, d) = daemon_on_a_repo();
        d.create_worktree(only_checkout(&d), "in-a-worktree".to_string())
            .await
            .unwrap();

        d.refresh_git_status();
        d.refresh_branches();

        assert!(
            !d.snapshot()[0].repositories[0]
                .branches
                .contains(&"in-a-worktree".to_string()),
            "it already has a directory of its own"
        );
    }

    #[tokio::test]
    async fn a_branch_that_already_exists_gets_a_worktree_rather_than_a_refusal() {
        // The tree offers a worktree for a branch row, and every branch row
        // is a branch that already exists.
        let (_dir, d) = daemon_on_a_repo();
        let checkout = only_checkout(&d);
        let on_it = head_of(&PathBuf::from(
            &d.snapshot()[0].repositories[0].checkouts[0].path,
        ));
        d.create_branch(checkout, "waiting").await.unwrap();
        d.switch_branch(checkout, &on_it).await.unwrap();

        d.create_worktree(checkout, "waiting".to_string())
            .await
            .unwrap();

        let snapshot = d.snapshot();
        let made = &snapshot[0].repositories[0].checkouts[1];
        assert_eq!(head_of(&PathBuf::from(&made.path)), "waiting");
    }

    #[tokio::test]
    async fn a_dirty_primary_checkout_is_not_switched_out_from_under_its_work() {
        let (dir, d) = daemon_on_a_repo();
        let checkout = only_checkout(&d);
        let on_it = head_of(dir.path());
        d.create_branch(checkout, "elsewhere").await.unwrap();
        d.switch_branch(checkout, &on_it).await.unwrap();
        std::fs::write(dir.path().join("a.txt"), "uncommitted\n").unwrap();

        let err = d
            .switch_branch(checkout, "elsewhere")
            .await
            .unwrap_err()
            .to_string();

        assert!(err.contains("worktree"), "say what to do instead: {err:?}");
        assert_eq!(head_of(dir.path()), on_it, "still where the work is");
    }

    #[tokio::test]
    async fn a_dirty_worktree_still_switches_because_argus_made_it() {
        // The refusal is about the repo the user already had. A linked
        // worktree is Argus's own, and an agent moving between branches in
        // one is ordinary work.
        let (_dir, d, worktree) = daemon_with_a_worktree("scratch").await;
        let path = PathBuf::from(&d.snapshot()[0].repositories[0].checkouts[1].path);
        d.create_branch(worktree, "second").await.unwrap();
        std::fs::write(path.join("a.txt"), "uncommitted\n").unwrap();

        d.switch_branch(worktree, "scratch").await.unwrap();

        assert_eq!(head_of(&path), "scratch");
    }

    #[tokio::test]
    async fn a_clean_primary_checkout_still_switches() {
        let (dir, d) = daemon_on_a_repo();
        let checkout = only_checkout(&d);
        let on_it = head_of(dir.path());
        d.create_branch(checkout, "clean-move").await.unwrap();

        d.switch_branch(checkout, &on_it).await.unwrap();

        assert_eq!(head_of(dir.path()), on_it);
    }

    #[tokio::test]
    async fn a_branch_operation_on_a_checkout_that_is_gone_errors() {
        let (_dir, d) = daemon_on_a_repo();
        assert!(d.create_branch(CheckoutId(9999), "x").await.is_err());
        assert!(d.switch_branch(CheckoutId(9999), "x").await.is_err());
    }

    #[tokio::test]
    async fn a_gui_editor_never_gets_a_pane_even_when_a_pane_was_asked_for() {
        // A GUI editor in a pty is a blank grid and a child that never speaks.
        // Use a missing executable with a known GUI-editor name so this test
        // exercises that branch without opening a real window.
        let dir = tempfile::tempdir().unwrap();
        let d = daemon_with_primary(&dir.path().to_string_lossy());
        let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;
        std::fs::write(dir.path().join("a.txt"), "x").unwrap();

        let made = d.spawn_editor(checkout, "a.txt", None, false, Some("missing/notepad.exe"));

        assert!(
            made.is_err(),
            "the deliberately missing editor must not launch"
        );
        assert!(
            d.snapshot()[0].repositories[0].checkouts[0]
                .panes
                .is_empty(),
            "a GUI editor must not become a pane"
        );
    }

    #[tokio::test]
    async fn a_terminal_editor_pane_has_no_harness_session() {
        let dir = tempfile::tempdir().unwrap();
        let d = daemon_with_primary(&dir.path().to_string_lossy());
        let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;
        std::fs::write(dir.path().join("a.txt"), "x").unwrap();
        let editor = std::env::current_exe().unwrap();

        let pane = d
            .spawn_editor(
                checkout,
                "a.txt",
                None,
                false,
                Some(&editor.to_string_lossy()),
            )
            .unwrap();

        let stored = d
            .inner
            .lock()
            .unwrap()
            .projects
            .iter()
            .flat_map(|project| &project.repositories)
            .flat_map(|repository| &repository.checkouts)
            .flat_map(|checkout| &checkout.panes)
            .find(|candidate| candidate.id == pane)
            .map(|pane| (pane.kind, pane.harness_session_id.clone()));
        assert_eq!(stored, Some((PaneKind::Editor, None)));
        d.close_pane(pane).unwrap();
    }

    // --- session restore ----------------------------------------------------

    /// A daemon whose only project is `dir`, with one agent template that
    /// runs the platform shell so restoring one actually starts something.
    ///
    /// Backed by the store in the temp config directory rather than an
    /// in-memory one, so what [`record`] writes is what it reads — these
    /// tests are about surviving a restart, and a store that does not
    /// outlive the daemon cannot show that.
    fn daemon_for_restore(dir: &std::path::Path) -> Arc<Daemon> {
        Daemon::with_store(restore_config(dir), crate::store::Store::open().unwrap())
    }

    fn restore_config(dir: &std::path::Path) -> ConfigFile {
        ConfigFile {
            workspaces: Vec::new(),
            projects: vec![ProjectConfig {
                name: "proj".to_string(),
                root: None,
                repos: vec![dir.to_string_lossy().to_string()],
                workspace: None,
                ..Default::default()
            }],
            agents: vec![AgentConfig {
                name: "test-agent".to_string(),
                cmd: vec![if cfg!(windows) { "cmd" } else { "sh" }.to_string()],
                env: Default::default(),
                harness: None,
                restart: Default::default(),
            }],
            harnesses: Vec::new(),
        }
    }

    /// Writes a session file as a previous daemon would have left it.
    /// Cheaper and more exact than running one: what is being tested is
    /// what the daemon does with the file, not the file format twice.
    fn record(panes: &[(PaneKind, &str)], checkout: &std::path::Path) {
        record_panes(
            panes
                .iter()
                .map(|(kind, title)| crate::store::SessionPane {
                    checkout_path: checkout.to_path_buf(),
                    kind: *kind,
                    title: title.to_string(),
                    template: Some(title.to_string()),
                    status: PaneStatus::Idle,
                    note: None,
                    harness_session_id: None,
                    harness: None,
                })
                .collect(),
        );
    }

    /// Writes the store as a previous daemon would have left it. Cheaper
    /// and more exact than running one: what is being tested is what the
    /// daemon does with the record, not the recording twice.
    fn record_panes(panes: Vec<crate::store::SessionPane>) {
        crate::store::Store::open()
            .unwrap()
            .save_panes(&panes)
            .unwrap();
    }

    fn persistent_agent_command() -> Vec<String> {
        if cfg!(windows) {
            vec!["cmd".into(), "/K".into(), "rem".into()]
        } else {
            vec![
                "sh".into(),
                "-c".into(),
                "sleep 30".into(),
                "argus-test".into(),
            ]
        }
    }

    fn daemon_with_claude_aliases(dir: &std::path::Path, names: &[&str]) -> Arc<Daemon> {
        daemon_running(dir, names, persistent_agent_command())
    }

    /// Like [`daemon_with_claude_aliases`], but the store is the temp config
    /// directory's — so [`record_agents`] is what a restart reads. Caller
    /// holds [`with_temp_config`].
    fn daemon_with_claude_aliases_for_restore(
        dir: &std::path::Path,
        names: &[&str],
    ) -> Arc<Daemon> {
        Daemon::with_store(
            running_config(dir, names, persistent_agent_command()),
            crate::store::Store::open().unwrap(),
        )
    }

    fn daemon_running(dir: &std::path::Path, names: &[&str], cmd: Vec<String>) -> Arc<Daemon> {
        // In-memory: these daemons live for whole seconds while a pane
        // starts, and [`Store::open`] would hold the process-global
        // `runtime.db` the tests running beside this one also open.
        Daemon::with_store(
            running_config(dir, names, cmd),
            crate::store::Store::in_memory()
                .expect("an in-memory runtime store needs nothing that can fail"),
        )
    }

    fn running_config(dir: &std::path::Path, names: &[&str], cmd: Vec<String>) -> ConfigFile {
        ConfigFile {
            workspaces: Vec::new(),
            projects: vec![ProjectConfig {
                name: "proj".into(),
                root: None,
                repos: vec![dir.to_string_lossy().to_string()],
                workspace: None,
                ..Default::default()
            }],
            agents: names
                .iter()
                .map(|name| AgentConfig {
                    name: (*name).into(),
                    cmd: cmd.clone(),
                    env: Default::default(),
                    harness: Some("claude".into()),
                    restart: Default::default(),
                })
                .collect(),
            harnesses: Vec::new(),
        }
    }

    fn record_agents(checkout: &std::path::Path, agents: &[(&str, Option<&str>)]) {
        record_panes(
            agents
                .iter()
                .map(|(template, session_id)| crate::store::SessionPane {
                    checkout_path: checkout.to_path_buf(),
                    kind: PaneKind::Agent,
                    title: (*template).into(),
                    template: Some((*template).into()),
                    status: PaneStatus::Idle,
                    note: None,
                    harness_session_id: session_id.map(str::to_string),
                    harness: None,
                })
                .collect(),
        );
    }

    fn close_all(d: &Daemon) {
        for p in &d.snapshot()[0].repositories[0].checkouts[0].panes {
            let _ = d.close_pane(p.id);
        }
    }

    fn saved_panes() -> Vec<crate::store::SessionPane> {
        crate::store::Store::open().unwrap().panes().unwrap()
    }

    #[test]
    fn nothing_recorded_means_nothing_restored() {
        let dir = tempfile::tempdir().unwrap();
        with_temp_config(|_| {
            let d = daemon_for_restore(dir.path());
            d.restore_session();
            assert!(d.snapshot()[0].repositories[0].checkouts[0]
                .panes
                .is_empty());
        });
    }

    #[tokio::test]
    async fn what_is_running_is_written_down() {
        let dir = tempfile::tempdir().unwrap();
        with_temp_config(|_| {
            let d = daemon_for_restore(dir.path());
            let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;
            d.spawn_shell(checkout).unwrap();

            let saved = saved_panes();
            assert_eq!(saved.len(), 1);
            assert_eq!(saved[0].kind, PaneKind::Shell);

            close_all(&d);
        });
    }

    #[tokio::test]
    async fn what_was_running_comes_back_after_a_restart() {
        // The point of the feature: a reboot should not cost you the panes
        // you had open.
        let dir = tempfile::tempdir().unwrap();
        with_temp_config(|_| {
            record(
                &[(PaneKind::Shell, "shell"), (PaneKind::Agent, "test-agent")],
                dir.path(),
            );

            let d = daemon_for_restore(dir.path());
            d.restore_session();

            let kinds: Vec<PaneKind> = d.snapshot()[0].repositories[0].checkouts[0]
                .panes
                .iter()
                .map(|p| p.kind)
                .collect();
            assert_eq!(kinds.len(), 2, "both panes came back: {kinds:?}");
            assert!(kinds.contains(&PaneKind::Shell));
            assert!(kinds.contains(&PaneKind::Agent));

            close_all(&d);
        });
    }

    #[tokio::test]
    async fn agent_status_and_note_survive_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        with_temp_config(|_| {
            record_panes(vec![crate::store::SessionPane {
                checkout_path: dir.path().to_path_buf(),
                kind: PaneKind::Agent,
                title: "review parser".to_string(),
                template: Some("test-agent".to_string()),
                status: PaneStatus::NeedsReview,
                note: Some("ready to inspect".to_string()),
                harness_session_id: None,
                harness: None,
            }]);

            let d = daemon_for_restore(dir.path());
            d.restore_session();

            let pane = &d.snapshot()[0].repositories[0].checkouts[0].panes[0];
            assert_eq!(pane.status, PaneStatus::NeedsReview);
            assert_eq!(pane.note.as_deref(), Some("ready to inspect"));
            close_all(&d);
        });
    }

    #[tokio::test]
    async fn an_agent_comes_back_as_the_template_it_was() {
        // The title is how a restored agent knows what to launch.
        let dir = tempfile::tempdir().unwrap();
        with_temp_config(|_| {
            record(&[(PaneKind::Agent, "test-agent")], dir.path());

            let d = daemon_for_restore(dir.path());
            d.restore_session();

            assert_eq!(
                d.snapshot()[0].repositories[0].checkouts[0].panes[0].title,
                "test-agent"
            );
            close_all(&d);
        });
    }

    #[tokio::test]
    async fn an_agent_that_renamed_itself_restores_its_display_title() {
        // Regression: an agent is spawned by template name, and a renamed
        // pane's title is no longer that. Restoring by title would look up
        // a template called "fixing the pty deadlock" and find nothing.
        let dir = tempfile::tempdir().unwrap();
        with_temp_config(|_| {
            record_panes(vec![crate::store::SessionPane {
                checkout_path: dir.path().to_path_buf(),
                kind: PaneKind::Agent,
                title: "fixing the pty deadlock".to_string(),
                template: Some("test-agent".to_string()),
                status: PaneStatus::Idle,
                note: None,
                harness_session_id: None,
                harness: None,
            }]);

            let d = daemon_for_restore(dir.path());
            d.restore_session();

            let panes = &d.snapshot()[0].repositories[0].checkouts[0].panes;
            assert_eq!(panes.len(), 1, "the renamed agent should be back");
            assert_eq!(
                panes[0].title, "fixing the pty deadlock",
                "its separately persisted display title should be restored"
            );

            close_all(&d);
        });
    }

    #[test]
    fn an_agent_whose_template_is_gone_costs_only_that_pane() {
        // Templates come from config, which changes between runs.
        let dir = tempfile::tempdir().unwrap();
        with_temp_config(|_| {
            record(&[(PaneKind::Agent, "no-such-template")], dir.path());

            let d = daemon_for_restore(dir.path());
            d.restore_session();

            assert!(
                d.snapshot()[0].repositories[0].checkouts[0]
                    .panes
                    .is_empty(),
                "skipped, not fatal"
            );
        });
    }

    #[test]
    fn an_editor_is_never_restored() {
        // It belonged to a floating window that no longer exists.
        let dir = tempfile::tempdir().unwrap();
        with_temp_config(|_| {
            record(&[(PaneKind::Editor, "a.rs")], dir.path());

            let d = daemon_for_restore(dir.path());
            d.restore_session();

            assert!(d.snapshot()[0].repositories[0].checkouts[0]
                .panes
                .is_empty());
        });
    }

    #[test]
    fn the_escape_hatch_starts_clean() {
        // For the case where the restore is itself the problem.
        let dir = tempfile::tempdir().unwrap();
        with_temp_config(|_| {
            record(&[(PaneKind::Shell, "shell")], dir.path());

            std::env::set_var(crate::store::NO_RESTORE, "1");
            let d = daemon_for_restore(dir.path());
            d.restore_session();
            std::env::remove_var(crate::store::NO_RESTORE);

            assert!(d.snapshot()[0].repositories[0].checkouts[0]
                .panes
                .is_empty());
        });
    }

    #[tokio::test]
    async fn a_session_file_from_an_older_argus_is_restored_from() {
        // The upgrade path: what the previous version left behind is
        // imported when the store first opens, and restores like anything
        // else recorded in it.
        let dir = tempfile::tempdir().unwrap();
        with_temp_config(|cfg| {
            std::fs::write(
                cfg.join("session.json"),
                format!(
                    r#"{{"panes":[{{"checkout_path":{:?},"kind":"Shell","title":"shell"}}]}}"#,
                    dir.path().to_string_lossy().replace('\\', "/")
                ),
            )
            .unwrap();

            let d = Daemon::with_store(
                restore_config(dir.path()),
                crate::store::Store::open().unwrap(),
            );
            d.restore_session();

            assert_eq!(
                d.snapshot()[0].repositories[0].checkouts[0].panes.len(),
                1,
                "the imported pane should have come back"
            );
            close_all(&d);
        });
    }

    #[tokio::test]
    async fn a_daemon_without_a_store_on_disk_writes_nothing() {
        // Every test builds a daemon; none of them may write over the real
        // user's state. `Daemon::new` is what guarantees that, by handing
        // one a store that lives and dies with the process.
        let dir = tempfile::tempdir().unwrap();
        with_temp_config(|cfg| {
            let d = Daemon::new(restore_config(dir.path()));
            let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;
            d.spawn_shell(checkout).unwrap();
            close_all(&d);

            assert!(
                !cfg.join("runtime.db").exists(),
                "a daemon persists only through the store it was given"
            );
        });
    }

    #[test]
    fn a_running_test_daemon_does_not_open_the_config_store() {
        // `daemon_with_claude_aliases` used to call `Store::open`, so every
        // pane-start test held the process-global `runtime.db` for seconds
        // and the tests running beside it failed with SQLITE_BUSY.
        with_temp_config(|cfg| {
            let dir = tempfile::tempdir().unwrap();
            let _d = daemon_with_claude_aliases(dir.path(), &["claude"]);
            assert!(
                !cfg.join("runtime.db").exists(),
                "holding the shared store is how parallel tests lose to SQLITE_BUSY"
            );
        });
    }

    #[tokio::test]
    async fn a_pane_you_closed_does_not_come_back() {
        // The file follows the tree, so closing one forgets it.
        let dir = tempfile::tempdir().unwrap();
        with_temp_config(|_| {
            let d = daemon_for_restore(dir.path());
            let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;
            let pane = d.spawn_shell(checkout).unwrap();
            let _ = d.close_pane(pane);

            assert!(saved_panes().is_empty());
        });
    }

    #[tokio::test]
    async fn an_exited_pane_is_not_recorded_as_running() {
        let dir = tempfile::tempdir().unwrap();
        with_temp_config(|_| {
            let d = daemon_for_restore(dir.path());
            let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;
            let pane = d.spawn_shell(checkout).unwrap();

            d.mark_pane_exited(pane, Some(0));

            assert!(saved_panes().is_empty());
            close_all(&d);
        });
    }

    #[tokio::test]
    async fn a_pane_in_a_worktree_comes_back_too() {
        // Regression: a worktree is discovered from git by a poll that has
        // not run yet when restore does, so a pane in one looked like a
        // pane whose checkout was gone — and was silently dropped.
        let dir = tempfile::tempdir().unwrap();
        let repo = real_repo(dir.path());
        let worktree = dir.path().join("wt-feature");
        repo.worktree("feature", &worktree, None).unwrap();

        with_temp_config(|_| {
            record(&[(PaneKind::Shell, "shell")], &worktree);

            let d = daemon_for_restore(dir.path());
            d.restore_session();

            let checkouts = d.snapshot().remove(0).repositories.remove(0).checkouts;
            let restored = checkouts
                .iter()
                .find(|c| same_path(std::path::Path::new(&c.path), &worktree))
                .expect("the worktree should have joined the tree");
            assert_eq!(restored.panes.len(), 1, "its pane should have come back");

            for c in &checkouts {
                for p in &c.panes {
                    let _ = d.close_pane(p.id);
                }
            }
        });
    }

    // --- resuming a conversation --------------------------------------------

    /// How many panes are currently holding a reopened conversation. Not
    /// in the snapshot: it is bookkeeping for the fallback, not something
    /// a row shows.
    fn resuming_panes(d: &Daemon) -> usize {
        d.inner
            .lock()
            .unwrap()
            .projects
            .iter()
            .flat_map(|p| p.repositories.iter())
            .flat_map(|r| r.checkouts.iter())
            .flat_map(|c| c.panes.iter())
            .filter(|p| p.resumed.is_some())
            .count()
    }

    #[tokio::test]
    async fn only_one_pane_per_checkout_reopens_the_conversation() {
        // `--continue` means "the last conversation in this directory", so
        // two of them would land on the same session and write over each
        // other. Both agents come back; only one carries the old thread.
        let dir = tempfile::tempdir().unwrap();
        with_temp_config(|_| {
            record(
                &[(PaneKind::Agent, "claude"), (PaneKind::Agent, "claude")],
                dir.path(),
            );

            let d = persistent(fake_claude_config(dir.path()));
            d.restore_session();

            let panes = d
                .snapshot()
                .remove(0)
                .repositories
                .remove(0)
                .checkouts
                .remove(0)
                .panes;
            assert_eq!(panes.len(), 2, "both agents came back");
            assert_eq!(resuming_panes(&d), 1, "one conversation, one claimant");

            close_all(&d);
        });
    }

    #[test]
    fn a_new_agent_is_a_new_conversation() {
        // Resume arguments belong to restore alone: asking for an agent
        // means asking for one, not for the last one back.
        let (args, resuming) = agent_args(
            &["--model".to_string(), "opus".to_string()],
            &["--continue".to_string()],
            &["--resume".to_string(), "{session_id}".to_string()],
            Start::Fresh,
            None,
        );
        assert_eq!(args, vec!["--model", "opus"]);
        assert!(!resuming);
    }

    #[test]
    fn a_restored_agent_is_asked_to_continue_where_it_left_off() {
        let (args, resuming) = agent_args(
            &["--model".to_string(), "opus".to_string()],
            &["--continue".to_string()],
            &["--resume".to_string(), "{session_id}".to_string()],
            Start::Resuming,
            None,
        );
        assert_eq!(
            args,
            vec!["--model", "opus", "--continue"],
            "after the template's own flags, which still apply"
        );
        assert!(resuming);
    }

    #[tokio::test]
    async fn distinct_exact_ids_in_one_checkout_restore_independently() {
        let dir = tempfile::tempdir().unwrap();
        with_temp_config(|_| {
            record_agents(
                dir.path(),
                &[("first", Some("session-a")), ("second", Some("session-b"))],
            );
            let d = daemon_with_claude_aliases_for_restore(dir.path(), &["first", "second"]);
            d.restore_session();

            assert_eq!(resuming_panes(&d), 2, "exact IDs need no broad claim guard");
            let mut ids: Vec<_> = d
                .session_panes()
                .into_iter()
                .filter_map(|pane| pane.harness_session_id)
                .collect();
            ids.sort();
            assert_eq!(ids, ["session-a", "session-b"]);
            close_all(&d);
        });
    }

    #[tokio::test]
    async fn aliases_of_one_harness_share_the_legacy_broad_claim() {
        let dir = tempfile::tempdir().unwrap();
        with_temp_config(|_| {
            record_agents(dir.path(), &[("first", None), ("second", None)]);
            let d = daemon_with_claude_aliases_for_restore(dir.path(), &["first", "second"]);
            d.restore_session();

            assert_eq!(d.snapshot()[0].repositories[0].checkouts[0].panes.len(), 2);
            assert_eq!(
                resuming_panes(&d),
                1,
                "claim is checkout plus harness, not template"
            );
            close_all(&d);
        });
    }

    #[test]
    fn exact_resume_expands_the_id_as_argv_not_shell_text() {
        let (args, resuming) = agent_args(
            &["--model".to_string(), "opus".to_string()],
            &["--continue".to_string()],
            &["--resume".to_string(), "{session_id}".to_string()],
            Start::Resuming,
            Some("session with spaces;still-one-arg"),
        );
        assert_eq!(
            args,
            [
                "--model",
                "opus",
                "--resume",
                "session with spaces;still-one-arg"
            ]
        );
        assert!(resuming);
    }

    #[test]
    fn a_harness_that_cannot_resume_restores_the_old_way() {
        // Nothing to append, and nothing for a failed start to fall back
        // from — the pane must not be treated as a resume that went wrong.
        let (args, resuming) = agent_args(&["-q".to_string()], &[], &[], Start::Resuming, None);
        assert_eq!(args, vec!["-q"]);
        assert!(!resuming);
    }

    #[test]
    fn an_immediate_refusal_reads_as_nothing_to_resume() {
        assert!(nothing_to_resume(Some(1), Duration::from_millis(300)));
        assert!(
            nothing_to_resume(None, Duration::from_millis(300)),
            "no exit code at all is still a start that did not take"
        );
    }

    #[test]
    fn a_restored_agent_that_ran_is_not_a_failed_resume() {
        assert!(
            !nothing_to_resume(Some(0), Duration::from_millis(300)),
            "these CLIs leave cleanly when you quit them"
        );
        assert!(
            !nothing_to_resume(Some(1), RESUME_GRACE + Duration::from_secs(1)),
            "it was up long enough to have been the conversation"
        );
    }

    #[tokio::test]
    async fn a_resume_with_nothing_behind_it_comes_back_as_a_fresh_agent() {
        // The cost of guessing wrong about what a CLI can continue: the
        // user gets the agent they had, not a dead row where one should be.
        let dir = tempfile::tempdir().unwrap();
        let d = daemon_with_fake_claude(dir.path());
        let pane = d
            .start_agent(only_checkout(&d), "claude", Start::Resuming, None)
            .unwrap();

        d.mark_pane_exited(pane, Some(1));

        let panes = d
            .snapshot()
            .remove(0)
            .repositories
            .remove(0)
            .checkouts
            .remove(0)
            .panes;
        assert_eq!(panes.len(), 1, "the dead row goes, it does not pile up");
        assert_ne!(panes[0].id, pane, "a new agent took its place");
        assert_eq!(panes[0].status, PaneStatus::Idle);

        close_all(&d);
    }

    #[tokio::test]
    async fn an_agent_that_starts_and_fails_again_is_left_alone() {
        // One retry, never a loop: the replacement is a plain agent, so its
        // own failure is just a failure.
        let dir = tempfile::tempdir().unwrap();
        let d = daemon_with_fake_claude(dir.path());
        let pane = d
            .start_agent(only_checkout(&d), "claude", Start::Resuming, None)
            .unwrap();

        d.mark_pane_exited(pane, Some(1));
        let replacement = d
            .snapshot()
            .remove(0)
            .repositories
            .remove(0)
            .checkouts
            .remove(0)
            .panes[0]
            .id;
        d.mark_pane_exited(replacement, Some(1));

        let panes = d
            .snapshot()
            .remove(0)
            .repositories
            .remove(0)
            .checkouts
            .remove(0)
            .panes;
        assert_eq!(panes.len(), 1);
        assert_eq!(panes[0].id, replacement, "no third attempt");
        assert_eq!(panes[0].status, PaneStatus::Exited { code: Some(1) });

        close_all(&d);
    }

    #[tokio::test]
    async fn quitting_a_restored_agent_leaves_it_quit() {
        let dir = tempfile::tempdir().unwrap();
        let d = daemon_with_fake_claude(dir.path());
        let pane = d
            .start_agent(only_checkout(&d), "claude", Start::Resuming, None)
            .unwrap();

        d.mark_pane_exited(pane, Some(0));

        let panes = d
            .snapshot()
            .remove(0)
            .repositories
            .remove(0)
            .checkouts
            .remove(0)
            .panes;
        assert_eq!(panes.len(), 1);
        assert_eq!(panes[0].id, pane, "still the pane the user closed");
        assert_eq!(panes[0].status, PaneStatus::Exited { code: Some(0) });

        close_all(&d);
    }
}
