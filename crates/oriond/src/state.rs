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
    /// True for a repo's configured working directory, false for a linked
    /// worktree created via `create_worktree`. Gates removal (§4 Level 2).
    primary: bool,
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

        let (tree_tx, _) = broadcast::channel(32);
        Arc::new(Daemon {
            inner: StdMutex::new(Inner { projects, ids }),
            tree_tx,
            templates,
            hook_port: std::sync::atomic::AtomicU16::new(0),
            hook_token: gen_token(),
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
                        primary: c.primary,
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
                    status: PaneStatus::Idle,
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

        // Claude Code is the only dialect this understands so far (§11);
        // other templates stay on process-state status alone. Must land
        // before the process starts — Claude only reads hooks at its own
        // startup, not on later file changes.
        if template.name == "claude" {
            let port = self.hook_port.load(std::sync::atomic::Ordering::Relaxed);
            if port != 0 {
                if let Err(e) = crate::hooks::install_claude_hooks(&path, id, port, &self.hook_token) {
                    tracing::warn!("failed to install claude hooks in {}: {e}", path.display());
                }
            }
        }

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
                    status: PaneStatus::Idle,
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

    /// Binds the loopback HTTP status receiver hook commands POST to (see
    /// `hooks::install_claude_hooks`) and starts serving it in the
    /// background. The bind itself is synchronous so `hook_port` is set
    /// before the daemon's client socket starts accepting — no window where
    /// a client could spawn an agent whose hooks point nowhere.
    pub fn start_hook_server(self: &Arc<Self>) -> anyhow::Result<()> {
        let std_listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        std_listener.set_nonblocking(true)?;
        let port = std_listener.local_addr()?.port();
        self.hook_port.store(port, std::sync::atomic::Ordering::Relaxed);
        let listener = tokio::net::TcpListener::from_std(std_listener)?;

        let daemon = self.clone();
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else { break };
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
    fn set_pane_hook_status(&self, pane: PaneId, status: PaneStatus) {
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            match find_pane(&mut inner.projects, pane) {
                Some(p) => match hook_status_update(p.status, status) {
                    Some(next) => {
                        p.status = next;
                        true
                    }
                    None => false,
                },
                None => false,
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
                    daemon.broadcast_tree();
                })
                .await;
            }
        });
    }

    /// Reconciles each project's checkouts against `git worktree list` on
    /// its primary checkout, so a worktree created or removed outside
    /// Orion — a bare `git worktree add`/`remove` from a shell — still
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
            let Inner { projects, ids } = &mut *guard;
            for project in projects.iter_mut() {
                let Some(primary_path) = project.checkouts.iter().find(|c| c.primary).map(|c| c.path.clone())
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
                    if project.checkouts.iter().any(|c| &c.path == path) {
                        continue;
                    }
                    let is_primary = *path == primary_path;
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
                    let gone = !project.checkouts[i].primary && !listed.contains(&project.checkouts[i].path);
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

        config::append_project(&name, &expanded)?;

        {
            let mut inner = self.inner.lock().unwrap();
            let project_id = ProjectId(inner.ids.alloc());
            let checkout_id = CheckoutId(inner.ids.alloc());
            inner.projects.push(Project {
                id: project_id,
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
        Ok(())
    }

    /// `git worktree add`s a new checkout in `base`'s project, branched off
    /// `base`'s current HEAD, and appends it to the tree. Placed under
    /// `.orion/worktrees/<branch>` beside the project's primary checkout
    /// (DESIGN.md §4 Level 2), regardless of which checkout `base` itself
    /// is — so worktrees always nest under the one directory, not under
    /// each other.
    pub async fn create_worktree(self: &Arc<Self>, base: CheckoutId, branch: String) -> anyhow::Result<()> {
        let branch = branch.trim().to_string();
        if branch.is_empty() {
            anyhow::bail!("branch name can't be empty");
        }

        let (project_id, base_path, primary_path) = {
            let inner = self.inner.lock().unwrap();
            find_checkout_context(&inner.projects, base).ok_or_else(|| anyhow::anyhow!("no such checkout"))?
        };
        let dest = primary_path.join(".orion").join("worktrees").join(&branch);

        let output = tokio::process::Command::new("git")
            .args(["worktree", "add", "-b", &branch])
            .arg(&dest)
            .current_dir(&base_path)
            .output()
            .await?;
        if !output.status.success() {
            anyhow::bail!("git worktree add failed: {}", String::from_utf8_lossy(&output.stderr).trim());
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
    /// checkout, which is the repo the user already had, not Orion's to
    /// delete (DESIGN.md §4 Level 2).
    pub async fn remove_checkout(&self, checkout: CheckoutId) -> anyhow::Result<()> {
        let (path, primary, primary_path, pane_ids) = {
            let inner = self.inner.lock().unwrap();
            let c = find_checkout_ref(&inner.projects, checkout).ok_or_else(|| anyhow::anyhow!("no such checkout"))?;
            let (_, _, primary_path) =
                find_checkout_context(&inner.projects, checkout).ok_or_else(|| anyhow::anyhow!("no such checkout"))?;
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

        let output = tokio::process::Command::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(&path)
            .current_dir(&primary_path)
            .output()
            .await?;
        if !output.status.success() {
            anyhow::bail!("git worktree remove failed: {}", String::from_utf8_lossy(&output.stderr).trim());
        }

        if let Some(branch) = branch {
            let _ = tokio::process::Command::new("git")
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
fn find_checkout_context(projects: &[Project], id: CheckoutId) -> Option<(ProjectId, PathBuf, PathBuf)> {
    projects.iter().find_map(|p| {
        let base = p.checkouts.iter().find(|c| c.id == id)?;
        let primary = p.checkouts.iter().find(|c| c.primary).unwrap_or(base);
        Some((p.id, base.path.clone(), primary.path.clone()))
    })
}

/// Prefers the checked-out branch name for a newly-discovered worktree —
/// matches how `create_worktree` names ones Orion made itself — falling
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

/// Reads one HTTP/1.1 request (headers only — the hook commands we install
/// never send a body), checks the bearer token, and applies the status the
/// path encodes. Hand-rolled rather than pulling in an HTTP server crate:
/// the request shape is entirely our own (we generate every hook command
/// that ever calls this), so there's nothing to be robust against beyond
/// "well-formed or ignored".
async fn handle_hook_request(stream: tokio::net::TcpStream, daemon: Arc<Daemon>) -> anyhow::Result<()> {
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
        if let Some(v) = line.strip_prefix("Authorization:").or_else(|| line.strip_prefix("authorization:")) {
            authorized = v.trim().eq_ignore_ascii_case(&format!("Bearer {}", daemon.hook_token));
        } else if let Some(v) = line
            .strip_prefix("Content-Length:")
            .or_else(|| line.strip_prefix("content-length:"))
        {
            content_length = v.trim().parse().unwrap_or(0);
        }
    }
    if content_length > 0 {
        let mut discard = vec![0u8; content_length];
        let _ = reader.read_exact(&mut discard).await;
    }

    if authorized {
        if let Some((pane, status)) = parse_hook_path(&path) {
            daemon.set_pane_hook_status(pane, status);
        }
        wr.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await?;
    } else {
        wr.write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await?;
    }
    Ok(())
}

/// `/hook/<pane_id>/<event>` -> which pane, and the status that event
/// implies. Unrecognized events (anything we didn't ask `hooks.rs` to
/// install) are ignored rather than erroring.
fn parse_hook_path(path: &str) -> Option<(PaneId, PaneStatus)> {
    let mut parts = path.trim_start_matches('/').split('/');
    if parts.next()? != "hook" {
        return None;
    }
    let pane = PaneId(parts.next()?.parse().ok()?);
    let status = match parts.next()? {
        "UserPromptSubmit" => PaneStatus::Working,
        "Stop" => PaneStatus::Idle,
        "Notification" => PaneStatus::Waiting,
        _ => return None,
    };
    Some((pane, status))
}

/// The status a pane should move to given a hook report, or `None` to leave
/// it alone. A hook firing after the process died (e.g. `Stop` racing a
/// crash) is stale and mustn't resurrect a dead pane's row; a report that
/// changes nothing isn't worth a tree broadcast either.
fn hook_status_update(current: PaneStatus, incoming: PaneStatus) -> Option<PaneStatus> {
    if matches!(current, PaneStatus::Exited { .. }) || current == incoming {
        return None;
    }
    Some(incoming)
}

/// Not cryptographically strong — see `Daemon::hook_token`'s doc comment —
/// just enough entropy that it isn't a fixed, guessable string.
fn gen_token() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::{SystemTime, UNIX_EPOCH};

    let mut hasher = DefaultHasher::new();
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos().hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    let a = hasher.finish();
    std::thread::current().id().hash(&mut hasher);
    let b = hasher.finish();
    format!("{a:016x}{b:016x}")
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
            projects: vec![ProjectConfig {
                name: "proj".to_string(),
                repos: vec![primary.to_string()],
            }],
            agents: Vec::new(),
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

    // --- hook path parsing -------------------------------------------------

    #[test]
    fn hook_path_maps_each_managed_event_to_a_status() {
        assert_eq!(
            parse_hook_path("/hook/7/UserPromptSubmit"),
            Some((PaneId(7), PaneStatus::Working))
        );
        assert_eq!(parse_hook_path("/hook/7/Stop"), Some((PaneId(7), PaneStatus::Idle)));
        assert_eq!(
            parse_hook_path("/hook/7/Notification"),
            Some((PaneId(7), PaneStatus::Waiting))
        );
    }

    #[test]
    fn hook_path_rejects_junk() {
        assert_eq!(parse_hook_path("/hook/7/SessionStart"), None, "unmanaged event");
        assert_eq!(parse_hook_path("/nope/7/Stop"), None, "wrong prefix");
        assert_eq!(parse_hook_path("/hook/abc/Stop"), None, "non-numeric pane id");
        assert_eq!(parse_hook_path("/hook/7"), None, "missing event");
        assert_eq!(parse_hook_path(""), None, "empty path");
    }

    // --- hook status application -------------------------------------------

    #[test]
    fn hook_status_applies_to_a_live_pane() {
        assert_eq!(
            hook_status_update(PaneStatus::Idle, PaneStatus::Working),
            Some(PaneStatus::Working)
        );
        assert_eq!(
            hook_status_update(PaneStatus::Working, PaneStatus::Waiting),
            Some(PaneStatus::Waiting)
        );
    }

    #[test]
    fn hook_status_never_resurrects_an_exited_pane() {
        for code in [Some(0), Some(1), None] {
            assert_eq!(
                hook_status_update(PaneStatus::Exited { code }, PaneStatus::Idle),
                None,
                "exit code {code:?} must stay exited"
            );
        }
    }

    #[test]
    fn hook_status_that_changes_nothing_is_not_an_update() {
        assert_eq!(hook_status_update(PaneStatus::Idle, PaneStatus::Idle), None);
    }

    // --- worktree reconciliation -------------------------------------------

    #[test]
    fn reconcile_adds_a_worktree_created_outside_orion() {
        let d = daemon_with_primary("/repo");
        d.reconcile_worktrees_with(|_| listing(&["/repo", "/repo/.orion/worktrees/feat"]));

        let paths = checkout_paths(&d);
        assert_eq!(paths.len(), 2, "discovered worktree should join the tree: {paths:?}");
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
        assert!(!linked.primary, "a discovered worktree is removable, not primary");
    }

    #[test]
    fn reconcile_is_idempotent() {
        let d = daemon_with_primary("/repo");
        for _ in 0..3 {
            d.reconcile_worktrees_with(|_| listing(&["/repo", "/repo/wt"]));
        }
        assert_eq!(checkout_paths(&d).len(), 2, "repeated ticks must not duplicate rows");
    }

    #[test]
    fn reconcile_drops_a_worktree_removed_outside_orion() {
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
        assert_eq!(worktree_display_name(std::path::Path::new("/repo/wt/feat-x"), false), "feat-x");
        assert_eq!(worktree_display_name(std::path::Path::new("/repo"), true), "repo");
    }

    // --- tree broadcast ----------------------------------------------------

    #[test]
    fn reconcile_result_is_visible_to_tree_subscribers() {
        let d = daemon_with_primary("/repo");
        let mut rx = d.subscribe_tree();
        d.reconcile_worktrees_with(|_| listing(&["/repo", "/repo/wt"]));
        d.broadcast_tree();

        let tree = rx.try_recv().expect("a tree snapshot should have been broadcast");
        assert_eq!(tree[0].checkouts.len(), 2);
    }

    #[test]
    fn default_agent_templates_are_offered_when_config_has_none() {
        let d = daemon_with_primary("/repo");
        assert_eq!(d.template_names(), vec!["claude", "codex", "opencode"]);
    }

    #[test]
    fn gen_token_is_not_a_fixed_string() {
        assert_eq!(gen_token().len(), 32);
        assert_ne!(gen_token(), gen_token());
    }
}
