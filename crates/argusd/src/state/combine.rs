//! Folding several projects of the panel into one.
//!
//! The repositories themselves move, not copies of them: ids, checkouts
//! and panes go with each, so an agent at work in one carries on and a
//! client's ids stay good. Boards need nothing done to them, since a board
//! is filed under its repository's Git directory or its workspace's name,
//! never under a project's.

use super::*;

impl Daemon {
    /// Moves every repository of the `from` projects into `into` and takes
    /// the emptied projects out of the panel. `into` keeps its name,
    /// workspace and settings; those of `from` are dropped with them.
    ///
    /// Running panes do not refuse it, as they refuse `remove_project`:
    /// nothing they run in leaves the panel, it only changes rows.
    ///
    /// Checked, recorded and moved under one hold of the lock, so `into`
    /// cannot leave between the check and the move and leave the store
    /// naming repositories under a project the tree no longer has.
    pub fn combine_projects(&self, into: ProjectId, from: &[ProjectId]) -> anyhow::Result<ProjectId> {
        {
            let mut inner = self.inner.lock().unwrap();
            let (into_name, moved, taken_out) = plan(&inner.projects, into, from)?;
            self.store.combine_projects(&into_name, &moved, &taken_out)?;

            let (leaving, staying): (Vec<Project>, Vec<Project>) =
                std::mem::take(&mut inner.projects)
                    .into_iter()
                    .partition(|p| from.contains(&p.id));
            inner.projects = staying;
            let target = inner
                .projects
                .iter_mut()
                .find(|p| p.id == into)
                .expect("plan found it under this same lock");
            for repository in leaving.into_iter().flat_map(|p| p.repositories) {
                fold(&mut target.repositories, repository);
            }
        }
        self.ensure_local_ignores();
        self.broadcast_tree();
        // Fewer projects in each workspace's rollup, and maybe another
        // workspace's repositories in this one's.
        self.broadcast_workspaces();
        Ok(into)
    }
}

/// What combining `from` into `into` writes: `into`'s name, the primary
/// paths it gains, and each `from` project as the store takes it out.
type Plan = (String, Vec<PathBuf>, Vec<(String, Option<PathBuf>)>);

fn plan(projects: &[Project], into: ProjectId, from: &[ProjectId]) -> anyhow::Result<Plan> {
    let find = |id: ProjectId| {
        projects
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| anyhow::anyhow!("no such project"))
    };
    let target = find(into)?;
    if from.is_empty() {
        anyhow::bail!("choose the projects to combine into {}", target.name);
    }
    let mut moved: Vec<PathBuf> = Vec::new();
    let mut taken_out = Vec::new();
    for (place, &id) in from.iter().enumerate() {
        let p = find(id)?;
        if id == into {
            anyhow::bail!("{} cannot be combined into itself", p.name);
        }
        if from[..place].contains(&id) {
            anyhow::bail!("{} is named twice", p.name);
        }
        let primaries = p
            .repositories
            .iter()
            .flat_map(|r| r.checkouts.iter())
            .filter(|c| c.primary)
            .map(|c| &c.path);
        // A path two of `from` both list is named once: the store ignores
        // a second row of the same project and path.
        moved.extend(primaries.filter(|path| !holds(&target.repositories, path)).cloned());
        taken_out.push((p.name.clone(), p.root.clone()));
    }
    Ok((target.name.clone(), moved, taken_out))
}

fn holds(repositories: &[Repository], path: &std::path::Path) -> bool {
    repositories
        .iter()
        .flat_map(|r| r.checkouts.iter())
        .any(|c| same_path(&c.path, path))
}

/// Adds `repository` to `repositories`, or, where they already hold its
/// directory, hands its panes to the row that does: a directory listed
/// twice is one row, and the panes in the leaving copy are still running.
///
/// A moved repository is no longer `discovered`: the store names it under
/// its new project, and `into`'s root scan, which may never reach it, must
/// not take it away for being missing.
fn fold(repositories: &mut Vec<Repository>, mut repository: Repository) {
    let primary = repository.checkouts.iter().find(|c| c.primary).map(|c| c.path.clone());
    let held = primary.and_then(|path| {
        repositories
            .iter_mut()
            .find(|r| r.checkouts.iter().any(|c| same_path(&c.path, &path)))
    });
    let Some(held) = held else {
        repository.discovered = false;
        repositories.push(repository);
        return;
    };
    for checkout in repository.checkouts {
        match held.checkouts.iter_mut().find(|c| same_path(&c.path, &checkout.path)) {
            Some(same) => same.panes.extend(checkout.panes),
            None => held.checkouts.push(checkout),
        }
    }
}
