use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use argus_protocol::{
    Cell, CheckoutId, CheckoutInfo, GitStatus, IdGen, PaneId, PaneInfo, PaneKind, PaneStatus,
    ProjectId, ProjectInfo, RepositoryId, RepositoryInfo, ServerMsg, WorkspaceId, WorkspaceInfo,
};
use tokio::sync::broadcast;

use crate::config::{self, AgentConfig, ConfigFile};
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
    /// Stable conversation identity reported by the harness. Also the pane's
    /// owner: reports carrying a different session belong to `children`.
    harness_session_id: Option<String>,
    /// Agents reporting through this pane that do not own it — a CLI started
    /// from inside the pane's own agent inherits the hook environment, so
    /// without this its every turn would rewrite its parent's row.
    children: Vec<ChildAgent>,
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

/// How many children a pane lists. A parent fanning out dozens of one-shot
/// CLIs should not push its own row off the column.
const MAX_CHILDREN: usize = 8;

/// How long a child may go without reporting before it stops being listed.
/// Long enough that a single slow tool call is not mistaken for death —
/// the common endings are handled without waiting for it, so this is only
/// the backstop for a child that vanishes silently.
const CHILD_SILENCE: Duration = Duration::from_secs(600);

/// A pane started with its harness's resume arguments, and what to start
/// instead if that turns out to have been a lie.
///
/// The CLIs answer "there is nothing to continue" by refusing to start:
/// `claude --continue` in a checkout that has never held a conversation
/// prints a line and exits. Restoring a pane must not leave a dead row
/// where an agent should be, so an immediate failure is taken as that
/// answer and the pane comes back as a plain new agent.
struct Resumed {
    checkout: CheckoutId,
    template: String,
    at: std::time::Instant,
}

/// How long after a resumed spawn a failure still reads as "there was
/// nothing to resume" rather than as the user quitting.
///
/// Long enough for a node CLI to start and give up, short enough that
/// quitting an agent you did not want back — which restore has just put in
/// front of you — is not misread as one. A false positive costs a fresh
/// agent pane, which is exactly what restore did before it could resume at
/// all; a false negative costs a dead row.
const RESUME_GRACE: Duration = Duration::from_secs(5);

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
    /// Last polled git status, or `None` before the first poll has run.
    /// Cached rather than read on demand because `snapshot` is taken under
    /// the daemon's one lock, and `git::status` is milliseconds of blocking
    /// I/O per checkout — long enough to be felt as typing lag, since every
    /// keystroke needs that same lock to find its pane (§4 Level 2).
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
    /// straight back (see `config::load_excluded_repos`).
    excluded: Vec<PathBuf>,
}

pub struct Daemon {
    inner: StdMutex<Inner>,
    /// SessionStart can fire after the child is spawned but before its pane
    /// is inserted into `inner`. Keep that first identity until insertion.
    starting_agents: StdMutex<HashMap<PaneId, Option<String>>>,
    tree_tx: broadcast::Sender<Vec<ProjectInfo>>,
    workspaces_tx: broadcast::Sender<Vec<WorkspaceInfo>>,
    templates: Vec<AgentConfig>,
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
    /// Off unless `main` turns it on. A daemon built in a test must not
    /// write over the real user's session file, and every structural
    /// change would otherwise do exactly that.
    persist: std::sync::atomic::AtomicBool,
}

type PaneSubscription = (
    u16,
    u16,
    Vec<Vec<Cell>>,
    argus_protocol::Cursor,
    broadcast::Receiver<ServerMsg>,
);

impl Daemon {
    pub fn new(config: ConfigFile) -> Arc<Self> {
        let mut ids = IdGen::default();

        // Workspaces come from three places, in this order: the built-in
        // default (always present, so a config that predates workspaces
        // keeps working), any `[[workspace]]` blocks, and any name a
        // project refers to without declaring. Declaring is therefore
        // optional — `workspace = "x"` on a project is enough to create it.
        let mut workspaces: Vec<Workspace> = Vec::new();
        let intern = |ws: &mut Vec<Workspace>, ids: &mut IdGen, name: &str| -> WorkspaceId {
            if let Some(w) = ws.iter().find(|w| w.name == name) {
                return w.id;
            }
            let id = WorkspaceId(ids.alloc());
            ws.push(Workspace {
                id,
                name: name.to_string(),
            });
            id
        };
        let default_ws = intern(&mut workspaces, &mut ids, config::DEFAULT_WORKSPACE);
        for w in &config.workspaces {
            intern(&mut workspaces, &mut ids, &w.name);
        }

        let excluded = config::load_excluded_repos();
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
                if let Some(root) = &root {
                    let found = retain_included(&excluded, crate::git::discover_repositories(root));
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
        let open = config::load_open_workspace()
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
            templates,
            harnesses,
            hook_port: std::sync::atomic::AtomicU16::new(0),
            hook_token: gen_token(),
            restoring: std::sync::atomic::AtomicBool::new(false),
            persist: std::sync::atomic::AtomicBool::new(false),
        });
        // Checkout rows are named after the branch occupying them, and that
        // name now comes from the cache. Filling it here rather than waiting
        // for the first poll means the first client to connect gets branch
        // names rather than directory names.
        daemon.refresh_git_status();
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
        self.templates.iter().map(|t| t.name.clone()).collect()
    }

    /// The tree as clients see it: only the open workspace's projects.
    /// Panes in the other workspaces are still alive and still updating —
    /// their rollups show up in [`Daemon::workspaces`] so a working agent
    /// somewhere you are not looking is still visible.
    pub fn snapshot(&self) -> Vec<ProjectInfo> {
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
                                }
                            })
                            .collect(),
                    })
                    .collect(),
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
        config::save_open_workspace(&name);
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
        config::append_workspace(name)?;

        {
            let mut inner = self.inner.lock().unwrap();
            let id = WorkspaceId(inner.ids.alloc());
            inner.workspaces.push(Workspace {
                id,
                name: name.to_string(),
            });
            inner.open = id;
        }
        config::save_open_workspace(name);
        self.broadcast_tree();
        self.broadcast_workspaces();
        Ok(())
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

    /// What is running, in a form that survives ids being reissued.
    fn session(&self) -> crate::session::Session {
        let inner = self.inner.lock().unwrap();
        crate::session::Session {
            panes: inner
                .projects
                .iter()
                .flat_map(|p| p.repositories.iter())
                .flat_map(|r| r.checkouts.iter())
                .flat_map(|c| {
                    c.panes
                        .iter()
                        .filter(|pane| !matches!(pane.status, PaneStatus::Exited { .. }))
                        .map(|pane| crate::session::SessionPane {
                            checkout_path: c.path.clone(),
                            kind: pane.kind,
                            title: pane.title.clone(),
                            template: pane.template.clone(),
                            status: pane.status,
                            note: pane.note.clone(),
                            harness_session_id: pane.harness_session_id.clone(),
                        })
                })
                .collect(),
        }
    }

    /// Records the session from here on, and remembers this one. Only
    /// `main` calls it; everything else runs without touching disk.
    pub fn persist_session(&self) {
        self.persist
            .store(true, std::sync::atomic::Ordering::Relaxed);
        self.record_session();
    }

    fn record_session(&self) {
        // Half a restore is not a session worth remembering.
        let ord = std::sync::atomic::Ordering::Relaxed;
        if !self.persist.load(ord) || self.restoring.load(ord) {
            return;
        }
        crate::session::save(&self.session());
    }

    /// Starts again whatever was running when the daemon last stopped, and
    /// asks each agent CLI to reopen the conversation it had.
    ///
    /// Failures are per pane and never fatal: a template that has since
    /// stopped working should cost you that pane, not the whole session.
    pub fn restore_session(self: &Arc<Self>) -> bool {
        let Some(saved) = crate::session::load() else {
            return false;
        };
        if saved.panes.is_empty() {
            return true;
        }
        // Only primary checkouts come from the config; a worktree is
        // discovered from git by a poll that has not run yet. Without this,
        // every pane in a worktree looks like a pane whose checkout is
        // gone, and is dropped.
        self.reconcile_worktrees();
        let known = self.checkout_paths();
        let wanted: Vec<crate::session::SessionPane> = crate::session::restorable(&saved, &known)
            .cloned()
            .collect();
        if wanted.is_empty() {
            return true;
        }

        self.restoring
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let mut restored = 0usize;
        let mut claimed: Vec<(PathBuf, String)> = Vec::new();
        for pane in &wanted {
            let Some(checkout) = self.checkout_at(&pane.checkout_path) else {
                continue;
            };
            let result = match pane.kind {
                PaneKind::Agent => {
                    let session_id = pane
                        .harness_session_id
                        .as_deref()
                        .and_then(valid_session_id);
                    // Exact IDs are independent. Only old records need to
                    // claim a checkout's broad "last conversation" resume.
                    let start = if session_id.is_some() {
                        Start::Resuming
                    } else {
                        let harness = self
                            .templates
                            .iter()
                            .find(|template| template.name == pane.template())
                            .map(|template| self.harness_for(template).name);
                        let key = harness.map(|harness| (pane.checkout_path.clone(), harness));
                        if key.as_ref().is_some_and(|key| claimed.contains(key)) {
                            Start::Fresh
                        } else {
                            if let Some(key) = key {
                                claimed.push(key);
                            }
                            Start::Resuming
                        }
                    };
                    self.start_agent(checkout, pane.template(), start, session_id)
                }
                _ => self.spawn_shell(checkout),
            };
            match result {
                Ok(id) => {
                    self.set_pane_hook_status(id, pane.status, pane.note.clone());
                    restored += 1;
                }
                Err(e) => tracing::warn!(
                    "could not restore {} in {}: {e}",
                    pane.title,
                    pane.checkout_path.display()
                ),
            }
        }
        self.restoring
            .store(false, std::sync::atomic::Ordering::Relaxed);

        tracing::info!("restored {restored} of {} panes", wanted.len());
        self.broadcast_tree();
        true
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
                    status: PaneStatus::Idle,
                    note: None,
                    template: None,
                    children: Vec::new(),
                    harness_session_id: None,
                    resumed: None,
                    runtime,
                });
            }
        }
        self.broadcast_tree();
        Ok(id)
    }

    /// Opens `rel_path` (repo-relative) in the user's editor as a pane.
    pub fn spawn_editor(
        self: &Arc<Self>,
        checkout: CheckoutId,
        rel_path: &str,
        line: Option<u32>,
        external: bool,
        command: Option<&str>,
    ) -> anyhow::Result<PaneId> {
        let path = self.checkout_path(checkout)?;
        // Rejected here rather than trusted: `path` is spawned into a
        // command line, and a client is not the authority on what is
        // inside the checkout. A leading separator is not `is_absolute` on
        // Windows, so it is checked by hand rather than left to the platform.
        if rel_path.is_empty()
            || rel_path.starts_with(['/', '\\'])
            || std::path::Path::new(rel_path).is_absolute()
            || has_windows_drive_prefix(rel_path)
            || rel_path.split(['/', '\\']).any(|c| c == "..")
        {
            anyhow::bail!("not a path inside the checkout: {rel_path}");
        }

        let editor = match command.map(str::trim).filter(|c| !c.is_empty()) {
            Some(c) => c.to_string(),
            None => crate::editor::resolve(),
        };
        let argv = crate::editor::command(&editor, rel_path, line);
        let (program, args) = argv.split_first().expect("never empty");

        // A GUI editor cannot live in a pty whatever the client asked for:
        // it would be a blank pane whose child never speaks, which is
        // indistinguishable from a hung one.
        if external || crate::editor::is_gui(&editor) {
            // No pty and no pane: this editor brings its own window, and
            // Argus has nothing to draw for it. Detached so closing the
            // daemon doesn't take the user's editor with it.
            crate::command::detached(program)
                .args(args)
                .current_dir(&path)
                .spawn()
                .map_err(|e| anyhow::anyhow!("could not start {program}: {e}"))?;
            return Ok(PaneId(0));
        }

        let id = {
            let mut inner = self.inner.lock().unwrap();
            PaneId(inner.ids.alloc())
        };
        let daemon = self.clone();
        let runtime = PaneRuntime::spawn(
            id,
            &path,
            pty::Spawn::Program {
                program: program.clone(),
                args: args.to_vec(),
                env: Vec::new(),
            },
            move |code| daemon.mark_pane_exited(id, code),
        )?;

        {
            let mut inner = self.inner.lock().unwrap();
            if let Some(c) = find_checkout(&mut inner.projects, checkout) {
                c.panes.push(Pane {
                    id,
                    kind: PaneKind::Editor,
                    title: rel_path.rsplit('/').next().unwrap_or(rel_path).to_string(),
                    status: PaneStatus::Idle,
                    note: None,
                    template: None,
                    children: Vec::new(),
                    harness_session_id: None,
                    resumed: None,
                    runtime,
                });
            }
        }
        self.broadcast_tree();
        Ok(id)
    }

    /// Starts a new agent, and with it a new conversation. What the user
    /// gets from the template picker.
    pub fn spawn_agent(
        self: &Arc<Self>,
        checkout: CheckoutId,
        template_name: &str,
    ) -> anyhow::Result<PaneId> {
        self.start_agent(checkout, template_name, Start::Fresh, None)
    }

    fn start_agent(
        self: &Arc<Self>,
        checkout: CheckoutId,
        template_name: &str,
        start: Start,
        harness_session_id: Option<String>,
    ) -> anyhow::Result<PaneId> {
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
        self.starting_agents.lock().unwrap().insert(id, None);

        // Must land before the process starts: a harness reads its hook
        // config at its own startup, not on later file changes.
        let harness = self.harness_for(&template);
        let port = self.hook_port.load(std::sync::atomic::Ordering::Relaxed);
        if port != 0 {
            if let Err(e) = harness.install(&path, id, port, &self.hook_token) {
                tracing::warn!(
                    "failed to install {} hooks in {}: {e}",
                    harness.name,
                    path.display()
                );
            }
        }

        // The template's own env wins: a user who set one of these by hand
        // meant it.
        let mut env = crate::harness::env(id, port, &self.hook_token);
        env.retain(|(k, _)| !template.env.contains_key(k));
        env.extend(template.env.clone());

        let (args, resuming) = agent_args(
            rest,
            &harness.resume,
            &harness.resume_id,
            start,
            harness_session_id.as_deref(),
        );

        let spec = pty::Spawn::Program {
            program: program.clone(),
            args,
            env,
        };

        let daemon = self.clone();
        let runtime = match PaneRuntime::spawn(id, &path, spec, move |code| {
            daemon.mark_pane_exited(id, code);
        }) {
            Ok(runtime) => runtime,
            Err(error) => {
                self.starting_agents.lock().unwrap().remove(&id);
                return Err(error);
            }
        };

        {
            // Lock in this order everywhere that spans the pre-spawn mailbox
            // and pane tree, so an arriving hook cannot slip between them.
            let mut starting = self.starting_agents.lock().unwrap();
            let mut inner = self.inner.lock().unwrap();
            let reported_session_id = starting.remove(&id).flatten();
            if let Some(c) = find_checkout(&mut inner.projects, checkout) {
                c.panes.push(Pane {
                    id,
                    kind: PaneKind::Agent,
                    title: template.name.clone(),
                    status: PaneStatus::Idle,
                    note: None,
                    template: Some(template.name.clone()),
                    children: Vec::new(),
                    harness_session_id: reported_session_id.or(harness_session_id),
                    resumed: resuming.then(|| Resumed {
                        checkout,
                        template: template.name.clone(),
                        at: std::time::Instant::now(),
                    }),
                    runtime,
                });
            }
        }
        self.broadcast_tree();
        Ok(id)
    }

    pub fn mark_pane_exited(self: &Arc<Self>, pane: PaneId, code: Option<i32>) {
        let retry = {
            let mut inner = self.inner.lock().unwrap();
            match find_pane(&mut inner.projects, pane) {
                Some(p) => {
                    p.status = PaneStatus::Exited { code };
                    p.note = None;
                    p.resumed
                        .take()
                        .filter(|r| nothing_to_resume(code, r.at.elapsed()))
                }
                None => None,
            }
        };

        if let Some(r) = retry {
            // The CLI has just told us there was no conversation to
            // continue. Take the dead row out rather than leaving it beside
            // its replacement, and give the user the agent they had.
            tracing::info!(
                "{} had nothing to resume in this checkout; starting it fresh",
                r.template
            );
            let _ = self.remove_pane(pane);
            if let Err(e) = self.start_agent(r.checkout, &r.template, Start::Fresh, None) {
                tracing::warn!("could not start {} after a failed resume: {e}", r.template);
            }
            return;
        }

        self.broadcast_tree();
    }

    /// Drops a pane from the tree without touching the checkout's managed
    /// hooks — for a pane being replaced in place, where an agent is about
    /// to take its seat and would only have to write them back.
    fn remove_pane(&self, pane: PaneId) -> Option<Pane> {
        let mut inner = self.inner.lock().unwrap();
        remove_pane_with_checkout(&mut inner.projects, pane).map(|(p, _)| p)
    }

    /// Kills the pane's process (best-effort — it may already have exited)
    /// and removes it from the tree entirely, so a closed pane actually
    /// disappears instead of lingering as a dead row the user can't clear.
    pub fn close_pane(&self, pane: PaneId) -> anyhow::Result<()> {
        let (removed, orphaned_checkout) = {
            let mut inner = self.inner.lock().unwrap();
            let taken = remove_pane_with_checkout(&mut inner.projects, pane);
            // Managed hooks belong to the checkout, not the pane, so they
            // come out only once the last agent there is gone — closing one
            // of two agent panes must not blind the other.
            let orphaned = taken
                .as_ref()
                .map(|(_, path)| path.clone())
                .filter(|path| !checkout_has_agent(&inner.projects, path));
            (taken.map(|(p, _)| p), orphaned)
        };
        let removed = removed.ok_or_else(|| anyhow::anyhow!("no such pane"))?;
        let _ = removed.runtime.kill();
        if let Some(path) = orphaned_checkout {
            for h in &self.harnesses {
                if let Err(e) = h.uninstall(&path) {
                    tracing::warn!(
                        "failed to clear {} hooks in {}: {e}",
                        h.name,
                        path.display()
                    );
                }
            }
        }
        self.broadcast_tree();
        Ok(())
    }

    pub fn write_pane(&self, pane: PaneId, bytes: &[u8]) -> anyhow::Result<()> {
        let input = {
            let inner = self.inner.lock().unwrap();
            let pane = find_pane_ref(&inner.projects, pane)
                .ok_or_else(|| anyhow::anyhow!("no such pane"))?;
            pane.runtime.input()
        };
        input.write(bytes)
    }

    pub fn paste_pane(&self, pane: PaneId, text: &str) -> anyhow::Result<()> {
        let input = {
            let inner = self.inner.lock().unwrap();
            let pane = find_pane_ref(&inner.projects, pane)
                .ok_or_else(|| anyhow::anyhow!("no such pane"))?;
            pane.runtime.input()
        };
        input.paste(text.as_bytes())
    }

    pub fn resize_pane(&self, pane: PaneId, rows: u16, cols: u16) -> anyhow::Result<()> {
        let inner = self.inner.lock().unwrap();
        let p =
            find_pane_ref(&inner.projects, pane).ok_or_else(|| anyhow::anyhow!("no such pane"))?;
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
        let p =
            find_pane_ref(&inner.projects, pane).ok_or_else(|| anyhow::anyhow!("no such pane"))?;
        Ok(p.runtime.snapshot_and_subscribe())
    }

    /// Binds the loopback HTTP status receiver hook commands POST to (see
    /// `hooks::install_claude_hooks`) and starts serving it in the
    /// background. The bind itself is synchronous so `hook_port` is set
    /// before the daemon's client socket starts accepting — no window where
    /// a client could spawn an agent whose hooks point nowhere.
    pub fn start_hook_server(self: &Arc<Self>) -> anyhow::Result<()> {
        let std_listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        std_listener.set_nonblocking(true)?;
        let port = std_listener.local_addr()?.port();
        self.hook_port
            .store(port, std::sync::atomic::Ordering::Relaxed);
        let listener = tokio::net::TcpListener::from_std(std_listener)?;

        let daemon = self.clone();
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let daemon = daemon.clone();
                tokio::spawn(async move {
                    let _ = handle_hook_request(stream, daemon).await;
                });
            }
        });
        Ok(())
    }

    /// Applies a hook-reported status, unless the pane has already exited —
    /// a hook firing after the process died (e.g. `Stop` racing a crash) is
    /// stale and shouldn't resurrect a dead pane's row.
    /// Applies a hook report to whichever agent sent it.
    ///
    /// Every report a harness makes carries the session it came from, so a
    /// CLI spawned inside a pane — which inherits the pane's hook URL and
    /// token and cannot be stopped from calling home — lands in that pane's
    /// child list instead of overwriting the row. The agent Argus started
    /// stays the authority on what the pane says.
    fn report_pane_status(
        &self,
        pane: PaneId,
        reporter: Option<&str>,
        status: PaneStatus,
        note: Option<String>,
    ) {
        match self.child_of(pane, reporter) {
            Some(session) => self.set_child_status(pane, &session, status, note),
            None => self.set_pane_hook_status(pane, status, note),
        }
    }

    fn report_pane_title(&self, pane: PaneId, reporter: Option<&str>, title: &str) {
        match self.child_of(pane, reporter) {
            Some(session) => self.set_child_label(pane, &session, title),
            None => self.set_pane_title(pane, title),
        }
    }

    /// The reporting session, when it is not the one that owns the pane.
    /// A report with no session at all is the pane's own: only a harness
    /// event carries one, and `argus-hook status` typed by hand has none.
    fn child_of(&self, pane: PaneId, reporter: Option<&str>) -> Option<String> {
        let reporter = reporter?;
        let inner = self.inner.lock().unwrap();
        let owner = find_pane_ref(&inner.projects, pane)?
            .harness_session_id
            .as_deref()?;
        (owner != reporter).then(|| reporter.to_string())
    }

    fn with_child(&self, pane: PaneId, session: &str, edit: impl FnOnce(&mut ChildAgent)) {
        {
            let mut inner = self.inner.lock().unwrap();
            let Some(p) = find_pane(&mut inner.projects, pane) else {
                return;
            };
            if matches!(p.status, PaneStatus::Exited { .. }) {
                return;
            }
            match p.children.iter_mut().find(|c| c.session_id == session) {
                Some(child) => {
                    child.at = std::time::Instant::now();
                    edit(child)
                }
                None => {
                    let mut child = ChildAgent {
                        session_id: session.to_string(),
                        label: None,
                        status: PaneStatus::Working,
                        note: None,
                        at: std::time::Instant::now(),
                    };
                    edit(&mut child);
                    p.children.push(child);
                    if p.children.len() > MAX_CHILDREN {
                        p.children.remove(0);
                    }
                }
            }
            // A child that has gone idle is no longer something running
            // under this row, so it stops being listed under it.
            p.children
                .retain(|c| !matches!(c.status, PaneStatus::Idle | PaneStatus::Exited { .. }));
        }
        self.broadcast_tree();
    }

    fn set_child_status(
        &self,
        pane: PaneId,
        session: &str,
        status: PaneStatus,
        note: Option<String>,
    ) {
        let note = note.map(|n| clean_title(&n)).filter(|n| !n.is_empty());
        self.with_child(pane, session, |child| {
            child.status = status;
            child.note = note;
        });
    }

    fn set_child_label(&self, pane: PaneId, session: &str, title: &str) {
        let label = clean_title(title);
        if label.is_empty() {
            return;
        }
        self.with_child(pane, session, |child| child.label = Some(label));
    }

    /// Forgets children that have gone quiet. A child says so when it
    /// finishes and is cleared with its parent's turn either way, so this
    /// only catches the one that was killed, crashed, or lost its harness
    /// mid-turn — otherwise its row would sit there claiming to be working
    /// for as long as the parent kept going.
    fn drop_silent_children(&self) {
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            let mut changed = false;
            for p in all_panes(&mut inner.projects) {
                let before = p.children.len();
                p.children.retain(|c| c.at.elapsed() < CHILD_SILENCE);
                changed |= p.children.len() != before;
            }
            changed
        };
        if changed {
            self.broadcast_tree();
        }
    }

    fn set_pane_hook_status(&self, pane: PaneId, status: PaneStatus, note: Option<String>) {
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            match find_pane(&mut inner.projects, pane) {
                Some(p) if !is_stale_report(p.status, status) => {
                    // A note explains one state; the report that leaves that
                    // state takes it away with it, so a stale "waiting for
                    // the db password" can't sit under a working row.
                    let note = note.map(|n| clean_title(&n)).filter(|n| !n.is_empty());
                    let mut changed = p.status != status || p.note != note;
                    p.status = status;
                    p.note = note;
                    // The turn that spawned them is over, so anything still
                    // listed under this row has finished without saying so.
                    // A background agent outliving the turn is not lost by
                    // this: its next report lists it again.
                    if status == PaneStatus::Idle && !p.children.is_empty() {
                        p.children.clear();
                        changed = true;
                    }
                    changed
                }
                _ => false,
            }
        };
        if changed {
            self.broadcast_tree();
        }
    }

    /// Renames a pane at the agent's own request (`argus-hook title ...`).
    ///
    /// A column of four rows all reading "claude" says nothing about which
    /// one is worth looking at; the agent knows what it is doing, so it is
    /// the one asked. Ignored for a pane that has exited — a rename racing
    /// a crash shouldn't relabel a dead row.
    fn set_pane_title(&self, pane: PaneId, title: &str) {
        let title = clean_title(title);
        if title.is_empty() {
            return;
        }
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            match find_pane(&mut inner.projects, pane) {
                Some(p) if !matches!(p.status, PaneStatus::Exited { .. }) && p.title != title => {
                    p.title = title;
                    true
                }
                _ => false,
            }
        };
        if changed {
            self.broadcast_tree();
        }
    }

    /// Records the conversation Argus would resume this pane with.
    ///
    /// A claim from a session that is not the pane's current owner is only
    /// honoured while the pane is not working: the pane's own agent starting
    /// over (`/clear`, a resume) is idle at that moment, whereas a CLI
    /// spawned from inside a turn arrives mid-work. The latter is listed as
    /// a child instead, which is what keeps a nested agent from stealing the
    /// identity the row resumes from.
    fn set_pane_session_id(&self, pane: PaneId, raw: &str) {
        let Some(session_id) = valid_session_id(raw) else {
            return;
        };
        if self.child_of(pane, Some(&session_id)).is_some() {
            let working = {
                let inner = self.inner.lock().unwrap();
                find_pane_ref(&inner.projects, pane)
                    .is_some_and(|p| p.status == PaneStatus::Working)
            };
            if working {
                self.with_child(pane, &session_id, |_| {});
                return;
            }
        }
        let changed = {
            let mut starting = self.starting_agents.lock().unwrap();
            let mut inner = self.inner.lock().unwrap();
            match find_pane(&mut inner.projects, pane) {
                Some(p)
                    if !matches!(p.status, PaneStatus::Exited { .. })
                        && p.harness_session_id.as_deref() != Some(&session_id) =>
                {
                    p.harness_session_id = Some(session_id);
                    p.children.clear();
                    true
                }
                Some(_) => false,
                None => match starting.get_mut(&pane) {
                    Some(pending) if pending.as_deref() != Some(&session_id) => {
                        *pending = Some(session_id);
                        true
                    }
                    _ => false,
                },
            }
        };
        if changed {
            self.broadcast_tree();
        }
    }

    /// Moves a live agent row to the known checkout it has started working
    /// in. The PTY stays intact; this changes Argus's affiliation, not the
    /// child process's working directory. The reporting command runs in the
    /// destination directory, which is the evidence that the agent moved.
    fn move_agent_to_checkout(
        &self,
        pane: PaneId,
        destination: &std::path::Path,
    ) -> anyhow::Result<()> {
        let (source_path, target_path, template, source_has_agent) = {
            let mut inner = self.inner.lock().unwrap();
            let (project_index, source_repository_index, source_index, pane_index) = inner
                .projects
                .iter()
                .enumerate()
                .find_map(|(project_index, project)| {
                    project.repositories.iter().enumerate().find_map(
                        |(repository_index, repository)| {
                            repository.checkouts.iter().enumerate().find_map(
                                |(checkout_index, checkout)| {
                                    checkout
                                        .panes
                                        .iter()
                                        .position(|candidate| candidate.id == pane)
                                        .map(|pane_index| {
                                            (
                                                project_index,
                                                repository_index,
                                                checkout_index,
                                                pane_index,
                                            )
                                        })
                                },
                            )
                        },
                    )
                })
                .ok_or_else(|| anyhow::anyhow!("no such pane"))?;

            let project = &mut inner.projects[project_index];
            let (target_repository_index, target_index) = project
                .repositories
                .iter()
                .enumerate()
                .find_map(|(repository_index, repository)| {
                    repository
                        .checkouts
                        .iter()
                        .position(|checkout| same_path(&checkout.path, destination))
                        .map(|checkout_index| (repository_index, checkout_index))
                })
                .ok_or_else(|| anyhow::anyhow!("destination is not a checkout in this project"))?;
            if source_repository_index == target_repository_index && source_index == target_index {
                return Ok(());
            }

            let moving = &project.repositories[source_repository_index].checkouts[source_index]
                .panes[pane_index];
            if moving.kind != PaneKind::Agent {
                anyhow::bail!("only agent panes can change checkout affiliation");
            }
            if matches!(moving.status, PaneStatus::Exited { .. }) {
                anyhow::bail!("an exited pane cannot change checkout affiliation");
            }

            let source_path = project.repositories[source_repository_index].checkouts[source_index]
                .path
                .clone();
            let target_path = project.repositories[target_repository_index].checkouts[target_index]
                .path
                .clone();
            let moving = project.repositories[source_repository_index].checkouts[source_index]
                .panes
                .remove(pane_index);
            let template = moving.template.clone();
            project.repositories[target_repository_index].checkouts[target_index]
                .panes
                .push(moving);
            let source_has_agent = project.repositories[source_repository_index].checkouts
                [source_index]
                .panes
                .iter()
                .any(|candidate| candidate.kind == PaneKind::Agent);
            (source_path, target_path, template, source_has_agent)
        };

        if !source_has_agent {
            for harness in &self.harnesses {
                if let Err(error) = harness.uninstall(&source_path) {
                    tracing::warn!(
                        "failed to clear {} hooks in {}: {error}",
                        harness.name,
                        source_path.display()
                    );
                }
            }
        }

        if let Some(template) = template
            .as_deref()
            .and_then(|name| self.templates.iter().find(|template| template.name == name))
        {
            let harness = self.harness_for(template);
            let port = self.hook_port.load(std::sync::atomic::Ordering::Relaxed);
            if port != 0 {
                if let Err(error) = harness.install(&target_path, pane, port, &self.hook_token) {
                    tracing::warn!(
                        "failed to install {} hooks in {}: {error}",
                        harness.name,
                        target_path.display()
                    );
                }
            }
        }

        self.broadcast_tree();
        Ok(())
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
                let _ = tokio::task::spawn_blocking(move || {
                    daemon.reconcile_worktrees();
                    daemon.drop_silent_children();
                    daemon.refresh_git_status();
                    let _ = daemon.tree_tx.send(daemon.snapshot());
                })
                .await;
            }
        });
    }

    /// Re-reads every checkout's git status and caches it on the checkout,
    /// so the tree snapshots that clients actually render cost nothing but
    /// a clone.
    ///
    /// Deliberately three phases — collect paths, read git, store results —
    /// with the lock dropped in the middle. Reading git under the lock is
    /// what this exists to avoid: status is several milliseconds of
    /// blocking I/O per checkout, and `write_pane` needs the same lock to
    /// find the pty a keystroke belongs to, so holding it across a sweep of
    /// every checkout puts that whole sweep in front of the next key.
    ///
    /// Checkouts are matched back by id: the tree can be rearranged while
    /// the lock is down, and a stale result must be dropped rather than
    /// land on whatever now occupies that position.
    pub fn refresh_git_status(&self) {
        let checkouts: Vec<(CheckoutId, PathBuf)> = {
            let inner = self.inner.lock().unwrap();
            inner
                .projects
                .iter()
                .flat_map(|p| p.repositories.iter())
                .flat_map(|r| r.checkouts.iter())
                .map(|c| (c.id, c.path.clone()))
                .collect()
        };

        let statuses: Vec<(CheckoutId, Option<GitStatus>)> = checkouts
            .into_iter()
            .map(|(id, path)| (id, crate::git::status(&path)))
            .collect();

        let mut inner = self.inner.lock().unwrap();
        for (id, status) in statuses {
            if let Some(c) = find_checkout(&mut inner.projects, id) {
                c.git = status;
            }
        }
    }

    /// Re-reads one checkout's status, for the moments where waiting for the
    /// next poll would show the user the state they just changed away from.
    /// A checkout row is named after the branch in its cached status, so a
    /// switch that only updated the fallback name would keep drawing the
    /// branch it just left for the rest of the tick.
    fn refresh_checkout_git(&self, checkout: CheckoutId) {
        let Ok(path) = self.checkout_path(checkout) else {
            return;
        };
        // Read first, take the lock second — same reason as the sweep above.
        let status = crate::git::status(&path);
        let mut inner = self.inner.lock().unwrap();
        if let Some(c) = find_checkout(&mut inner.projects, checkout) {
            c.git = status;
        }
    }

    /// Reconciles each repository's checkouts against `git worktree list` on
    /// its primary checkout, so a worktree created or removed outside
    /// Argus — a bare `git worktree add`/`remove` from a shell — still
    /// shows up, or disappears, without going through `create_worktree` /
    /// `remove_checkout`. Runs on the same blocking-pool tick as the git
    /// status poll (§4 Level 2, §11 worktree auto-discovery).
    fn reconcile_worktrees(&self) {
        self.reconcile_worktrees_with(crate::git::list_worktrees);
    }

    /// The reconciliation itself, with the worktree listing injected so
    /// tests can drive it without a real repo (and without waiting on the
    /// `git` binary). Production always passes `git::list_worktrees`.
    fn reconcile_worktrees_with(&self, list: impl Fn(&std::path::Path) -> Vec<PathBuf>) {
        let mut orphaned_panes: Vec<Pane> = Vec::new();
        {
            let mut guard = self.inner.lock().unwrap();
            let Inner { projects, ids, .. } = &mut *guard;
            for project in projects.iter_mut() {
                for repository in project.repositories.iter_mut() {
                    let Some(primary_path) = repository
                        .checkouts
                        .iter()
                        .find(|c| c.primary)
                        .map(|c| c.path.clone())
                    else {
                        continue;
                    };
                    let listed = list(&primary_path);
                    if listed.is_empty() {
                        continue;
                    }

                    for path in &listed {
                        if repository
                            .checkouts
                            .iter()
                            .any(|c| same_path(&c.path, path))
                        {
                            continue;
                        }
                        let is_primary = same_path(path, &primary_path);
                        repository.checkouts.push(Checkout {
                            id: CheckoutId(ids.alloc()),
                            name: worktree_display_name(path, is_primary),
                            path: path.clone(),
                            primary: is_primary,
                            panes: Vec::new(),
                            git: None,
                        });
                    }

                    let mut i = 0;
                    while i < repository.checkouts.len() {
                        let gone = !repository.checkouts[i].primary
                            && !listed
                                .iter()
                                .any(|path| same_path(path, &repository.checkouts[i].path));
                        if gone {
                            orphaned_panes.extend(repository.checkouts.remove(i).panes);
                        } else {
                            i += 1;
                        }
                    }
                }
            }
        }
        for pane in orphaned_panes {
            let _ = pane.runtime.kill();
        }
    }

    /// Looks under each project's root again, so a repository cloned into
    /// it — or deleted out of it — reaches the tree without restarting the
    /// daemon. `reconcile_worktrees` does the same job one level further
    /// down, for a repository's checkouts.
    fn reconcile_repositories(&self) -> bool {
        self.reconcile_repositories_with(crate::git::discover_repositories)
    }

    /// The reconciliation itself, with the scan injected so tests can state
    /// what is on disk instead of building it. Reports whether anything
    /// changed. Production always passes `git::discover_repositories`.
    fn reconcile_repositories_with(
        &self,
        discover: impl Fn(&std::path::Path) -> Vec<PathBuf>,
    ) -> bool {
        // Scanning happens between the two locks and never inside one, for
        // the same reason `add_project` scans before taking it.
        let roots: Vec<(ProjectId, PathBuf)> = {
            let inner = self.inner.lock().unwrap();
            inner
                .projects
                .iter()
                .filter_map(|p| p.root.clone().map(|root| (p.id, root)))
                .collect()
        };
        if roots.is_empty() {
            return false;
        }
        let scanned: Vec<(ProjectId, Vec<PathBuf>)> = roots
            .into_iter()
            .map(|(id, root)| (id, discover(&root)))
            .collect();

        let mut changed = false;
        let mut inner = self.inner.lock().unwrap();
        let Inner {
            projects,
            ids,
            excluded,
            ..
        } = &mut *inner;
        for (project_id, found) in scanned {
            let found = retain_included(excluded, found);
            let Some(project) = projects.iter_mut().find(|p| p.id == project_id) else {
                continue;
            };
            changed |= install_discovered(ids, &mut project.repositories, &found);

            // A repository that has left the root leaves the project with
            // it — but not while it is holding panes. A directory can go
            // missing for reasons that are none of the user's doing (a drive
            // that dropped, a scan racing a move), and taking a running
            // agent down with it is not a trade worth making: those rows
            // stay until they are empty. A repository the config named
            // outright is never removed this way at all.
            project.repositories.retain(|repository| {
                if !repository.discovered
                    || repository.checkouts.iter().any(|c| !c.panes.is_empty())
                {
                    return true;
                }
                let still_there = repository
                    .checkouts
                    .iter()
                    .filter(|c| c.primary)
                    .any(|c| found.iter().any(|path| same_path(&c.path, path)));
                changed |= !still_there;
                still_there
            });
        }
        changed
    }

    /// Re-scans project roots on a slower beat than the git poll: a scan
    /// walks directories rather than reading one repository's state, and a
    /// repository being cloned is a far rarer event than a file being
    /// edited. Blocking-pool, like the poll, and for the same reason.
    pub fn start_project_scan(self: &Arc<Self>) {
        let daemon = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            loop {
                interval.tick().await;
                let daemon = daemon.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    if daemon.reconcile_repositories() {
                        // A repository the scan just found has no cached
                        // status yet, and its row is named from that status.
                        daemon.refresh_git_status();
                        daemon.broadcast_tree();
                        // A project gaining or losing a repository moves the
                        // workspace rollup counts too.
                        daemon.broadcast_workspaces();
                    }
                })
                .await;
            }
        });
    }

    /// Adds a brand-new project rooted at an arbitrary directory — not
    /// restricted to whatever's already in `projects.toml` or wherever the
    /// daemon happens to be running from — and persists it so it survives a
    /// restart.
    ///
    /// The directory is the project's root, and every Git repository at or
    /// beneath it becomes one of its repositories. Pointing at a repository
    /// therefore adds that one repository, which is what it has always
    /// meant; pointing at the directory a dozen of them live in adds the
    /// dozen. A root with none of them yet is a project all the same — the
    /// scan runs again, and the first clone into it arrives on its own.
    pub fn add_project(&self, path: &str) -> anyhow::Result<()> {
        let expanded = config::expand_home(path);
        if !expanded.is_dir() {
            anyhow::bail!("not a directory: {}", expanded.display());
        }
        let name = expanded
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string());

        // Scanned before the lock is taken: this walks directories, and
        // every pane event in the daemon queues behind that mutex.
        let found = crate::git::discover_repositories(&expanded);

        // New projects land in whichever workspace is open, so "add this
        // directory" means "add it to what I am looking at".
        let (workspace, workspace_name) = self.open_workspace_ref();
        config::append_project(&name, &expanded, &workspace_name)?;

        {
            let mut inner = self.inner.lock().unwrap();
            let Inner { projects, ids, .. } = &mut *inner;
            let mut repositories = Vec::new();
            install_discovered(ids, &mut repositories, &found);
            projects.push(Project {
                id: ProjectId(ids.alloc()),
                workspace,
                name,
                root: Some(expanded),
                repositories,
            });
        }
        self.broadcast_tree();
        // The rollup counts changed too.
        self.broadcast_workspaces();
        Ok(())
    }

    /// Adds one repository to a project that is already in the panel, by
    /// path — the way a repository that lives nowhere near the project's
    /// root joins it. Named in the project's `repos` list rather than found
    /// by a scan, so it survives a restart and no amount of rescanning
    /// takes it away again.
    ///
    /// The path is taken at its word, as `repos` always has: a directory
    /// that is not a Git repository still becomes a row, which is how a
    /// plain directory gets panes. A path the user had previously removed
    /// stops being excluded — asking for it back is the undo for that.
    pub fn add_repository(&self, project: ProjectId, path: &str) -> anyhow::Result<()> {
        let expanded = config::expand_home(path);
        if !expanded.is_dir() {
            anyhow::bail!("not a directory: {}", expanded.display());
        }

        let (index, name) = {
            let inner = self.inner.lock().unwrap();
            let index = inner
                .projects
                .iter()
                .position(|p| p.id == project)
                .ok_or_else(|| anyhow::anyhow!("no such project"))?;
            let p = &inner.projects[index];
            if p.repositories
                .iter()
                .flat_map(|r| r.checkouts.iter())
                .any(|c| same_path(&c.path, &expanded))
            {
                anyhow::bail!("{} already has {}", p.name, expanded.display());
            }
            (index, p.name.clone())
        };

        // Config first, for the same reason removal writes it first: a row
        // that appears in the panel but not in the file is gone again after
        // a restart.
        config::append_repo(index, &name, &expanded)?;

        let unexcluded = {
            let mut inner = self.inner.lock().unwrap();
            let was_excluded = is_excluded(&inner.excluded, &expanded);
            inner.excluded.retain(|e| !same_path(e, &expanded));
            let Inner { projects, ids, .. } = &mut *inner;
            let repository = new_repository(ids, expanded.clone(), false);
            let p = projects
                .iter_mut()
                .find(|p| p.id == project)
                .ok_or_else(|| anyhow::anyhow!("no such project"))?;
            p.repositories.push(repository);
            was_excluded.then(|| inner.excluded.clone())
        };
        if let Some(remaining) = unexcluded {
            config::rewrite_excluded_repos(&remaining)?;
        }

        self.broadcast_tree();
        Ok(())
    }

    /// Takes a project out of the panel and out of `projects.toml`.
    /// Nothing on disk is touched — this is the undo for `add_project`, not
    /// a delete, and adding the same directory again brings the same tree
    /// back.
    ///
    /// Refused while any of its panes is alive. Removing the row would
    /// leave those processes running with nowhere to reach them, and unlike
    /// `remove_checkout` — where killing the panes is the point, because
    /// the worktree they sit in is going away — here the checkout survives
    /// and the user can simply look at it again.
    pub fn remove_project(&self, project: ProjectId) -> anyhow::Result<()> {
        let (index, name, excluded_paths) = {
            let inner = self.inner.lock().unwrap();
            let index = inner
                .projects
                .iter()
                .position(|p| p.id == project)
                .ok_or_else(|| anyhow::anyhow!("no such project"))?;
            let p = &inner.projects[index];
            if p.repositories
                .iter()
                .flat_map(|r| r.checkouts.iter())
                .any(|c| !c.panes.is_empty())
            {
                anyhow::bail!("close {}'s panes before removing it", p.name);
            }
            // Exclusions under a project that no longer exists describe
            // nothing, so they leave with it — otherwise adding the same
            // directory back would bring back a project missing exactly the
            // repositories the user had once removed, with nothing on
            // screen to explain why.
            let paths: Vec<PathBuf> = p
                .repositories
                .iter()
                .flat_map(|r| r.checkouts.iter())
                .filter(|c| c.primary)
                .map(|c| c.path.clone())
                .collect();
            (index, p.name.clone(), paths)
        };

        // Config first: a project that vanishes from the panel but not from
        // the file comes back on the next restart, which reads as Argus
        // having ignored the request.
        config::remove_project(index, &name)?;

        let remaining = {
            let mut inner = self.inner.lock().unwrap();
            inner.projects.retain(|p| p.id != project);
            inner
                .excluded
                .retain(|e| !excluded_paths.iter().any(|p| same_path(e, p)));
            inner.excluded.clone()
        };
        config::rewrite_excluded_repos(&remaining)?;
        self.broadcast_tree();
        // One fewer project in the workspace's rollup.
        self.broadcast_workspaces();
        Ok(())
    }

    /// Takes one repository row out of its project. Like `remove_project`,
    /// nothing on disk changes; unlike it, the project's root keeps being
    /// scanned, so the path has to be remembered as excluded or the next
    /// scan would put the row straight back.
    pub fn remove_repository(&self, repository: RepositoryId) -> anyhow::Result<()> {
        let path = {
            let inner = self.inner.lock().unwrap();
            let r = inner
                .projects
                .iter()
                .flat_map(|p| p.repositories.iter())
                .find(|r| r.id == repository)
                .ok_or_else(|| anyhow::anyhow!("no such repository"))?;
            if r.checkouts.iter().any(|c| !c.panes.is_empty()) {
                anyhow::bail!("close {}'s panes before removing it", r.name);
            }
            r.checkouts
                .iter()
                .find(|c| c.primary)
                .map(|c| c.path.clone())
                .ok_or_else(|| anyhow::anyhow!("{} has no primary checkout", r.name))?
        };

        config::append_excluded_repo(&path)?;
        {
            let mut inner = self.inner.lock().unwrap();
            inner.excluded.push(path);
            for project in inner.projects.iter_mut() {
                project.repositories.retain(|r| r.id != repository);
            }
        }
        self.broadcast_tree();
        Ok(())
    }

    /// `git worktree add`s a new checkout in `base`'s repository, branched off
    /// `base`'s current HEAD, and appends it to the tree. Placed under
    /// `.argus/worktrees/<branch>` beside the repository's primary checkout
    /// (DESIGN.md §4 Level 2), regardless of which checkout `base` itself
    /// is — so worktrees always nest under the one directory, not under
    /// each other.
    /// Moves this checkout onto an existing branch. `git` refuses when the
    /// switch would clobber uncommitted work, and that refusal is exactly
    /// what should reach the user, so its stderr is passed through.
    pub async fn switch_branch(&self, checkout: CheckoutId, branch: &str) -> anyhow::Result<()> {
        self.git_switch(checkout, &["switch", branch], branch).await
    }

    /// Creates a branch here and moves onto it, leaving the checkout where
    /// it is — unlike `create_worktree`, which makes a directory for it.
    pub async fn create_branch(&self, checkout: CheckoutId, branch: &str) -> anyhow::Result<()> {
        self.git_switch(checkout, &["switch", "-c", branch], branch)
            .await
    }

    async fn git_switch(
        &self,
        checkout: CheckoutId,
        args: &[&str],
        branch: &str,
    ) -> anyhow::Result<()> {
        let branch = branch.trim();
        if branch.is_empty() {
            anyhow::bail!("branch name can't be empty");
        }
        // Leading dashes would be read as flags, and git's own refname
        // rules reject the rest.
        if branch.starts_with('-') {
            anyhow::bail!("not a valid branch name: {branch}");
        }
        let path = self.checkout_path(checkout)?;

        let output = crate::command::git()
            .args(args)
            .current_dir(&path)
            .output()
            .await?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("{}", stderr.trim());
        }

        // The checkout's name follows the branch it sits on, the way it did
        // when the worktree was created.
        {
            let mut inner = self.inner.lock().unwrap();
            if let Some(c) = find_checkout(&mut inner.projects, checkout) {
                c.name = branch.to_string();
            }
        }
        self.refresh_checkout_git(checkout);
        self.broadcast_tree();
        Ok(())
    }

    pub async fn create_worktree(
        self: &Arc<Self>,
        base: CheckoutId,
        branch: String,
    ) -> anyhow::Result<()> {
        let branch = branch.trim().to_string();
        if branch.is_empty() {
            anyhow::bail!("branch name can't be empty");
        }

        let (repository_id, base_path, primary_path) = {
            let inner = self.inner.lock().unwrap();
            find_checkout_context(&inner.projects, base)
                .ok_or_else(|| anyhow::anyhow!("no such checkout"))?
        };
        let dest = primary_path.join(".argus").join("worktrees").join(&branch);

        let output = crate::command::git()
            .args(["worktree", "add", "-b", &branch])
            .arg(&dest)
            .current_dir(&base_path)
            .output()
            .await?;
        if !output.status.success() {
            anyhow::bail!(
                "git worktree add failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        let added = {
            let mut inner = self.inner.lock().unwrap();
            let id = CheckoutId(inner.ids.alloc());
            find_repository(&mut inner.projects, repository_id).map(|r| {
                r.checkouts.push(Checkout {
                    id,
                    name: branch,
                    path: dest,
                    primary: false,
                    panes: Vec::new(),
                    git: None,
                });
                id
            })
        };
        if let Some(id) = added {
            self.refresh_checkout_git(id);
        }
        self.broadcast_tree();
        Ok(())
    }

    /// Kills every pane in a linked-worktree checkout, `git worktree
    /// remove`s and deletes its branch (both best-effort — the checkout
    /// leaves the tree regardless), and refuses outright on the primary
    /// checkout, which is the repo the user already had, not Argus's to
    /// delete (DESIGN.md §4 Level 2).
    /// Errors rather than `None`, so a stale id reaches the user as text.
    pub fn checkout_path(&self, checkout: CheckoutId) -> anyhow::Result<PathBuf> {
        let inner = self.inner.lock().unwrap();
        find_checkout_ref(&inner.projects, checkout)
            .map(|c| c.path.clone())
            .ok_or_else(|| anyhow::anyhow!("no such checkout"))
    }

    pub async fn remove_checkout(&self, checkout: CheckoutId) -> anyhow::Result<()> {
        let (path, primary, primary_path, pane_ids) = {
            let inner = self.inner.lock().unwrap();
            let c = find_checkout_ref(&inner.projects, checkout)
                .ok_or_else(|| anyhow::anyhow!("no such checkout"))?;
            let (_, _, primary_path) = find_checkout_context(&inner.projects, checkout)
                .ok_or_else(|| anyhow::anyhow!("no such checkout"))?;
            (
                c.path.clone(),
                c.primary,
                primary_path,
                c.panes.iter().map(|p| p.id).collect::<Vec<_>>(),
            )
        };
        if primary {
            anyhow::bail!("refusing to remove the primary checkout");
        }

        let branch = crate::git::status(&path).and_then(|s| s.branch);

        for pane in pane_ids {
            let _ = self.close_pane(pane);
        }

        let output = crate::command::git()
            .args(["worktree", "remove", "--force"])
            .arg(&path)
            .current_dir(&primary_path)
            .output()
            .await?;
        if !output.status.success() {
            anyhow::bail!(
                "git worktree remove failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        if let Some(branch) = branch {
            let _ = crate::command::git()
                .args(["branch", "-D", &branch])
                .current_dir(&primary_path)
                .output()
                .await;
        }

        {
            let mut inner = self.inner.lock().unwrap();
            remove_checkout_entry(&mut inner.projects, checkout);
        }
        self.broadcast_tree();
        Ok(())
    }
}

fn find_checkout(projects: &mut [Project], id: CheckoutId) -> Option<&mut Checkout> {
    projects
        .iter_mut()
        .flat_map(|p| p.repositories.iter_mut())
        .flat_map(|r| r.checkouts.iter_mut())
        .find(|c| c.id == id)
}

fn find_checkout_ref(projects: &[Project], id: CheckoutId) -> Option<&Checkout> {
    projects
        .iter()
        .flat_map(|p| p.repositories.iter())
        .flat_map(|r| r.checkouts.iter())
        .find(|c| c.id == id)
}

fn find_repository(projects: &mut [Project], id: RepositoryId) -> Option<&mut Repository> {
    projects
        .iter_mut()
        .flat_map(|p| p.repositories.iter_mut())
        .find(|r| r.id == id)
}

/// For a checkout, the id of its owning repository, that checkout's own path
/// (the base to branch off / run `git worktree` commands from), and its
/// repository's primary checkout path (where new worktrees get placed).
fn find_checkout_context(
    projects: &[Project],
    id: CheckoutId,
) -> Option<(RepositoryId, PathBuf, PathBuf)> {
    projects
        .iter()
        .flat_map(|p| p.repositories.iter())
        .find_map(|r| {
            let base = r.checkouts.iter().find(|c| c.id == id)?;
            let primary = r.checkouts.iter().find(|c| c.primary).unwrap_or(base);
            Some((r.id, base.path.clone(), primary.path.clone()))
        })
}

/// Prefers the checked-out branch name for a newly-discovered worktree —
/// matches how `create_worktree` names ones Argus made itself — falling
/// back to the directory name for a detached HEAD or an unreadable repo.
fn worktree_display_name(path: &std::path::Path, is_primary: bool) -> String {
    if !is_primary {
        if let Some(branch) = crate::git::status(path).and_then(|s| s.branch) {
            return branch;
        }
    }
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

fn remove_checkout_entry(projects: &mut [Project], id: CheckoutId) -> Option<Checkout> {
    for project in projects.iter_mut() {
        for repository in project.repositories.iter_mut() {
            if let Some(pos) = repository.checkouts.iter().position(|c| c.id == id) {
                return Some(repository.checkouts.remove(pos));
            }
        }
    }
    None
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

fn all_panes(projects: &mut [Project]) -> impl Iterator<Item = &mut Pane> {
    projects
        .iter_mut()
        .flat_map(|p| p.repositories.iter_mut())
        .flat_map(|r| r.checkouts.iter_mut())
        .flat_map(|c| c.panes.iter_mut())
}

fn find_pane(projects: &mut [Project], id: PaneId) -> Option<&mut Pane> {
    projects
        .iter_mut()
        .flat_map(|p| p.repositories.iter_mut())
        .flat_map(|r| r.checkouts.iter_mut())
        .flat_map(|c| c.panes.iter_mut())
        .find(|p| p.id == id)
}

fn find_pane_ref(projects: &[Project], id: PaneId) -> Option<&Pane> {
    projects
        .iter()
        .flat_map(|p| p.repositories.iter())
        .flat_map(|r| r.checkouts.iter())
        .flat_map(|c| c.panes.iter())
        .find(|p| p.id == id)
}

/// Whether any agent pane is still open in the checkout at `path`. Gates
/// tearing down that checkout's managed hooks, which are shared by every
/// agent running there.
fn checkout_has_agent(projects: &[Project], path: &std::path::Path) -> bool {
    projects
        .iter()
        .flat_map(|p| p.repositories.iter())
        .flat_map(|r| r.checkouts.iter())
        .filter(|c| c.path == path)
        .any(|c| c.panes.iter().any(|p| p.kind == PaneKind::Agent))
}

/// Removes a pane from whichever checkout holds it, returning it along with
/// that checkout's path — which the caller can't look up afterwards, the
/// pane being gone by then.
fn remove_pane_with_checkout(projects: &mut [Project], id: PaneId) -> Option<(Pane, PathBuf)> {
    for project in projects.iter_mut() {
        for repository in project.repositories.iter_mut() {
            for checkout in repository.checkouts.iter_mut() {
                if let Some(pos) = checkout.panes.iter().position(|p| p.id == id) {
                    return Some((checkout.panes.remove(pos), checkout.path.clone()));
                }
            }
        }
    }
    None
}

/// Reads one HTTP/1.1 request, checks the bearer token, and applies the pane
/// operation its path encodes. Hand-rolled rather than pulling in an HTTP server crate:
/// the request shape is entirely our own (we generate every hook command
/// that ever calls this), so there's nothing to be robust against beyond
/// "well-formed or ignored".
async fn handle_hook_request(
    stream: tokio::net::TcpStream,
    daemon: Arc<Daemon>,
) -> anyhow::Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

    let (rd, mut wr) = tokio::io::split(stream);
    let mut reader = BufReader::new(rd);

    let mut request_line = String::new();
    reader.read_line(&mut request_line).await?;
    let path = request_line
        .strip_prefix("POST ")
        .and_then(|rest| rest.split(' ').next())
        .unwrap_or("")
        .to_string();

    let mut authorized = false;
    let mut content_length: usize = 0;
    let mut reporter: Option<String> = None;
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some(v) = line
            .strip_prefix("Authorization:")
            .or_else(|| line.strip_prefix("authorization:"))
        {
            authorized = v
                .trim()
                .eq_ignore_ascii_case(&format!("Bearer {}", daemon.hook_token));
        } else if let Some(v) = line
            .strip_prefix("Content-Length:")
            .or_else(|| line.strip_prefix("content-length:"))
        {
            content_length = v.trim().parse().unwrap_or(0);
        } else if let Some(v) = line
            .strip_prefix("X-Argus-Session:")
            .or_else(|| line.strip_prefix("x-argus-session:"))
        {
            reporter = valid_session_id(v);
        }
    }
    // Capped: the only body anyone sends is a pane title, and this server
    // trusts nothing about a request beyond having written the command that
    // makes it.
    let mut body = vec![0u8; content_length.min(MAX_BODY)];
    if !body.is_empty() {
        let _ = reader.read_exact(&mut body).await;
    }

    if authorized {
        match parse_pane_path(&path) {
            Some((pane, Endpoint::Status(report))) => {
                let note = String::from_utf8_lossy(&body).to_string();
                daemon.report_pane_status(pane, reporter.as_deref(), report.status(), Some(note))
            }
            Some((pane, Endpoint::Title)) => {
                daemon.report_pane_title(pane, reporter.as_deref(), &String::from_utf8_lossy(&body))
            }
            Some((pane, Endpoint::Checkout))
                if daemon.child_of(pane, reporter.as_deref()).is_none() =>
            {
                let destination = PathBuf::from(String::from_utf8_lossy(&body).trim());
                if let Err(error) = daemon.move_agent_to_checkout(pane, &destination) {
                    tracing::warn!("pane {} could not move checkout: {error}", pane.0);
                }
            }
            Some((pane, Endpoint::Session)) => {
                daemon.set_pane_session_id(pane, &String::from_utf8_lossy(&body))
            }
            // A checkout move from an agent that does not own the pane is
            // dropped: the row follows the agent Argus started in it.
            _ => {}
        }
        wr.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await?;
    } else {
        wr.write_all(
            b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )
        .await?;
    }
    Ok(())
}

const MAX_BODY: usize = 4096;

/// What a request wants done to a pane.
#[derive(Debug, PartialEq, Eq)]
enum Endpoint {
    Status(crate::harness::Report),
    Title,
    Checkout,
    Session,
}

/// `/pane/<id>/status/<working|idle|waiting|needs-review|done|failed>`,
/// `/pane/<id>/title`, and `/pane/<id>/checkout`.
///
/// The status is named in the URL rather than the harness's own event name:
/// the installer already knows what each of its events means, so by the time
/// a request arrives the daemon needs no dialect at all. That is what makes
/// a harness a config block instead of a match arm here.
fn parse_pane_path(path: &str) -> Option<(PaneId, Endpoint)> {
    let mut parts = path.trim_start_matches('/').split('/');
    if parts.next()? != "pane" {
        return None;
    }
    let pane = PaneId(parts.next()?.parse().ok()?);
    let endpoint = match parts.next()? {
        "status" => Endpoint::Status(crate::harness::Report::parse(parts.next()?)?),
        "title" => Endpoint::Title,
        "checkout" => Endpoint::Checkout,
        "session" => Endpoint::Session,
        _ => return None,
    };
    if parts.next().is_some() {
        return None;
    }
    Some((pane, endpoint))
}

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

fn valid_session_id(raw: &str) -> Option<String> {
    const MAX: usize = 512;
    let id = raw.trim();
    (!id.is_empty() && id.len() <= MAX && !id.chars().any(char::is_control)).then(|| id.to_string())
}

/// Not cryptographically strong — see `Daemon::hook_token`'s doc comment —
/// just enough entropy that it isn't a fixed, guessable string.
fn gen_token() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static NEXT_TOKEN: AtomicU64 = AtomicU64::new(0);

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let sequence = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
    format!("{now:016x}{sequence:016x}")
}

fn has_windows_drive_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

/// The arguments an agent pane starts with, and whether that command asks
/// the CLI to continue a conversation rather than open a new one.
///
/// Restoring a pane means restoring what was in it, so the harness's resume
/// arguments go on the end of the template's own command — the user's flags
/// still apply to the conversation being continued. A harness Argus cannot
/// ask to resume leaves the command exactly as it was, and the pane is not
/// treated as resumed: there is nothing for a failure to fall back from.
fn agent_args(
    configured: &[String],
    resume: &[String],
    resume_id: &[String],
    start: Start,
    session_id: Option<&str>,
) -> (Vec<String>, bool) {
    let mut args = configured.to_vec();
    if start == Start::Fresh {
        return (args, false);
    }
    if let Some(session_id) = session_id {
        if resume_id.is_empty() {
            return (args, false);
        }
        args.extend(
            resume_id
                .iter()
                .map(|arg| arg.replace("{session_id}", session_id)),
        );
    } else {
        if resume.is_empty() {
            return (args, false);
        }
        args.extend(resume.iter().cloned());
    }
    (args, true)
}

/// Whether a resumed agent's exit reads as "there was no conversation to
/// continue" rather than as an agent the user is done with.
///
/// A clean exit is always the user's: every one of these CLIs exits 0 when
/// you leave it, and refuses with a status when it cannot start.
fn nothing_to_resume(code: Option<i32>, ran_for: Duration) -> bool {
    code != Some(0) && ran_for < RESUME_GRACE
}

/// A repository holding only its primary checkout, which is what both a
/// configured path and a discovered one start as. Linked worktrees arrive
/// afterwards, from `reconcile_worktrees`.
fn new_repository(ids: &mut IdGen, path: PathBuf, discovered: bool) -> Repository {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    Repository {
        id: RepositoryId(ids.alloc()),
        name: name.clone(),
        discovered,
        checkouts: vec![Checkout {
            id: CheckoutId(ids.alloc()),
            name,
            path,
            primary: true,
            panes: Vec::new(),
            git: None,
        }],
    }
}

/// Adds the repositories a scan found that aren't there yet, and reports
/// whether it added any. Repositories already present are left exactly as
/// they are, ids, checkouts and panes included: a scan is a way of noticing
/// what is on disk, not a reason to rebuild the tree. A discovered path that
/// matches a repository the config named outright belongs to that
/// repository rather than to a second row of its own.
fn install_discovered(
    ids: &mut IdGen,
    repositories: &mut Vec<Repository>,
    found: &[PathBuf],
) -> bool {
    let mut added = false;
    for path in found {
        if repositories
            .iter()
            .any(|r| r.checkouts.iter().any(|c| same_path(&c.path, path)))
        {
            continue;
        }
        repositories.push(new_repository(ids, path.clone(), true));
        added = true;
    }
    added
}

/// Whether this path is one the user has taken out of the panel.
fn is_excluded(excluded: &[PathBuf], path: &std::path::Path) -> bool {
    excluded.iter().any(|e| same_path(e, path))
}

fn retain_included(excluded: &[PathBuf], found: Vec<PathBuf>) -> Vec<PathBuf> {
    found
        .into_iter()
        .filter(|path| !is_excluded(excluded, path))
        .collect()
}

fn same_path(a: &std::path::Path, b: &std::path::Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProjectConfig;

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

    fn listing(paths: &[&str]) -> Vec<PathBuf> {
        paths.iter().map(PathBuf::from).collect()
    }

    // --- the pane API ------------------------------------------------------

    #[test]
    fn a_status_path_names_the_pane_and_the_state_itself() {
        // Not the harness's event name: the installer already resolved that,
        // which is what lets a new harness be config rather than code.
        assert_eq!(
            parse_pane_path("/pane/7/status/working"),
            Some((PaneId(7), Endpoint::Status(crate::harness::Report::Working)))
        );
        assert_eq!(
            parse_pane_path("/pane/7/status/failed"),
            Some((PaneId(7), Endpoint::Status(crate::harness::Report::Failed)))
        );
        assert_eq!(
            parse_pane_path("/pane/7/status/needs-review"),
            Some((
                PaneId(7),
                Endpoint::Status(crate::harness::Report::NeedsReview)
            ))
        );
        assert_eq!(
            parse_pane_path("/pane/7/status/done"),
            Some((PaneId(7), Endpoint::Status(crate::harness::Report::Done)))
        );
        assert_eq!(
            parse_pane_path("/pane/7/title"),
            Some((PaneId(7), Endpoint::Title))
        );
        assert_eq!(
            parse_pane_path("/pane/7/checkout"),
            Some((PaneId(7), Endpoint::Checkout))
        );
        assert_eq!(
            parse_pane_path("/pane/7/session"),
            Some((PaneId(7), Endpoint::Session))
        );
    }

    #[test]
    fn a_pane_path_rejects_junk() {
        assert_eq!(
            parse_pane_path("/pane/7/status/Stop"),
            None,
            "an event name"
        );
        assert_eq!(
            parse_pane_path("/pane/7/status/exited"),
            None,
            "ours to decide"
        );
        assert_eq!(parse_pane_path("/nope/7/title"), None, "wrong prefix");
        assert_eq!(parse_pane_path("/pane/abc/title"), None, "non-numeric pane");
        assert_eq!(parse_pane_path("/pane/7"), None, "no endpoint");
        assert_eq!(
            parse_pane_path("/pane/7/title/extra"),
            None,
            "trailing junk"
        );
        assert_eq!(parse_pane_path(""), None, "empty path");
    }

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
            d.session().panes[0].harness_session_id.as_deref(),
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
            d.session().panes[0].harness_session_id.as_deref(),
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
            }],
            agents: vec![AgentConfig {
                name: "claude".to_string(),
                cmd: vec![if cfg!(windows) { "cmd" } else { "sh" }.to_string()],
                env: Default::default(),
                harness: None,
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
        assert_eq!(d.session().panes[0].checkout_path, second.path());
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
            d.session().panes[0].harness_session_id.as_deref(),
            Some(body)
        );
        d.close_pane(pane).unwrap();
    }

    #[test]
    fn session_identity_arriving_before_pane_registration_is_retained() {
        let d = daemon_with_primary("/repo");
        let pane = PaneId(42);
        d.starting_agents.lock().unwrap().insert(pane, None);

        d.set_pane_session_id(pane, "session-early");

        assert_eq!(
            d.starting_agents
                .lock()
                .unwrap()
                .get(&pane)
                .and_then(Option::as_deref),
            Some("session-early")
        );
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
            vec!["claude", "codex", "opencode", "agy"]
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
        Daemon::new(ConfigFile {
            workspaces: Vec::new(),
            projects: vec![ProjectConfig {
                name: "proj".to_string(),
                root: None,
                repos: vec![dir.to_string_lossy().to_string()],
                workspace: None,
            }],
            agents: vec![AgentConfig {
                name: "claude".to_string(),
                cmd: vec!["echo".to_string(), "hi".to_string()],
                env: Default::default(),
                harness: None,
            }],
            harnesses: Vec::new(),
        })
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
            }],
            agents: Vec::new(),
            harnesses: Vec::new(),
        })
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
            d.reconcile_repositories_with(|_| listing(&[&cloned.to_string_lossy()])),
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
            !d.reconcile_repositories_with(|_| listing(&[&child.to_string_lossy()])),
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

        assert!(d.reconcile_repositories_with(|_| Vec::new()));
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

        assert!(!d.reconcile_repositories_with(|_| Vec::new()));
        assert_eq!(
            repository_names(&d),
            vec!["orion"],
            "still there, with its pane"
        );

        d.close_pane(pane).unwrap();
        assert!(d.reconcile_repositories_with(|_| Vec::new()));
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
        assert!(!d.reconcile_repositories_with(|_| Vec::new()));
        assert_eq!(repository_names(&d), vec!["scratch"]);
    }

    #[test]
    fn a_project_without_a_root_is_never_scanned() {
        let d = daemon_with_repositories(&["/configured"]);
        assert!(
            !d.reconcile_repositories_with(|_| panic!("a rootless project has nothing to scan")),
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

        with_temp_config(|cfg| {
            let d = Daemon::new(ConfigFile::default());
            d.add_project(&dir.path().to_string_lossy()).unwrap();

            let written = std::fs::read_to_string(cfg.join("projects.toml")).unwrap();
            let root = dir.path().to_string_lossy().replace('\\', "/");
            assert!(
                written.contains(&format!("root = {root:?}")),
                "the root is what gets scanned again next time:\n{written}"
            );
            assert!(
                !written.contains("repos = "),
                "and what it found is not frozen into the file:\n{written}"
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
            let d = Daemon::new(ConfigFile::default());
            d.add_project(&dir.path().to_string_lossy()).unwrap();
            let before = repository_names(&d);

            let restarted = Daemon::new(crate::config::load().unwrap());
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

        let d = Daemon::new(ConfigFile::default());
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
            let restarted = Daemon::new(crate::config::load().unwrap());
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
            assert!(!d.reconcile_repositories_with(crate::git::discover_repositories));
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
            let restarted = Daemon::new(crate::config::load().unwrap());
            assert_eq!(repository_names(&restarted), vec!["orion"]);
        });
    }

    // --- removing what was added --------------------------------------------

    /// A project rooted at a temp directory holding one repository per
    /// name, added through `add_project` so it is written to the config the
    /// way the TUI writes it.
    fn added_project_with(names: &[&str]) -> (tempfile::TempDir, Arc<Daemon>) {
        let dir = tempfile::tempdir().unwrap();
        for name in names {
            let child = dir.path().join(name);
            std::fs::create_dir(&child).unwrap();
            let _repo = real_repo(&child);
        }
        let d = Daemon::new(ConfigFile::default());
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
            let written = std::fs::read_to_string(cfg.join("projects.toml")).unwrap();
            assert!(
                !written.contains("[[project]]"),
                "and out of the config, not just this run:
{written}"
            );
            assert!(
                dir.path().join("orion").is_dir(),
                "removing is not deleting — the repository is still on disk"
            );
        });
    }

    #[test]
    fn removing_one_project_keeps_the_rest_of_the_users_file() {
        // The config is hand-edited and full of comments; a removal is a
        // text edit to one block, not a serde round-trip of the whole file.
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

            let d = Daemon::new(crate::config::load().unwrap());
            let doomed = d
                .snapshot()
                .into_iter()
                .find(|p| p.name == "doomed")
                .unwrap()
                .id;
            d.remove_project(doomed).unwrap();

            let after = std::fs::read_to_string(&cfg_path).unwrap();
            assert!(
                after.contains("# what these are") && after.contains(r#"name = "also-keep""#),
                "everything else is left exactly as it was:
{after}"
            );
            assert!(
                !after.contains(r#"name = "doomed""#),
                "and the block itself is gone:
{after}"
            );
            assert!(
                !after.contains("# the one going away"),
                "with the comment that introduced it:
{after}"
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

            let d = Daemon::new(crate::config::load().unwrap());
            let first = d
                .snapshot()
                .into_iter()
                .find(|p| p.name == "first")
                .unwrap();
            d.add_repository(first.id, &added.path().to_string_lossy())
                .unwrap();

            let after = std::fs::read_to_string(&cfg_path).unwrap();
            let repos = crate::config::load().unwrap().projects.remove(0).repos;
            assert_eq!(
                repos,
                vec![
                    "/one".to_string(),
                    added.path().to_string_lossy().replace('\\', "/")
                ],
                "the new path joins the ones already listed:
{after}"
            );
            assert!(
                after.contains("# hand written")
                    && after.contains(r#"name = "second""#)
                    && after.contains(r#"root = "/somewhere""#),
                "and nothing else in the file moved:
{after}"
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

            let restarted = Daemon::new(crate::config::load().unwrap());
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

            let restarted = Daemon::new(crate::config::load().unwrap());
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

            let restarted = Daemon::new(crate::config::load().unwrap());
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
                },
                ProjectConfig {
                    name: "day-job".to_string(),
                    root: None,
                    repos: vec!["/day-job".to_string()],
                    workspace: Some("work".to_string()),
                },
                ProjectConfig {
                    name: "side".to_string(),
                    root: None,
                    repos: vec!["/side".to_string()],
                    workspace: Some("weekend".to_string()),
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
            let d = Daemon::new(config_with_workspaces());
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
            let d = Daemon::new(config_with_workspaces());
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
            let d = Daemon::new(config_with_workspaces());
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
            let d = Daemon::new(config_with_workspaces());
            d.open_workspace(workspace_named(&d, "work")).unwrap();
            assert_eq!(d.workspaces().iter().filter(|w| w.open).count(), 1);
        });
    }

    #[test]
    fn reopening_the_already_open_workspace_changes_nothing() {
        with_temp_config(|_| {
            let d = Daemon::new(config_with_workspaces());
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
            let d = Daemon::new(config_with_workspaces());
            assert!(d.open_workspace(WorkspaceId(9999)).is_err());
        });
    }

    #[test]
    fn the_open_workspace_is_remembered_for_the_next_daemon() {
        with_temp_config(|_| {
            let d = Daemon::new(config_with_workspaces());
            d.open_workspace(workspace_named(&d, "work")).unwrap();
            drop(d);

            let next = Daemon::new(config_with_workspaces());
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
            let d = Daemon::new(config_with_workspaces());
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
            let d = Daemon::new(config_with_workspaces());
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
                    },
                    ProjectConfig {
                        name: "elsewhere".to_string(),
                        root: None,
                        repos: vec![dir.path().to_string_lossy().to_string()],
                        workspace: Some("other".to_string()),
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
            let d = Daemon::new(config_with_workspaces());
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
        with_temp_config(|dir| {
            let d = Daemon::new(config_with_workspaces());
            d.open_workspace(workspace_named(&d, "work")).unwrap();
            d.add_project(&repo.path().to_string_lossy()).unwrap();

            let written = std::fs::read_to_string(dir.join("projects.toml")).unwrap();
            assert!(
                written.contains(r#"workspace = "work""#),
                "the workspace must be written out, not just held in memory:\n{written}"
            );
        });
    }
    #[test]
    fn a_created_workspace_is_declared_on_disk_and_opened() {
        with_temp_config(|dir| {
            let d = Daemon::new(config_with_workspaces());
            d.create_workspace("side").unwrap();

            let ws = d.workspaces();
            let side = ws.iter().find(|w| w.name == "side").expect("it exists");
            assert!(side.open, "you land in what you just made");
            assert_eq!(side.projects, 0, "and it starts empty");
            assert_eq!(names_of(&d).len(), 0, "so the tree is empty too");

            // Declared, not implied: an empty workspace has no project in
            // the file to imply it, so it would not survive a restart.
            let written = std::fs::read_to_string(dir.join("projects.toml")).unwrap();
            assert!(
                written.contains("[[workspace]]") && written.contains(r#"name = "side""#),
                "the declaration must be written out:\n{written}"
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
            let d = Daemon::new(config_with_workspaces());
            d.create_workspace("side").unwrap();
            drop(d);

            let reloaded = Daemon::new(crate::config::load().unwrap());
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
            let d = Daemon::new(config_with_workspaces());
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
            let d = Daemon::new(config_with_workspaces());
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
        let repo = git2::Repository::init(dir.path()).unwrap();
        std::fs::write(dir.path().join("a.txt"), "one\n").unwrap();
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

        let d = daemon_with_primary(&dir.path().to_string_lossy());
        (dir, d)
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
    fn daemon_for_restore(dir: &std::path::Path) -> Arc<Daemon> {
        Daemon::new(ConfigFile {
            workspaces: Vec::new(),
            projects: vec![ProjectConfig {
                name: "proj".to_string(),
                root: None,
                repos: vec![dir.to_string_lossy().to_string()],
                workspace: None,
            }],
            agents: vec![AgentConfig {
                name: "test-agent".to_string(),
                cmd: vec![if cfg!(windows) { "cmd" } else { "sh" }.to_string()],
                env: Default::default(),
                harness: None,
            }],
            harnesses: Vec::new(),
        })
    }

    /// Writes a session file as a previous daemon would have left it.
    /// Cheaper and more exact than running one: what is being tested is
    /// what the daemon does with the file, not the file format twice.
    fn record(panes: &[(PaneKind, &str)], checkout: &std::path::Path) {
        crate::session::save(&crate::session::Session {
            panes: panes
                .iter()
                .map(|(kind, title)| crate::session::SessionPane {
                    checkout_path: checkout.to_path_buf(),
                    kind: *kind,
                    title: title.to_string(),
                    template: Some(title.to_string()),
                    status: PaneStatus::Idle,
                    note: None,
                    harness_session_id: None,
                })
                .collect(),
        });
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
        Daemon::new(ConfigFile {
            workspaces: Vec::new(),
            projects: vec![ProjectConfig {
                name: "proj".into(),
                root: None,
                repos: vec![dir.to_string_lossy().to_string()],
                workspace: None,
            }],
            agents: names
                .iter()
                .map(|name| AgentConfig {
                    name: (*name).into(),
                    cmd: persistent_agent_command(),
                    env: Default::default(),
                    harness: Some("claude".into()),
                })
                .collect(),
            harnesses: Vec::new(),
        })
    }

    fn record_agents(checkout: &std::path::Path, agents: &[(&str, Option<&str>)]) {
        crate::session::save(&crate::session::Session {
            panes: agents
                .iter()
                .map(|(template, session_id)| crate::session::SessionPane {
                    checkout_path: checkout.to_path_buf(),
                    kind: PaneKind::Agent,
                    title: (*template).into(),
                    template: Some((*template).into()),
                    status: PaneStatus::Idle,
                    note: None,
                    harness_session_id: session_id.map(str::to_string),
                })
                .collect(),
        });
    }

    fn close_all(d: &Daemon) {
        for p in &d.snapshot()[0].repositories[0].checkouts[0].panes {
            let _ = d.close_pane(p.id);
        }
    }

    fn saved_panes() -> Vec<crate::session::SessionPane> {
        crate::session::load().unwrap().panes
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
            d.persist_session();
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
            crate::session::save(&crate::session::Session {
                panes: vec![crate::session::SessionPane {
                    checkout_path: dir.path().to_path_buf(),
                    kind: PaneKind::Agent,
                    title: "review parser".to_string(),
                    template: Some("test-agent".to_string()),
                    status: PaneStatus::NeedsReview,
                    note: Some("ready to inspect".to_string()),
                    harness_session_id: None,
                }],
            });

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
    async fn an_agent_that_renamed_itself_still_comes_back() {
        // Regression: an agent is spawned by template name, and a renamed
        // pane's title is no longer that. Restoring by title would look up
        // a template called "fixing the pty deadlock" and find nothing.
        let dir = tempfile::tempdir().unwrap();
        with_temp_config(|_| {
            crate::session::save(&crate::session::Session {
                panes: vec![crate::session::SessionPane {
                    checkout_path: dir.path().to_path_buf(),
                    kind: PaneKind::Agent,
                    title: "fixing the pty deadlock".to_string(),
                    template: Some("test-agent".to_string()),
                    status: PaneStatus::Idle,
                    note: None,
                    harness_session_id: None,
                }],
            });

            let d = daemon_for_restore(dir.path());
            d.restore_session();

            let panes = &d.snapshot()[0].repositories[0].checkouts[0].panes;
            assert_eq!(panes.len(), 1, "the renamed agent should be back");
            assert_eq!(
                panes[0].title, "test-agent",
                "back under its template's name"
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

            std::env::set_var(crate::session::NO_RESTORE, "1");
            let d = daemon_for_restore(dir.path());
            d.restore_session();
            std::env::remove_var(crate::session::NO_RESTORE);

            assert!(d.snapshot()[0].repositories[0].checkouts[0]
                .panes
                .is_empty());
        });
    }

    #[test]
    fn a_broken_session_is_left_untouched_for_recovery() {
        let dir = tempfile::tempdir().unwrap();
        with_temp_config(|cfg| {
            let path = cfg.join("session.json");
            let broken = b"{ incomplete";
            std::fs::write(&path, broken).unwrap();

            let d = daemon_for_restore(dir.path());
            assert!(!d.restore_session(), "main must not enable persistence");
            assert_eq!(std::fs::read(&path).unwrap(), broken);
        });
    }

    #[tokio::test]
    async fn a_daemon_that_was_never_told_to_persist_records_nothing() {
        // Every test builds a daemon; none of them may write over the real
        // user's session.
        let dir = tempfile::tempdir().unwrap();
        with_temp_config(|cfg| {
            let d = daemon_for_restore(dir.path());
            let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;
            d.spawn_shell(checkout).unwrap();
            close_all(&d);

            assert!(
                !cfg.join("session.json").exists(),
                "persistence must be opt-in"
            );
        });
    }

    #[tokio::test]
    async fn a_pane_you_closed_does_not_come_back() {
        // The file follows the tree, so closing one forgets it.
        let dir = tempfile::tempdir().unwrap();
        with_temp_config(|_| {
            let d = daemon_for_restore(dir.path());
            d.persist_session();
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
            d.persist_session();
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

            let d = daemon_with_fake_claude(dir.path());
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
            let d = daemon_with_claude_aliases(dir.path(), &["first", "second"]);
            d.restore_session();

            assert_eq!(resuming_panes(&d), 2, "exact IDs need no broad claim guard");
            let mut ids: Vec<_> = d
                .session()
                .panes
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
            let d = daemon_with_claude_aliases(dir.path(), &["first", "second"]);
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
