use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use argus_protocol::{
    Cell, CheckoutId, CheckoutInfo, IdGen, PaneId, PaneInfo, PaneKind, PaneStatus, ProjectId,
    ProjectInfo, ServerMsg, WorkspaceId, WorkspaceInfo,
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
    /// Set while this pane is a conversation Argus asked a CLI to reopen,
    /// and it is too early to be sure it could. See [`Resumed`].
    resumed: Option<Resumed>,
    runtime: PaneRuntime,
}

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
}

struct Project {
    id: ProjectId,
    name: String,
    /// Which workspace this project is filed under. The tree a client sees
    /// is scoped to whichever workspace is open (DESIGN.md §11).
    workspace: WorkspaceId,
    checkouts: Vec<Checkout>,
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
}

pub struct Daemon {
    inner: StdMutex<Inner>,
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

        let projects = config
            .projects
            .into_iter()
            .map(|p| Project {
                id: ProjectId(ids.alloc()),
                workspace: match p.workspace.as_deref() {
                    Some(name) => intern(&mut workspaces, &mut ids, name),
                    None => default_ws,
                },
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
                            primary: true,
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
        let harnesses = config::harnesses(config.harnesses);

        // Reopen whatever was open last time, if that workspace still
        // exists — a name can disappear from the config between runs.
        let open = config::load_open_workspace()
            .and_then(|name| workspaces.iter().find(|w| w.name == name).map(|w| w.id))
            .unwrap_or(default_ws);

        let (tree_tx, _) = broadcast::channel(32);
        let (workspaces_tx, _) = broadcast::channel(32);
        Arc::new(Daemon {
            inner: StdMutex::new(Inner {
                workspaces,
                projects,
                ids,
                open,
            }),
            workspaces_tx,
            tree_tx,
            templates,
            harnesses,
            hook_port: std::sync::atomic::AtomicU16::new(0),
            hook_token: gen_token(),
            restoring: std::sync::atomic::AtomicBool::new(false),
            persist: std::sync::atomic::AtomicBool::new(false),
        })
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
            .flat_map(|p| p.checkouts.iter())
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
                                note: pane.note.clone(),
                                template: pane.template.clone(),
                            })
                            .collect(),
                        git: crate::git::status(&c.path),
                        primary: c.primary,
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
                        .flat_map(|p| p.checkouts.iter())
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
                .flat_map(|p| p.checkouts.iter())
                .flat_map(|c| {
                    c.panes
                        .iter()
                        .filter(|pane| !matches!(pane.status, PaneStatus::Exited { .. }))
                        .map(|pane| crate::session::SessionPane {
                            checkout_path: c.path.clone(),
                            kind: pane.kind,
                            title: pane.title.clone(),
                            template: pane.template.clone(),
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
        let mut claimed: Vec<(PathBuf, &str)> = Vec::new();
        for pane in &wanted {
            let Some(checkout) = self.checkout_at(&pane.checkout_path) else {
                continue;
            };
            let result = match pane.kind {
                PaneKind::Agent => {
                    // A resume argument names "the last conversation here",
                    // not a particular one, so two panes of the same agent
                    // in one checkout would both reopen the same session
                    // and write over each other. The first pane claims it;
                    // the rest come back as new agents.
                    let key = (pane.checkout_path.clone(), pane.template());
                    let start = if claimed.contains(&key) {
                        Start::Fresh
                    } else {
                        claimed.push(key);
                        Start::Resuming
                    };
                    self.start_agent(checkout, pane.template(), start).map(|_| ())
                }
                _ => self.spawn_shell(checkout).map(|_| ()),
            };
            match result {
                Ok(()) => restored += 1,
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
            .flat_map(|p| p.checkouts.iter())
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
        self.start_agent(checkout, template_name, Start::Fresh)
    }

    fn start_agent(
        self: &Arc<Self>,
        checkout: CheckoutId,
        template_name: &str,
        start: Start,
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

        let (args, resuming) = agent_args(rest, &harness.resume, start);

        let spec = pty::Spawn::Program {
            program: program.clone(),
            args,
            env,
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
                    status: PaneStatus::Idle,
                    note: None,
                    template: Some(template.name.clone()),
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
            if let Err(e) = self.start_agent(r.checkout, &r.template, Start::Fresh) {
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
        let (rows, cols, cells, cursor) = p.runtime.full_snapshot();
        let rx = p.runtime.subscribe();
        Ok((rows, cols, cells, cursor, rx))
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
    fn set_pane_hook_status(&self, pane: PaneId, status: PaneStatus, note: Option<String>) {
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            match find_pane(&mut inner.projects, pane) {
                Some(p) if !matches!(p.status, PaneStatus::Exited { .. }) => {
                    // A note explains one state; the report that leaves that
                    // state takes it away with it, so a stale "waiting for
                    // the db password" can't sit under a working row.
                    let note = note.map(|n| clean_title(&n)).filter(|n| !n.is_empty());
                    let changed = p.status != status || p.note != note;
                    p.status = status;
                    p.note = note;
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
                    let _ = daemon.tree_tx.send(daemon.snapshot());
                })
                .await;
            }
        });
    }

    /// Reconciles each project's checkouts against `git worktree list` on
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
                let Some(primary_path) = project
                    .checkouts
                    .iter()
                    .find(|c| c.primary)
                    .map(|c| c.path.clone())
                else {
                    continue;
                };
                let listed = list(&primary_path);
                if listed.is_empty() {
                    // Not a git repo (or `git` failed) — nothing to
                    // reconcile against, and never treat this as "every
                    // worktree was removed".
                    continue;
                }

                for path in &listed {
                    if project.checkouts.iter().any(|c| same_path(&c.path, path)) {
                        continue;
                    }
                    let is_primary = same_path(path, &primary_path);
                    let id = CheckoutId(ids.alloc());
                    let name = worktree_display_name(path, is_primary);
                    project.checkouts.push(Checkout {
                        id,
                        name,
                        path: path.clone(),
                        primary: is_primary,
                        panes: Vec::new(),
                    });
                }

                let mut i = 0;
                while i < project.checkouts.len() {
                    let gone = !project.checkouts[i].primary
                        && !listed
                            .iter()
                            .any(|path| same_path(path, &project.checkouts[i].path));
                    if gone {
                        orphaned_panes.extend(project.checkouts.remove(i).panes);
                    } else {
                        i += 1;
                    }
                }
            }
        }
        for pane in orphaned_panes {
            let _ = pane.runtime.kill();
        }
    }

    /// Adds a brand-new project rooted at an arbitrary directory — not
    /// restricted to whatever's already in `projects.toml` or wherever the
    /// daemon happens to be running from — and persists it so it survives
    /// a restart. The project gets exactly one (primary) checkout, at
    /// `path` itself.
    pub fn add_project(&self, path: &str) -> anyhow::Result<()> {
        let expanded = config::expand_home(path);
        if !expanded.is_dir() {
            anyhow::bail!("not a directory: {}", expanded.display());
        }
        let name = expanded
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string());

        // New projects land in whichever workspace is open, so "add this
        // directory" means "add it to what I am looking at".
        let (workspace, workspace_name) = self.open_workspace_ref();
        config::append_project(&name, &expanded, &workspace_name)?;

        {
            let mut inner = self.inner.lock().unwrap();
            let project_id = ProjectId(inner.ids.alloc());
            let checkout_id = CheckoutId(inner.ids.alloc());
            inner.projects.push(Project {
                id: project_id,
                workspace,
                name: name.clone(),
                checkouts: vec![Checkout {
                    id: checkout_id,
                    name,
                    path: expanded,
                    primary: true,
                    panes: Vec::new(),
                }],
            });
        }
        self.broadcast_tree();
        // The rollup counts changed too.
        self.broadcast_workspaces();
        Ok(())
    }

    /// `git worktree add`s a new checkout in `base`'s project, branched off
    /// `base`'s current HEAD, and appends it to the tree. Placed under
    /// `.argus/worktrees/<branch>` beside the project's primary checkout
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

        let (project_id, base_path, primary_path) = {
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

        {
            let mut inner = self.inner.lock().unwrap();
            let id = CheckoutId(inner.ids.alloc());
            if let Some(p) = find_project(&mut inner.projects, project_id) {
                p.checkouts.push(Checkout {
                    id,
                    name: branch,
                    path: dest,
                    primary: false,
                    panes: Vec::new(),
                });
            }
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
        .flat_map(|p| p.checkouts.iter_mut())
        .find(|c| c.id == id)
}

fn find_checkout_ref(projects: &[Project], id: CheckoutId) -> Option<&Checkout> {
    projects
        .iter()
        .flat_map(|p| p.checkouts.iter())
        .find(|c| c.id == id)
}

fn find_project(projects: &mut [Project], id: ProjectId) -> Option<&mut Project> {
    projects.iter_mut().find(|p| p.id == id)
}

/// For a checkout, the id of its owning project, that checkout's own path
/// (the base to branch off / run `git worktree` commands from), and its
/// project's primary checkout path (where new worktrees get placed).
fn find_checkout_context(
    projects: &[Project],
    id: CheckoutId,
) -> Option<(ProjectId, PathBuf, PathBuf)> {
    projects.iter().find_map(|p| {
        let base = p.checkouts.iter().find(|c| c.id == id)?;
        let primary = p.checkouts.iter().find(|c| c.primary).unwrap_or(base);
        Some((p.id, base.path.clone(), primary.path.clone()))
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
        if let Some(pos) = project.checkouts.iter().position(|c| c.id == id) {
            return Some(project.checkouts.remove(pos));
        }
    }
    None
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

/// Whether any agent pane is still open in the checkout at `path`. Gates
/// tearing down that checkout's managed hooks, which are shared by every
/// agent running there.
fn checkout_has_agent(projects: &[Project], path: &std::path::Path) -> bool {
    projects
        .iter()
        .flat_map(|p| p.checkouts.iter())
        .filter(|c| c.path == path)
        .any(|c| c.panes.iter().any(|p| p.kind == PaneKind::Agent))
}

/// Removes a pane from whichever checkout holds it, returning it along with
/// that checkout's path — which the caller can't look up afterwards, the
/// pane being gone by then.
fn remove_pane_with_checkout(projects: &mut [Project], id: PaneId) -> Option<(Pane, PathBuf)> {
    for project in projects.iter_mut() {
        for checkout in project.checkouts.iter_mut() {
            if let Some(pos) = checkout.panes.iter().position(|p| p.id == id) {
                return Some((checkout.panes.remove(pos), checkout.path.clone()));
            }
        }
    }
    None
}

/// Reads one HTTP/1.1 request (headers only — the hook commands we install
/// never send a body), checks the bearer token, and applies the status the
/// path encodes. Hand-rolled rather than pulling in an HTTP server crate:
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
                daemon.set_pane_hook_status(pane, report.status(), Some(note))
            }
            Some((pane, Endpoint::Title)) => {
                daemon.set_pane_title(pane, &String::from_utf8_lossy(&body))
            }
            None => {}
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
}

/// `/pane/<id>/status/<working|idle|waiting>` and `/pane/<id>/title`.
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
fn agent_args(configured: &[String], resume: &[String], start: Start) -> (Vec<String>, bool) {
    let mut args = configured.to_vec();
    if start == Start::Fresh || resume.is_empty() {
        return (args, false);
    }
    args.extend(resume.iter().cloned());
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
                repos: vec![primary.to_string()],
                workspace: None,
            }],
            agents: Vec::new(),
            harnesses: Vec::new(),
        })
    }

    fn checkout_paths(d: &Daemon) -> Vec<String> {
        d.snapshot()
            .into_iter()
            .flat_map(|p| p.checkouts)
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
            parse_pane_path("/pane/7/title"),
            Some((PaneId(7), Endpoint::Title))
        );
    }

    #[test]
    fn a_pane_path_rejects_junk() {
        assert_eq!(parse_pane_path("/pane/7/status/Stop"), None, "an event name");
        assert_eq!(parse_pane_path("/pane/7/status/exited"), None, "ours to decide");
        assert_eq!(parse_pane_path("/nope/7/title"), None, "wrong prefix");
        assert_eq!(parse_pane_path("/pane/abc/title"), None, "non-numeric pane");
        assert_eq!(parse_pane_path("/pane/7"), None, "no endpoint");
        assert_eq!(parse_pane_path("/pane/7/title/extra"), None, "trailing junk");
        assert_eq!(parse_pane_path(""), None, "empty path");
    }

    #[test]
    fn a_title_from_a_model_is_flattened_and_cut_to_fit_a_row() {
        assert_eq!(clean_title("  fixing\n the   pty  "), "fixing the pty");
        let long = clean_title(&"x".repeat(200));
        assert!(long.chars().count() <= 49, "got {} chars", long.chars().count());
        assert!(long.ends_with('…'));
        assert_eq!(clean_title("   "), "");
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
            .flat_map(|p| p.checkouts)
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

        d.set_pane_hook_status(pane, PaneStatus::Failed, Some("cargo test won't build".into()));
        let info = pane_info(&d, pane);
        assert_eq!(info.status, PaneStatus::Failed);
        assert!(info.note.is_some());

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

    // --- worktree reconciliation -------------------------------------------

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

        let checkouts: Vec<_> = d.snapshot().into_iter().flat_map(|p| p.checkouts).collect();
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
            .flat_map(|p| p.checkouts)
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
        assert_eq!(tree[0].checkouts.len(), 2);
    }

    #[test]
    fn default_agent_templates_are_offered_when_config_has_none() {
        let d = daemon_with_primary("/repo");
        assert_eq!(d.template_names(), vec!["claude", "codex", "opencode"]);
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
            // `codex` is the honest exception: it has no hook mechanism at
            // all, so the environment really is all it gets.
            if name == "codex" {
                continue;
            }
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
        d.snapshot()[0].checkouts[0].id
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
                repos: vec![configured],
                workspace: None,
            }],
            agents: Vec::new(),
            harnesses: Vec::new(),
        });

        for _ in 0..3 {
            d.reconcile_worktrees();
        }
        let checkouts = &d.snapshot()[0].checkouts;
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
                repos: vec![dir.path().to_string_lossy().to_string()],
                workspace: None,
            }],
            agents: Vec::new(),
            harnesses: Vec::new(),
        });
        d.reconcile_worktrees();
        assert_eq!(d.snapshot()[0].checkouts.len(), 1);

        // Someone runs `git worktree add` in a shell.
        repo.worktree("feature", &dir.path().join("wt-feature"), None)
            .unwrap();

        d.reconcile_worktrees();
        let checkouts = &d.snapshot()[0].checkouts;
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
                    repos: vec!["/home-thing".to_string()],
                    workspace: None,
                },
                ProjectConfig {
                    name: "day-job".to_string(),
                    repos: vec!["/day-job".to_string()],
                    workspace: Some("work".to_string()),
                },
                ProjectConfig {
                    name: "side".to_string(),
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
                        repos: vec![dir.path().to_string_lossy().to_string()],
                        workspace: None,
                    },
                    ProjectConfig {
                        name: "elsewhere".to_string(),
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
            let checkout = d.snapshot()[0].checkouts[0].id;
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
    fn an_editor_pane_will_not_open_a_path_outside_the_checkout() {
        // `path` comes from a client and lands on a command line.
        let dir = tempfile::tempdir().unwrap();
        let d = daemon_with_primary(&dir.path().to_string_lossy());
        let checkout = d.snapshot()[0].checkouts[0].id;

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
    async fn creating_a_branch_moves_this_checkout_onto_it() {
        // Unlike `create_worktree`, which puts the branch in a directory of
        // its own and leaves this checkout where it was.
        let (dir, d) = daemon_on_a_repo();
        let checkout = d.snapshot()[0].checkouts[0].id;

        d.create_branch(checkout, "feature/x").await.unwrap();

        assert_eq!(head_of(dir.path()), "feature/x");
        assert_eq!(
            d.snapshot()[0].checkouts.len(),
            1,
            "no new checkout — that is what a worktree is for"
        );
    }

    #[tokio::test]
    async fn the_checkouts_name_follows_the_branch_it_moves_to() {
        let (_dir, d) = daemon_on_a_repo();
        let checkout = d.snapshot()[0].checkouts[0].id;

        d.create_branch(checkout, "feature/x").await.unwrap();

        assert_eq!(d.snapshot()[0].checkouts[0].name, "feature/x");
    }

    #[tokio::test]
    async fn switching_moves_between_branches_that_already_exist() {
        let (dir, d) = daemon_on_a_repo();
        let checkout = d.snapshot()[0].checkouts[0].id;
        let start = head_of(dir.path());
        d.create_branch(checkout, "other").await.unwrap();

        d.switch_branch(checkout, &start).await.unwrap();

        assert_eq!(head_of(dir.path()), start);
        assert_eq!(d.snapshot()[0].checkouts[0].name, start);
    }

    #[tokio::test]
    async fn switching_pushes_a_new_tree_so_every_client_sees_the_move() {
        let (_dir, d) = daemon_on_a_repo();
        let checkout = d.snapshot()[0].checkouts[0].id;
        let mut rx = d.subscribe_tree();

        d.create_branch(checkout, "feature/x").await.unwrap();

        let tree = rx.try_recv().expect("clients need to be told");
        assert_eq!(tree[0].checkouts[0].name, "feature/x");
    }

    #[tokio::test]
    async fn switching_to_a_branch_that_does_not_exist_reports_gits_own_words() {
        let (_dir, d) = daemon_on_a_repo();
        let checkout = d.snapshot()[0].checkouts[0].id;

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
        let checkout = d.snapshot()[0].checkouts[0].id;
        d.create_branch(checkout, "taken").await.unwrap();

        assert!(d.create_branch(checkout, "taken").await.is_err());
    }

    #[tokio::test]
    async fn an_empty_or_flag_like_branch_name_never_reaches_git() {
        // A leading dash would be parsed as an option rather than a name.
        let (_dir, d) = daemon_on_a_repo();
        let checkout = d.snapshot()[0].checkouts[0].id;

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
        let checkout = d.snapshot()[0].checkouts[0].id;
        std::fs::write(dir.path().join("a.txt"), "x").unwrap();

        let made = d.spawn_editor(
            checkout,
            "a.txt",
            None,
            false,
            Some("missing/notepad.exe"),
        );

        assert!(made.is_err(), "the deliberately missing editor must not launch");
        assert!(
            d.snapshot()[0].checkouts[0].panes.is_empty(),
            "a GUI editor must not become a pane"
        );
    }

    // --- session restore ----------------------------------------------------

    /// A daemon whose only project is `dir`, with one agent template that
    /// runs the platform shell so restoring one actually starts something.
    fn daemon_for_restore(dir: &std::path::Path) -> Arc<Daemon> {
        Daemon::new(ConfigFile {
            workspaces: Vec::new(),
            projects: vec![ProjectConfig {
                name: "proj".to_string(),
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
                })
                .collect(),
        });
    }

    fn close_all(d: &Daemon) {
        for p in &d.snapshot()[0].checkouts[0].panes {
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
            assert!(d.snapshot()[0].checkouts[0].panes.is_empty());
        });
    }

    #[tokio::test]
    async fn what_is_running_is_written_down() {
        let dir = tempfile::tempdir().unwrap();
        with_temp_config(|_| {
            let d = daemon_for_restore(dir.path());
            d.persist_session();
            let checkout = d.snapshot()[0].checkouts[0].id;
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

            let kinds: Vec<PaneKind> = d.snapshot()[0].checkouts[0]
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
    async fn an_agent_comes_back_as_the_template_it_was() {
        // The title is how a restored agent knows what to launch.
        let dir = tempfile::tempdir().unwrap();
        with_temp_config(|_| {
            record(&[(PaneKind::Agent, "test-agent")], dir.path());

            let d = daemon_for_restore(dir.path());
            d.restore_session();

            assert_eq!(d.snapshot()[0].checkouts[0].panes[0].title, "test-agent");
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
                }],
            });

            let d = daemon_for_restore(dir.path());
            d.restore_session();

            let panes = &d.snapshot()[0].checkouts[0].panes;
            assert_eq!(panes.len(), 1, "the renamed agent should be back");
            assert_eq!(panes[0].title, "test-agent", "back under its template's name");

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
                d.snapshot()[0].checkouts[0].panes.is_empty(),
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

            assert!(d.snapshot()[0].checkouts[0].panes.is_empty());
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

            assert!(d.snapshot()[0].checkouts[0].panes.is_empty());
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
            let checkout = d.snapshot()[0].checkouts[0].id;
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
            let checkout = d.snapshot()[0].checkouts[0].id;
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
            let checkout = d.snapshot()[0].checkouts[0].id;
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

            let checkouts = d.snapshot().remove(0).checkouts;
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
            .flat_map(|p| p.checkouts.iter())
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

            let panes = d.snapshot().remove(0).checkouts.remove(0).panes;
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
            Start::Fresh,
        );
        assert_eq!(args, vec!["--model", "opus"]);
        assert!(!resuming);
    }

    #[test]
    fn a_restored_agent_is_asked_to_continue_where_it_left_off() {
        let (args, resuming) = agent_args(
            &["--model".to_string(), "opus".to_string()],
            &["--continue".to_string()],
            Start::Resuming,
        );
        assert_eq!(
            args,
            vec!["--model", "opus", "--continue"],
            "after the template's own flags, which still apply"
        );
        assert!(resuming);
    }

    #[test]
    fn a_harness_that_cannot_resume_restores_the_old_way() {
        // Nothing to append, and nothing for a failed start to fall back
        // from — the pane must not be treated as a resume that went wrong.
        let (args, resuming) = agent_args(&["-q".to_string()], &[], Start::Resuming);
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
            .start_agent(only_checkout(&d), "claude", Start::Resuming)
            .unwrap();

        d.mark_pane_exited(pane, Some(1));

        let panes = d.snapshot().remove(0).checkouts.remove(0).panes;
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
            .start_agent(only_checkout(&d), "claude", Start::Resuming)
            .unwrap();

        d.mark_pane_exited(pane, Some(1));
        let replacement = d.snapshot().remove(0).checkouts.remove(0).panes[0].id;
        d.mark_pane_exited(replacement, Some(1));

        let panes = d.snapshot().remove(0).checkouts.remove(0).panes;
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
            .start_agent(only_checkout(&d), "claude", Start::Resuming)
            .unwrap();

        d.mark_pane_exited(pane, Some(0));

        let panes = d.snapshot().remove(0).checkouts.remove(0).panes;
        assert_eq!(panes.len(), 1);
        assert_eq!(panes[0].id, pane, "still the pane the user closed");
        assert_eq!(panes[0].status, PaneStatus::Exited { code: Some(0) });

        close_all(&d);
    }
}
