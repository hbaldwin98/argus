//! Keeping the tree in step with what is actually on disk.
//!
//! Nothing here is asked for by a client. Repositories get cloned into a
//! project root, a branch gets switched in some other terminal, a worktree
//! gets removed by hand, `projects.toml` gets saved — and the rows have to
//! follow without anyone pressing a key.
//!
//! Two beats and two watchers do it: a two-second git status sweep, a
//! ten-second root rescan, a `notify` watch on each repository's metadata,
//! and a watch on the config file. All of the git work runs on the blocking
//! pool and never under the daemon lock, because a keystroke needs that same
//! lock to find the pty it belongs to.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use argus_protocol::{CheckoutId, GitStatus, RepositoryId};

use super::*;

impl Daemon {
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
                    daemon.refresh_branches();
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
        self.refresh_git_status_with(crate::git::status);
    }

    /// The sweep itself, with the status read injected so tests can state
    /// what git would have said — including that it could not say anything.
    /// Production always passes `git::status`.
    pub(super) fn refresh_git_status_with(
        &self,
        read: impl Fn(&std::path::Path) -> Option<GitStatus>,
    ) {
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
            .map(|(id, path)| (id, read(&path)))
            .collect();

        let mut inner = self.inner.lock().unwrap();
        for (id, status) in statuses {
            let Some(c) = find_checkout(&mut inner.projects, id) else {
                continue;
            };
            let Some(status) = status else {
                // The read failed, which is not the same as the checkout
                // having nothing to report. Dropping the cache here is what
                // made a `git switch` in another terminal throw the row back
                // to the directory it was created as and free the branch it
                // was really on. Keep what we last knew until a read works.
                continue;
            };
            c.git = Some(status);
        }
    }

    /// Re-reads `projects.toml` and folds it into the running tree.
    ///
    /// Nothing is rebuilt: projects, repositories and checkouts are matched
    /// to what the file now says and updated in place, so ids stay valid
    /// and every pane keeps running. What the file no longer mentions is
    /// removed — unless it is holding panes, which are somebody's work in
    /// progress and not the config file's to end. Those rows stay until
    /// they are empty, and the next reload takes them.
    ///
    /// Agent templates are replaced wholesale, since a template is looked
    /// up by name each time an agent starts. Harnesses are not: a running
    /// agent's hooks on disk were written by the harness it started under.
    pub fn reload_config(self: &Arc<Self>) -> anyhow::Result<()> {
        let config = config::load()?;
        let templates = if config.agents.is_empty() {
            config::default_agents()
        } else {
            config.agents
        };
        *self.templates.lock().unwrap() = templates;

        {
            let mut inner = self.inner.lock().unwrap();
            let excluded = inner.excluded.clone();
            let Inner {
                workspaces,
                projects,
                ids,
                ..
            } = &mut *inner;

            for declared in &config.workspaces {
                intern_workspace(workspaces, ids, &declared.name);
            }

            for p in &config.projects {
                let workspace = match p.workspace.as_deref() {
                    Some(name) => intern_workspace(workspaces, ids, name),
                    None => intern_workspace(workspaces, ids, config::DEFAULT_WORKSPACE),
                };
                let root = p.root.as_deref().map(config::expand_home);
                let named: Vec<PathBuf> = p
                    .repos
                    .iter()
                    .map(|repo| config::expand_home(repo))
                    .filter(|path| !is_excluded(&excluded, path))
                    .collect();

                match projects.iter_mut().find(|live| live.name == p.name) {
                    Some(live) => {
                        live.workspace = workspace;
                        live.root = root;
                        live.worktree_root =
                            p.worktree_root.as_deref().map(config::expand_home);
                        live.setup = p.setup.clone();
                        live.exclusive = p.exclusive;
                        live.scan = crate::git::Scan {
                            exclude: p.exclude.clone(),
                            include: p.include.clone(),
                        };
                        for path in &named {
                            if !live.repositories.iter().any(|r| {
                                r.checkouts.iter().any(|c| c.primary && &c.path == path)
                            }) {
                                live.repositories
                                    .push(new_repository(ids, path.clone(), false));
                            }
                        }
                        // A repository the config named and no longer does.
                        // Discovered ones answer to the root scan instead.
                        live.repositories.retain(|r| {
                            r.discovered
                                || named.iter().any(|path| {
                                    r.checkouts.iter().any(|c| c.primary && &c.path == path)
                                })
                                || r.checkouts.iter().any(|c| !c.panes.is_empty())
                        });
                    }
                    None => {
                        let repositories = named
                            .into_iter()
                            .map(|path| new_repository(ids, path, false))
                            .collect();
                        projects.push(Project {
                            id: ProjectId(ids.alloc()),
                            workspace,
                            name: p.name.clone(),
                            root,
                            repositories,
                            worktree_root: p
                                .worktree_root
                                .as_deref()
                                .map(config::expand_home),
                            setup: p.setup.clone(),
                            exclusive: p.exclusive,
                            scan: crate::git::Scan {
                                exclude: p.exclude.clone(),
                                include: p.include.clone(),
                            },
                        });
                    }
                }
            }

            projects.retain(|live| {
                config.projects.iter().any(|p| p.name == live.name)
                    || live
                        .repositories
                        .iter()
                        .flat_map(|r| r.checkouts.iter())
                        .any(|c| !c.panes.is_empty())
            });

            // Workspaces are only ever interned, never dropped: the open
            // one stays valid, and a workspace whose projects all left is
            // an empty tab rather than a dangling id.
        }

        // A repository named for the first time has no status yet, and its
        // row is named from that status.
        self.reconcile_repositories();
        self.refresh_git_status();
        self.refresh_branches();
        self.broadcast_tree();
        self.broadcast_workspaces();
        Ok(())
    }

    /// Reloads whenever `projects.toml` is written, so editing the file is
    /// all there is to editing the config.
    ///
    /// An editor's save is several writes and often a rename, hence the
    /// pause before reading; a file caught half-written simply fails to
    /// parse, and a config that does not parse is logged and ignored rather
    /// than allowed to take the running tree with it.
    pub fn start_config_watch(self: &Arc<Self>) {
        let path = config::config_path();
        let Some(dir) = path.parent().map(std::path::Path::to_path_buf) else {
            return;
        };
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();
        let Some(watch) = crate::watch::directory(&dir, move || {
            let _ = tx.send(());
        }) else {
            return;
        };

        let daemon = self.clone();
        tokio::spawn(async move {
            // Held for as long as the loop runs: dropping the watcher stops
            // the watch.
            let _watch = watch;
            while rx.recv().await.is_some() {
                tokio::time::sleep(Duration::from_millis(250)).await;
                while rx.try_recv().is_ok() {}

                let daemon = daemon.clone();
                let _ = tokio::task::spawn_blocking(move || match daemon.reload_config() {
                    Ok(()) => tracing::info!("reloaded {}", config::config_path().display()),
                    Err(e) => tracing::warn!("keeping the running config: {e}"),
                })
                .await;
            }
        });
    }

    /// Watches every repository's Git metadata and refreshes the moment it
    /// changes, so a branch switch, a commit, or a worktree made in a shell
    /// lands in the tree when it happens instead of up to a tick later.
    ///
    /// The poll stays: editing a file touches nothing under `.git`, so
    /// dirty state and changed-file counts still need the sweep. This is
    /// the half that can be known exactly, done exactly.
    pub fn start_git_watch(self: &Arc<Self>) {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();
        let Some(mut watch) = crate::watch::GitWatch::new(move || {
            let _ = tx.send(());
        }) else {
            return;
        };

        let daemon = self.clone();
        tokio::spawn(async move {
            let mut resync = tokio::time::interval(Duration::from_secs(10));
            loop {
                tokio::select! {
                    // The set of repositories changes as roots are scanned
                    // and projects come and go; re-derive it on the same
                    // slow beat the scan itself runs on.
                    _ = resync.tick() => watch.sync(&daemon.git_dirs()),
                    event = rx.recv() => {
                        if event.is_none() {
                            break;
                        }
                        // One user action is many writes — a commit alone
                        // moves HEAD, a ref, and the index — so let the
                        // burst finish before reading any of it.
                        tokio::time::sleep(Duration::from_millis(150)).await;
                        while rx.try_recv().is_ok() {}

                        let daemon = daemon.clone();
                        let _ = tokio::task::spawn_blocking(move || {
                            daemon.reconcile_worktrees();
                            daemon.refresh_git_status();
                            daemon.refresh_branches();
                            let _ = daemon.tree_tx.send(daemon.snapshot());
                        })
                        .await;
                    }
                }
            }
        });
    }

    /// Every repository's Git directory, for the watch to follow. Uses the
    /// primary checkout's, which is where linked worktrees keep theirs too.
    fn git_dirs(&self) -> Vec<PathBuf> {
        let primaries: Vec<PathBuf> = {
            let inner = self.inner.lock().unwrap();
            inner
                .projects
                .iter()
                .flat_map(|p| p.repositories.iter())
                .filter_map(|r| r.checkouts.iter().find(|c| c.primary))
                .map(|c| c.path.clone())
                .collect()
        };
        primaries
            .iter()
            .filter_map(|path| crate::git::git_dir(path))
            .collect()
    }

    /// Re-reads each repository's local branches and caches the ones no
    /// checkout is sitting on, along with which branch is the main line.
    ///
    /// Runs after `refresh_git_status` and reads the branch names it just
    /// cached: what makes a branch "without a checkout" is that no checkout
    /// of that repository currently has it, which is exactly what a
    /// checkout's status says. Three phases with the lock dropped in the
    /// middle, for the reason the status sweep documents.
    pub fn refresh_branches(&self) {
        let repositories: Vec<(RepositoryId, PathBuf, Vec<String>)> = {
            let inner = self.inner.lock().unwrap();
            inner
                .projects
                .iter()
                .flat_map(|p| p.repositories.iter())
                .filter_map(|r| {
                    let primary = r.checkouts.iter().find(|c| c.primary)?;
                    let occupied = r
                        .checkouts
                        .iter()
                        .filter_map(|c| c.git.as_ref().and_then(|g| g.branch.clone()))
                        .collect();
                    Some((r.id, primary.path.clone(), occupied))
                })
                .collect()
        };

        let listed: Vec<BranchState> = repositories
            .into_iter()
            .map(|(id, path, occupied)| BranchState {
                id,
                free: crate::browse::branches(&path)
                    .into_iter()
                    .filter(|b| !occupied.contains(b))
                    .collect(),
                default: crate::git::default_branch(&path),
                remote: crate::git::remote_branches(&path),
            })
            .collect();

        let mut inner = self.inner.lock().unwrap();
        for state in listed {
            if let Some(r) = find_repository(&mut inner.projects, state.id) {
                r.branches = state.free;
                r.default_branch = state.default;
                r.remote_branches = state.remote;
            }
        }
    }

    /// Re-reads one checkout's status, for the moments where waiting for the
    /// next poll would show the user the state they just changed away from.
    /// A checkout row is named after the branch in its cached status, so a
    /// switch that only updated the fallback name would keep drawing the
    /// branch it just left for the rest of the tick.
    pub(super) fn refresh_checkout_git(&self, checkout: CheckoutId) {
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
    pub(super) fn reconcile_worktrees(&self) {
        self.reconcile_worktrees_with(crate::git::list_worktrees);
    }

    /// The reconciliation itself, with the worktree listing injected so
    /// tests can drive it without a real repo (and without waiting on the
    /// `git` binary). Production always passes `git::list_worktrees`.
    pub(super) fn reconcile_worktrees_with(&self, list: impl Fn(&std::path::Path) -> Vec<PathBuf>) {
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
    pub(super) fn reconcile_repositories(&self) -> bool {
        self.reconcile_repositories_with(crate::git::discover_repositories_within)
    }

    /// The reconciliation itself, with the scan injected so tests can state
    /// what is on disk instead of building it. Reports whether anything
    /// changed. Production always passes `git::discover_repositories`.
    pub(super) fn reconcile_repositories_with(
        &self,
        discover: impl Fn(&std::path::Path, &crate::git::Scan) -> Vec<PathBuf>,
    ) -> bool {
        // Scanning happens between the two locks and never inside one, for
        // the same reason `add_project` scans before taking it.
        let roots: Vec<(ProjectId, PathBuf, crate::git::Scan)> = {
            let inner = self.inner.lock().unwrap();
            inner
                .projects
                .iter()
                .filter_map(|p| p.root.clone().map(|root| (p.id, root, p.scan.clone())))
                .collect()
        };
        if roots.is_empty() {
            return false;
        }
        let scanned: Vec<(ProjectId, Vec<PathBuf>)> = roots
            .into_iter()
            .map(|(id, root, scan)| (id, discover(&root, &scan)))
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
                // A project added at runtime is written to the config as a
                // root and nothing else; worktree settings are something
                // the user adds to the file by hand.
                worktree_root: None,
                setup: Vec::new(),
                exclusive: false,
                scan: crate::git::Scan::default(),
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
}
