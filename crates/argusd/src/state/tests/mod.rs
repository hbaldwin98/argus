//! The daemon's tests, split the way the daemon is: one file per concern,
//! over the fixtures they all build a daemon from.

mod branches;
mod hook_lifecycle;
mod note;
mod pane_api;
mod reconcile;
mod reload;
mod restore;

use super::*;
use crate::config::ProjectConfig;
use argus_protocol::{
    parse_pane_path, Endpoint, NoteCounts, NoteTarget, ReviewAnchor, ReviewBase, TodoState,
    MAX_NOTE_BYTES,
};
use super::agents::clean_title;
use super::session::RESUME_GRACE;

/// A daemon with one project whose primary checkout is `primary`, and no
/// panes. Nothing here touches disk: `Daemon::new` only expands paths,
/// and every test below injects its own worktree listing.
pub(super) fn daemon_with_primary(primary: &str) -> Arc<Daemon> {
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

pub(super) fn daemon_with_repositories(repositories: &[&str]) -> Arc<Daemon> {
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

pub(super) fn checkout_paths(d: &Daemon) -> Vec<String> {
    d.snapshot()
        .into_iter()
        .flat_map(|p| p.repositories)
        .flat_map(|r| r.checkouts)
        .map(|c| c.path)
        .collect()
}

pub(super) fn listing(paths: &[&str]) -> Vec<PathBuf> {
    paths.iter().map(PathBuf::from).collect()
}

pub(super) fn pane_info(d: &Daemon, pane: PaneId) -> PaneInfo {
    d.snapshot()
        .into_iter()
        .flat_map(|p| p.repositories)
        .flat_map(|r| r.checkouts)
        .flat_map(|c| c.panes)
        .find(|p| p.id == pane)
        .expect("pane should still be in the tree")
}

pub(super) fn pane_size(d: &Daemon, pane: PaneId) -> (u16, u16) {
    let (rows, cols, _, _, _, _, _) = d.subscribe_pane(pane).unwrap();
    (rows, cols)
}

/// A daemon whose one agent template has a restart policy.
pub(super) fn daemon_with_a_restarting_agent(
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

pub(super) fn panes_of(d: &Daemon) -> Vec<PaneInfo> {
    d.snapshot()
        .remove(0)
        .repositories
        .remove(0)
        .checkouts
        .remove(0)
        .panes
}

pub(super) fn daemon_with_two_agent_checkouts(
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

pub(super) fn status_on(branch: &str) -> GitStatus {
    GitStatus {
        branch: Some(branch.to_string()),
        dirty: false,
        changed_files: 0,
        ahead: 0,
        behind: 0,
    }
}

// --- reconciliation against a real repo ---------------------------------

/// Builds a real repo with one commit, so `git::list_worktrees` has
/// something truthful to return. Mirrors `git::tests::repo_with_a_commit`.
pub(super) fn real_repo(dir: &std::path::Path) -> git2::Repository {
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

// --- project roots ------------------------------------------------------

/// A project that finds its repositories under `root`, with `repos`
/// naming any that are also written down outright.
pub(super) fn daemon_rooted_at(root: &std::path::Path, repos: &[&str]) -> Arc<Daemon> {
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
pub(super) fn declares_a_project(cfg: &std::path::Path) -> bool {
    declares(cfg, "[[project]]")
}

pub(super) fn declares_a_workspace(cfg: &std::path::Path) -> bool {
    declares(cfg, "[[workspace]]")
}

pub(super) fn declares(cfg: &std::path::Path, header: &str) -> bool {
    std::fs::read_to_string(cfg.join("projects.toml"))
        .unwrap_or_default()
        .lines()
        .any(|line| line.trim_start().starts_with(header))
}

pub(super) fn repository_names(d: &Daemon) -> Vec<String> {
    d.snapshot()
        .into_iter()
        .flat_map(|p| p.repositories)
        .map(|r| r.name)
        .collect()
}

// --- adding one repository to a project ---------------------------------

/// A project rooted at `dir`, added the way the TUI adds it, plus a
/// repository sitting somewhere the root will never scan.
pub(super) fn project_and_an_outside_repository() -> (tempfile::TempDir, tempfile::TempDir, Arc<Daemon>) {
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

// --- removing what was added --------------------------------------------

/// A project rooted at a temp directory holding one repository per
/// name, added through `add_project` so it is written to the config the
/// way the TUI writes it.
/// A daemon backed by the store in the temp config directory. What
/// every test about surviving a restart needs: `Daemon::new` hands out
/// a store that dies with the process, which is exactly what makes it
/// safe everywhere else.
pub(super) fn persistent(config: ConfigFile) -> Arc<Daemon> {
    Daemon::with_store(config, crate::store::Store::open().unwrap())
}

pub(super) fn added_project_with(names: &[&str]) -> (tempfile::TempDir, Arc<Daemon>) {
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

// --- workspaces ---------------------------------------------------------

/// `ARGUS_CONFIG_DIR` is process-global, so tests that read or write
/// config take this lock and restore the variable afterwards.
static CONFIG_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Runs `f` with the config directory pointed at a fresh temp dir, so
/// nothing here can see — or corrupt — the real user's config.
pub(super) fn with_temp_config<T>(f: impl FnOnce(&std::path::Path) -> T) -> T {
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

// --- managed hook lifecycle ---------------------------------------------

/// A daemon whose single checkout is `dir`, with a "claude" template
/// that is really just `echo` — enough to exercise the hook-install path
/// (which keys off the template *name*) without launching a real agent.
pub(super) fn daemon_with_fake_claude(dir: &std::path::Path) -> Arc<Daemon> {
    Daemon::new(fake_claude_config(dir))
}

pub(super) fn fake_claude_config(dir: &std::path::Path) -> ConfigFile {
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
pub(super) fn daemon_with_an_exclusive_project(dir: &std::path::Path) -> Arc<Daemon> {
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

pub(super) fn review_anchor(line: u32) -> ReviewAnchor {
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

pub(super) async fn post_agent_hook(
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

pub(super) fn settings_of(dir: &std::path::Path) -> std::path::PathBuf {
    dir.join(".claude").join("settings.local.json")
}

pub(super) fn only_checkout(d: &Daemon) -> CheckoutId {
    d.snapshot()[0].repositories[0].checkouts[0].id
}

pub(super) fn config_with_workspaces() -> ConfigFile {
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

pub(super) fn names_of(d: &Daemon) -> Vec<String> {
    d.snapshot().into_iter().map(|p| p.name).collect()
}

pub(super) fn workspace_named(d: &Daemon, name: &str) -> WorkspaceId {
    d.workspaces()
        .into_iter()
        .find(|w| w.name == name)
        .unwrap_or_else(|| panic!("no workspace {name:?}"))
        .id
}

// --- branches -----------------------------------------------------------

/// A real repo with one commit, and a daemon whose only checkout is it.
pub(super) fn daemon_on_a_repo() -> (tempfile::TempDir, Arc<Daemon>) {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    let d = daemon_with_primary(&dir.path().to_string_lossy());
    (dir, d)
}

/// The same repo, in a project that says where worktrees go and what to
/// run in one.
pub(super) fn daemon_on_a_repo_with(
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

pub(super) fn init_repo(dir: &std::path::Path) {
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

pub(super) fn head_of(path: &std::path::Path) -> String {
    git2::Repository::open(path)
        .unwrap()
        .head()
        .unwrap()
        .shorthand()
        .unwrap()
        .to_string()
}

/// A branch on the repo's current commit, holding nothing of its own.
pub(super) fn branch_off_head(dir: &std::path::Path, name: &str) {
    let repo = git2::Repository::open(dir).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch(name, &head, false).unwrap();
}

/// A second repository standing in for a remote, and the repo under
/// test wired to it. Git takes a path as a URL, forward slashes and all.
pub(super) fn remote_holding(branch: &str) -> (tempfile::TempDir, String) {
    let upstream = tempfile::tempdir().unwrap();
    init_repo(upstream.path());
    branch_off_head(upstream.path(), branch);
    let url = upstream.path().to_string_lossy().replace('\\', "/");
    (upstream, url)
}

/// A branch one commit ahead of HEAD, which is what makes `-d` refuse.
pub(super) fn commit_on_a_branch(dir: &std::path::Path, branch: &str) {
    let repo = git2::Repository::open(dir).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    let tree = head.tree().unwrap();
    let sig = git2::Signature::now("t", "t@example.com").unwrap();
    repo.commit(
        Some(&format!("refs/heads/{branch}")),
        &sig,
        &sig,
        "work",
        &tree,
        &[&head],
    )
    .unwrap();
}

// --- session restore ----------------------------------------------------

/// A daemon whose only project is `dir`, with one agent template that
/// runs the platform shell so restoring one actually starts something.
///
/// Backed by the store in the temp config directory rather than an
/// in-memory one, so what [`record`] writes is what it reads — these
/// tests are about surviving a restart, and a store that does not
/// outlive the daemon cannot show that.
pub(super) fn daemon_for_restore(dir: &std::path::Path) -> Arc<Daemon> {
    Daemon::with_store(restore_config(dir), crate::store::Store::open().unwrap())
}

pub(super) fn restore_config(dir: &std::path::Path) -> ConfigFile {
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
pub(super) fn record(panes: &[(PaneKind, &str)], checkout: &std::path::Path) {
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
pub(super) fn record_panes(panes: Vec<crate::store::SessionPane>) {
    crate::store::Store::open()
        .unwrap()
        .save_panes(&panes)
        .unwrap();
}

pub(super) fn persistent_agent_command() -> Vec<String> {
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

pub(super) fn daemon_with_claude_aliases(dir: &std::path::Path, names: &[&str]) -> Arc<Daemon> {
    daemon_running(dir, names, persistent_agent_command())
}

/// Like [`daemon_with_claude_aliases`], but the store is the temp config
/// directory's — so [`record_agents`] is what a restart reads. Caller
/// holds [`with_temp_config`].
pub(super) fn daemon_with_claude_aliases_for_restore(
    dir: &std::path::Path,
    names: &[&str],
) -> Arc<Daemon> {
    Daemon::with_store(
        running_config(dir, names, persistent_agent_command()),
        crate::store::Store::open().unwrap(),
    )
}

pub(super) fn daemon_running(dir: &std::path::Path, names: &[&str], cmd: Vec<String>) -> Arc<Daemon> {
    // In-memory: these daemons live for whole seconds while a pane
    // starts, and [`Store::open`] would hold the process-global
    // `runtime.db` the tests running beside this one also open.
    Daemon::with_store(
        running_config(dir, names, cmd),
        crate::store::Store::in_memory()
            .expect("an in-memory runtime store needs nothing that can fail"),
    )
}

pub(super) fn running_config(dir: &std::path::Path, names: &[&str], cmd: Vec<String>) -> ConfigFile {
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

/// A daemon whose one project has opted into agent note writes. Every
/// other test daemon has not, which is the default the write path is meant
/// to refuse.
pub(super) fn daemon_allowing_agent_todos(dir: &std::path::Path) -> Arc<Daemon> {
    let mut config = running_config(dir, &["claude"], persistent_agent_command());
    config.projects[0].agent_todos = true;
    Daemon::with_store(
        config,
        crate::store::Store::in_memory().expect("an in-memory store needs nothing that can fail"),
    )
}

pub(super) fn record_agents(checkout: &std::path::Path, agents: &[(&str, Option<&str>)]) {
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

pub(super) fn close_all(d: &Daemon) {
    for p in &d.snapshot()[0].repositories[0].checkouts[0].panes {
        let _ = d.close_pane(p.id);
    }
}

pub(super) fn saved_panes() -> Vec<crate::store::SessionPane> {
    crate::store::Store::open().unwrap().panes().unwrap()
}

// --- resuming a conversation --------------------------------------------

/// How many panes are currently holding a reopened conversation. Not
/// in the snapshot: it is bookkeeping for the fallback, not something
/// a row shows.
pub(super) fn resuming_panes(d: &Daemon) -> usize {
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
