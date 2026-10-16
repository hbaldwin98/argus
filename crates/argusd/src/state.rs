//! The daemon's tree: what it is, and what a client is shown of it.
//!
//! One `Daemon` type behind a small set of mutexes. Everything done *to*
//! the tree lives in a sibling module named after it — `build` for
//! constructing one, `panes` for a pane's lifecycle, `agents` for what an
//! agent reports about itself, `viewers` for what each client shows (the
//! size it asks a pty for, and the view that says where the person is),
//! `git_ops` for the writes to Git, `git_reads` for the port its reads
//! go through, `pushes` for what is published for connections to push,
//! `sync` for the polls and watchers that keep the tree level with the
//! disk, `panel` for the rows the user adds and removes, `workspaces` for
//! which scope is open,
//! `extensions` for what extensions publish and what goes back to them,
//! `session` for what survives a restart, `sessions`
//! for the agent sessions a repository's harnesses kept, `inbox` for
//! the way what Argus says reaches an agent, and `tree` for finding your
//! way around.
//!
//! The type and its locking are one thing; only which file a concern is
//! read in is split.
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use argus_protocol::{
    CheckoutId, CheckoutInfo, Focus, GitStatus, IdGen, PaneId, PaneInfo, PaneKind,
    PaneStatus, ProjectId, ProjectInfo, RepositoryId, RepositoryInfo, View,
    WorkspaceId, WorkspaceInfo,
};
use tokio::sync::broadcast;

mod agent_launch;
mod agents;
mod asker;
mod board_address;
mod board_parts;
mod build;
mod clock;
mod decisions;
mod extension_consoles;
mod extension_host;
mod extensions;
mod features;
mod follow;
mod diagrams;
mod tasks;
mod reconcile;
mod saved_views;
mod tracker_map;
mod view_timers;
mod views;
mod git_ops;
mod git_reads;
mod inbox;
mod panel;
mod panes;
mod processes;
mod pushes;
mod session;
mod sessions;
mod sync;
mod tree;
mod viewers;
mod workspaces;

pub use board_parts::FeaturePart;
pub(crate) use features::Filing;
pub use extensions::ExtensionNews;
pub use pushes::{BoardNews, Pushes, TreeNews};
pub use saved_views::SavedViewNews;
pub use views::ViewNews;
pub use git_ops::BranchDeletion;
pub use viewers::ViewerId;
use viewers::Viewers;
use crate::hook_server::{gen_token, valid_session_id};
use session::{agent_args, first_message, nothing_to_resume, Recorded, Resumed};
use tree::*;

use crate::config::{self, AgentConfig, ConfigFile};
use crate::paths::same_path;
use crate::pty;

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
    /// Whether the agent's last turn ended with work still running behind
    /// it. Said by the report that ended the turn, and by each one since.
    background: bool,
    /// Where the agent's last turn ended, as its harness reported it. Live
    /// only; the row follows the agent when this changes. See `follow`.
    directory: Option<PathBuf>,
    /// Live only: model, context, spend and tool, as the harness last
    /// reported them. Not persisted; a restored agent reports afresh.
    telemetry: argus_protocol::AgentTelemetry,
    /// When `status` last changed, for a client to say how long the pane
    /// has been where it is.
    status_since: std::time::SystemTime,
    /// The inbox what is said to its agent goes through instead of its
    /// terminal, while its harness's adapter holds one open. See `inbox`.
    inbox: inbox::Inbox,
    /// A hook won the race with session restoration, so saved metadata must
    /// not overwrite what the newly started process already reported.
    restore_status_reported: bool,
    restore_title_reported: bool,
    /// Set while this pane is a conversation Argus asked a CLI to reopen,
    /// and it is too early to be sure it could. See [`Resumed`].
    resumed: Option<Resumed>,
    runtime: Box<dyn processes::Process>,
}

impl Pane {
    /// A pane as it enters the tree, before anything has been heard from
    /// it. Every default lives here, so a new field is added in one place
    /// and a start that knows more sets only what it knows.
    fn new(id: PaneId, kind: PaneKind, title: String, runtime: Box<dyn processes::Process>) -> Self {
        Self {
            id,
            kind,
            title,
            status: PaneStatus::Idle,
            note: None,
            template: None,
            harness: None,
            harness_session_id: None,
            children: Vec::new(),
            background: false,
            directory: None,
            telemetry: Default::default(),
            status_since: std::time::SystemTime::now(),
            inbox: None,
            restore_status_reported: false,
            restore_title_reported: false,
            resumed: None,
            runtime,
        }
    }

    /// Whether an agent is running in this pane, and so can be spoken to.
    fn is_running_agent(&self) -> bool {
        self.kind == PaneKind::Agent && !matches!(self.status, PaneStatus::Exited { .. })
    }
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

/// Whether a spawn opens a new conversation, continues the one the pane
/// had before the daemon stopped, or reopens one the person picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Start {
    Fresh,
    Resuming,
    /// A past conversation the person chose to resume by its id. Unlike a
    /// restore it is their own start: held to the one-agent rule as a
    /// fresh one is, and a CLI that refuses the id is left to say so rather
    /// than quietly replaced by a new conversation.
    Picked,
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
    /// How far each of `branches` is ahead of the main branch, counted on
    /// the same refresh: walking history is git I/O like the rest.
    unmerged: std::collections::BTreeMap<String, usize>,
}

impl Repository {
    /// The configured working directory, which is what names a repository
    /// in the store.
    fn primary_path(&self) -> Option<&std::path::Path> {
        self.checkouts.iter().find(|c| c.primary).map(|c| c.path.as_path())
    }
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
    settings: ProjectSettings,
}

/// Everything a project takes from its `projects.toml` entry beyond what
/// shapes the tree. Built in one place so a new key is read in one place:
/// startup and a config reload both go through `from_config`, and a
/// project added at runtime starts from `Default`.
#[derive(Default)]
struct ProjectSettings {
    /// Where this project's worktrees are created, if it says. See
    /// `worktree_context`.
    worktree_root: Option<PathBuf>,
    /// Commands run in a worktree this project has just created, in order.
    setup: Vec<String>,
    /// Whether a checkout here may hold only one agent at a time.
    exclusive: bool,
    /// What this project's root scan may and may not walk into.
    scan: crate::git::Scan,
}

impl ProjectSettings {
    fn from_config(p: &config::ProjectConfig) -> Self {
        Self {
            worktree_root: p.worktree_root.as_deref().map(config::expand_home),
            setup: p.setup.clone(),
            exclusive: p.exclusive,
            scan: crate::git::Scan {
                exclude: p.exclude.clone(),
                include: p.include.clone(),
            },
        }
    }
}

struct Workspace {
    id: WorkspaceId,
    name: String,
}

struct Inner {
    workspaces: Vec<Workspace>,
    projects: Vec<Project>,
    /// Shells that belong to no checkout, in the order they were started.
    /// Outside every workspace, so switching one never hides them.
    consoles: Vec<Pane>,
    /// The panes running extensions' console views. Apart from `consoles`
    /// because they are no tab of the person's and are not brought back.
    extension_consoles: Vec<extension_consoles::HeldConsole>,
    ids: IdGen,
    /// Exactly one workspace is open at a time, daemon-global. Panes in the
    /// others keep running; they are just not in the tree clients render.
    open: WorkspaceId,
    /// Repository paths the user has removed from the panel. A scan finds
    /// them again every ten seconds, so without this the row would come
    /// straight back (see [`crate::store::Store::excluded_repos`]).
    excluded: Vec<PathBuf>,
    /// When each repository last had a pane close in it, by its primary
    /// checkout's path, in seconds since the Unix epoch. Kept apart from
    /// `Repository` because a scan rebuilds those, and persisted so the
    /// rail's quiet repositories keep their order across a restart.
    used: HashMap<PathBuf, u64>,
}

pub struct Daemon {
    inner: StdMutex<Inner>,
    /// Hooks can fire after the child is spawned but before its pane is
    /// inserted into `inner`. Keep their latest bounded state until insertion.
    starting_agents: StdMutex<HashMap<PaneId, PendingStart>>,
    /// What is published for the connections to push.
    pushes: pushes::Pushes,
    /// What agents have shown, in memory only. Changes are sent while this
    /// is locked, so a watcher's replay and the events after it do not cross.
    agent_views: StdMutex<views::Views>,
    /// The views agents kept, and what clients were last told of them.
    /// Changes are sent while this is locked, as the agents' are.
    saved_views: StdMutex<saved_views::Held>,
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
    /// The panes the store was last given, so a broadcast that changed
    /// nothing it keeps — telemetry, a child's status — writes nothing.
    /// Held across computing and saving, which also keeps two recordings
    /// from landing in the opposite order to the states they read.
    recorded: StdMutex<Recorded>,
    /// Runtime state that outlives this run. Which store a daemon holds is
    /// what decides whether it persists at all: `new` gives it one that
    /// lives and dies with the process, so a daemon built in a test cannot
    /// write over the real user's state, and only `with_store` reaches disk.
    store: crate::store::Store,
    /// How many times a template has been restarted in a checkout lately,
    /// and when that run of restarts began. Keeps a CLI that dies on
    /// every start from being restarted forever.
    restart_attempts: StdMutex<HashMap<(CheckoutId, String), (u32, std::time::Instant)>>,
    /// The board key each repository's checkout path files under. Reading
    /// one opens the repository, so it is read once per path, and a pushed
    /// board can name the repositories it belongs to without opening them
    /// all on every write.
    board_keys: StdMutex<HashMap<PathBuf, String>>,
    /// Every attached client's requested size for the panes it is showing.
    /// A pty has one size and clients do not have to agree on it, so the
    /// sizes are collected here and reconciled rather than applied as they
    /// arrive.
    viewers: StdMutex<Viewers>,
    next_viewer: std::sync::atomic::AtomicU64,
    /// Names each inbox connection, so a stale one closing cannot close
    /// the one that replaced it.
    next_inbox: std::sync::atomic::AtomicU64,
    /// What the running extensions publish, and the dialogs they have open.
    /// See `extensions`.
    extensions: StdMutex<extensions::Registry>,
    /// Where a harness's plugin folder is written (see
    /// `harness::plugin_dir`); unset until `main` names it, so a daemon
    /// built in a test writes none.
    plugin_root: std::sync::OnceLock<std::path::PathBuf>,
    /// Where a file pasted into a pane is kept (see `pastes`); unset until
    /// `main` names it, so a daemon built in a test takes no files.
    paste_root: std::sync::OnceLock<std::path::PathBuf>,
    /// Every read of Git the tree is kept level by. See `git_reads`.
    git: Arc<dyn git_reads::GitReads>,
    /// The one turn at generating a review that every client waits on.
    reviews: crate::review_queue::ReviewQueue,
    /// What every pane's process is started through: a pty, unless a test
    /// is scripting when each one exits.
    processes: Arc<dyn processes::Processes>,
    /// Where the durations the daemon decides on are read from.
    clock: Arc<dyn clock::Clock>,
}

/// A project as clients are shown it.
fn project_info(p: &Project, used: &HashMap<PathBuf, u64>) -> ProjectInfo {
    ProjectInfo {
        id: p.id,
        name: p.name.clone(),
        root: p.root.as_ref().map(|path| path.to_string_lossy().to_string()),
        repositories: p
            .repositories
            .iter()
            .map(|r| RepositoryInfo {
                id: r.id,
                name: r.name.clone(),
                branches: r.branches.clone(),
                default_branch: r.default_branch.clone(),
                remote_branches: r.remote_branches.clone(),
                unmerged: r.unmerged.clone(),
                last_used: r.primary_path().and_then(|path| used.get(path).copied()),
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
                            panes: c.panes.iter().map(pane_info).collect(),
                            git,
                            primary: c.primary,
                        }
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn pane_info(pane: &Pane) -> PaneInfo {
    PaneInfo {
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
                label: c.label.clone().unwrap_or_else(|| "agent".to_string()),
                status: c.status,
                note: c.note.clone(),
            })
            .collect(),
        telemetry: pane.telemetry.clone(),
        background: pane.background,
        since: pane
            .status_since
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs()),
        live: pane.inbox.is_some(),
    }
}

use crate::pty::PaneSubscription;

impl Daemon {
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
        let inner = self.inner.lock().unwrap();
        let open = inner.open;
        inner
            .projects
            .iter()
            .filter(|p| p.workspace == open)
            .map(|p| project_info(p, &inner.used))
            .collect()
    }

    /// Every console, in the order they were started. Sent beside each
    /// tree rather than carried by the broadcast, so a change to a console
    /// rides the tree's broadcast and nothing else.
    pub fn consoles(&self) -> Vec<PaneInfo> {
        self.inner.lock().unwrap().consoles.iter().map(pane_info).collect()
    }

    /// What is published for the connections to push.
    pub fn pushes(&self) -> &Pushes {
        &self.pushes
    }

    pub fn subscribe_workspaces(&self) -> broadcast::Receiver<Vec<WorkspaceInfo>> {
        self.pushes.subscribe_workspaces()
    }

    fn broadcast_tree(&self) {
        // Structural changes only — pane output never reaches here — so
        // recording the session on the same edge is cheap and means the
        // file is never more than one spawn or close out of date.
        self.record_session();
        self.pushes.tree(|| self.snapshot());
        self.push_saved_views();
    }

    /// The checkout at `path`, whatever workspace it is in.
    fn checkout_at(&self, path: &std::path::Path) -> Option<CheckoutId> {
        let inner = self.inner.lock().unwrap();
        let found = checkouts(&inner.projects)
            .find(|c| same_path(&c.path, path))
            .map(|c| c.id);
        found
    }

    /// A client's place in the review queue, for as long as it holds it.
    pub fn review_lane<R>(&self) -> crate::review_queue::ReviewLane<R> {
        self.reviews.lane()
    }

    fn broadcast_workspaces(&self) {
        self.pushes.workspaces(self.workspaces());
    }

    /// Binds the loopback pane API hook commands POST to (see
    /// `hooks::install_claude_hooks`) and starts serving it in the
    /// background. The bind itself is synchronous so `hook_port` is set
    /// before the daemon's client socket starts accepting — no window where
    /// a client could spawn an agent whose hooks point nowhere.
    pub fn start_hook_server(self: &Arc<Self>) -> anyhow::Result<()> {
        let port = crate::hook_server::start(self.clone())?;
        self.hook_port.store(port, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    /// The bearer token every pane-API request has to carry.
    pub(crate) fn hook_token(&self) -> &str {
        &self.hook_token
    }
}

#[cfg(test)]
mod tests;
