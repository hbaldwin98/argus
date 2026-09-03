//! Adding and removing the rows of the panel by hand.
//!
//! Each of these both edits the live tree and records the edit in the
//! store, because the panel is the user's arrangement of what the config
//! declares: a project added at runtime has to survive a restart, and a
//! repository removed has to stay removed against a scan that keeps
//! finding it.

use super::*;

impl Daemon {
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
        self.store.add_project(&crate::store::ProjectOverlay {
            name: name.clone(),
            root: expanded.clone(),
            workspace: workspace_name,
        })?;

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
                agent_todos: false,
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

        let name = {
            let inner = self.inner.lock().unwrap();
            let p = inner
                .projects
                .iter()
                .find(|p| p.id == project)
                .ok_or_else(|| anyhow::anyhow!("no such project"))?;
            if p.repositories
                .iter()
                .flat_map(|r| r.checkouts.iter())
                .any(|c| same_path(&c.path, &expanded))
            {
                anyhow::bail!("{} already has {}", p.name, expanded.display());
            }
            p.name.clone()
        };

        // Recorded first, for the same reason removal is: a row that
        // appears in the panel but nowhere else is gone again after a
        // restart.
        self.store.add_repo(&name, &expanded)?;

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
            self.store.set_excluded_repos(&remaining)?;
        }

        self.broadcast_tree();
        Ok(())
    }

    /// Takes a project out of the panel. Nothing on disk is touched — this
    /// is the undo for `add_project`, not a delete, and adding the same
    /// directory again brings the same tree back. A project the config
    /// declares is recorded as hidden rather than removed, because that
    /// file is the user's to edit.
    ///
    /// Refused while any of its panes is alive. Removing the row would
    /// leave those processes running with nowhere to reach them, and unlike
    /// `remove_checkout` — where killing the panes is the point, because
    /// the worktree they sit in is going away — here the checkout survives
    /// and the user can simply look at it again.
    pub fn remove_project(&self, project: ProjectId) -> anyhow::Result<()> {
        let (name, root, excluded_paths) = {
            let inner = self.inner.lock().unwrap();
            let p = inner
                .projects
                .iter()
                .find(|p| p.id == project)
                .ok_or_else(|| anyhow::anyhow!("no such project"))?;
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
            (p.name.clone(), p.root.clone(), paths)
        };

        // Recorded first: a project that vanishes from the panel but not
        // from the store comes back on the next restart, which reads as
        // Argus having ignored the request.
        self.store.remove_project(&name, root.as_deref())?;

        let remaining = {
            let mut inner = self.inner.lock().unwrap();
            inner.projects.retain(|p| p.id != project);
            inner
                .excluded
                .retain(|e| !excluded_paths.iter().any(|p| same_path(e, p)));
            inner.excluded.clone()
        };
        self.store.set_excluded_repos(&remaining)?;
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

        self.store.exclude_repo(&path)?;
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
