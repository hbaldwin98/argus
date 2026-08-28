//! Finding your way around the tree, and the small edits that reshape it.
//!
//! Free functions rather than methods because they take `&mut [Project]`,
//! not `&Daemon`: every caller already holds the lock, and a method would
//! invite taking it a second time. The tree is a plain nested `Vec`, so a
//! lookup is a walk — with a handful of rows per column that is cheaper
//! than the bookkeeping an index would need to survive reconciliation.

use super::*;

/// The four levels of the tree, flattened. Every lookup here is one of
/// these plus a `find`, so the walk itself is written once.
pub(super) fn checkouts(projects: &[Project]) -> impl Iterator<Item = &Checkout> {
    projects
        .iter()
        .flat_map(|p| p.repositories.iter())
        .flat_map(|r| r.checkouts.iter())
}

pub(super) fn checkouts_mut(projects: &mut [Project]) -> impl Iterator<Item = &mut Checkout> {
    projects
        .iter_mut()
        .flat_map(|p| p.repositories.iter_mut())
        .flat_map(|r| r.checkouts.iter_mut())
}

pub(super) fn panes(projects: &[Project]) -> impl Iterator<Item = &Pane> {
    checkouts(projects).flat_map(|c| c.panes.iter())
}

pub(super) fn panes_mut(projects: &mut [Project]) -> impl Iterator<Item = &mut Pane> {
    checkouts_mut(projects).flat_map(|c| c.panes.iter_mut())
}

/// The id of the workspace by this name, creating it if this is the first
/// time it has been mentioned. Workspaces come from three places — the
/// built-in default, `[[workspace]]` blocks, and any name a project refers
/// to without declaring — so declaring one is optional.
pub(super) fn intern_workspace(
    workspaces: &mut Vec<Workspace>,
    ids: &mut IdGen,
    name: &str,
) -> WorkspaceId {
    if let Some(w) = workspaces.iter().find(|w| w.name == name) {
        return w.id;
    }
    let id = WorkspaceId(ids.alloc());
    workspaces.push(Workspace {
        id,
        name: name.to_string(),
    });
    id
}

pub(super) fn find_checkout(projects: &mut [Project], id: CheckoutId) -> Option<&mut Checkout> {
    checkouts_mut(projects).find(|c| c.id == id)
}

pub(super) fn find_checkout_ref(projects: &[Project], id: CheckoutId) -> Option<&Checkout> {
    checkouts(projects).find(|c| c.id == id)
}

pub(super) fn find_repository(
    projects: &mut [Project],
    id: RepositoryId,
) -> Option<&mut Repository> {
    projects
        .iter_mut()
        .flat_map(|p| p.repositories.iter_mut())
        .find(|r| r.id == id)
}

/// The primary checkout of whichever repository holds `id` — where a
/// worktree command has to run, since a linked worktree is not where git
/// keeps the registration it is about to change. Falls back to the
/// checkout itself for a repository with no primary row.
pub(super) fn primary_path_of(projects: &[Project], id: CheckoutId) -> Option<PathBuf> {
    projects
        .iter()
        .flat_map(|p| p.repositories.iter())
        .find_map(|r| {
            let base = r.checkouts.iter().find(|c| c.id == id)?;
            let primary = r.checkouts.iter().find(|c| c.primary).unwrap_or(base);
            Some(primary.path.clone())
        })
}

/// Everything creating a worktree needs to know about where it goes.
pub(super) struct WorktreeContext {
    pub(super) repository: RepositoryId,
    /// The checkout whose HEAD a new branch is cut from, and where the
    /// `git worktree add` runs.
    pub(super) base: PathBuf,
    /// The directory worktrees for this repository are placed under.
    pub(super) root: PathBuf,
    /// The project's setup commands, run in whatever is created.
    pub(super) setup: Vec<String>,
}

/// Where this repository's worktrees go: the project's configured root with
/// a directory per repository under it, or `.argus/worktrees` beside the
/// primary checkout when the project doesn't say.
///
/// A shared root needs the repository level — two repositories in one
/// project routinely have a `main` or a `feat/x`, and without it the second
/// one to ask would land on the first one's directory.
pub(super) fn worktree_context(projects: &[Project], id: CheckoutId) -> Option<WorktreeContext> {
    projects.iter().find_map(|p| {
        p.repositories.iter().find_map(|r| {
            let base = r.checkouts.iter().find(|c| c.id == id)?;
            let primary = r.checkouts.iter().find(|c| c.primary).unwrap_or(base);
            let root = match &p.worktree_root {
                Some(root) => root.join(&r.name),
                None => primary.path.join(".argus").join("worktrees"),
            };
            Some(WorktreeContext {
                repository: r.id,
                base: base.path.clone(),
                root,
                setup: p.setup.clone(),
            })
        })
    })
}

/// Prefers the checked-out branch name for a newly-discovered worktree —
/// matches how `create_worktree` names ones Argus made itself — falling
/// back to the directory name for a detached HEAD or an unreadable repo.
pub(super) fn worktree_display_name(path: &std::path::Path, is_primary: bool) -> String {
    if !is_primary {
        if let Some(branch) = crate::git::status(path).and_then(|s| s.branch) {
            return branch;
        }
    }
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

pub(super) fn remove_checkout_entry(projects: &mut [Project], id: CheckoutId) -> Option<Checkout> {
    for project in projects.iter_mut() {
        for repository in project.repositories.iter_mut() {
            if let Some(pos) = repository.checkouts.iter().position(|c| c.id == id) {
                return Some(repository.checkouts.remove(pos));
            }
        }
    }
    None
}

/// The pane and the id of the checkout holding it — which the pane itself
/// does not carry, and which a caller acting on the pane's exit needs in
/// order to put something back in its place.
pub(super) fn find_pane_with_checkout(
    projects: &mut [Project],
    id: PaneId,
) -> Option<(&mut Pane, CheckoutId)> {
    checkouts_mut(projects).find_map(|c| {
        let checkout = c.id;
        c.panes
            .iter_mut()
            .find(|p| p.id == id)
            .map(|p| (p, checkout))
    })
}

pub(super) fn find_pane(projects: &mut [Project], id: PaneId) -> Option<&mut Pane> {
    panes_mut(projects).find(|p| p.id == id)
}

pub(super) fn find_pane_ref(projects: &[Project], id: PaneId) -> Option<&Pane> {
    panes(projects).find(|p| p.id == id)
}

/// The agent already working in this checkout, if its project allows only
/// one. `None` when the project has not asked for that, or when nothing is
/// running there — sharing a checkout is otherwise allowed, and merely
/// shown (TARGET.md §Repository and checkout model).
pub(super) fn exclusive_conflict(projects: &[Project], checkout: CheckoutId) -> Option<String> {
    let project = projects.iter().find(|p| {
        p.repositories
            .iter()
            .any(|r| r.checkouts.iter().any(|c| c.id == checkout))
    })?;
    if !project.exclusive {
        return None;
    }
    project
        .repositories
        .iter()
        .flat_map(|r| r.checkouts.iter())
        .find(|c| c.id == checkout)?
        .panes
        .iter()
        .find(|p| p.kind == PaneKind::Agent)
        .map(|p| p.title.clone())
}

/// Whether any agent pane is still open in the checkout at `path`. Gates
/// tearing down that checkout's managed hooks, which are shared by every
/// agent running there.
pub(super) fn checkout_has_agent(projects: &[Project], path: &std::path::Path) -> bool {
    checkouts(projects)
        .filter(|c| c.path == path)
        .any(|c| c.panes.iter().any(|p| p.kind == PaneKind::Agent))
}

/// Removes a pane from whichever checkout holds it, returning it along with
/// that checkout's path — which the caller can't look up afterwards, the
/// pane being gone by then.
pub(super) fn remove_pane_with_checkout(
    projects: &mut [Project],
    id: PaneId,
) -> Option<(Pane, PathBuf)> {
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

/// A repository holding only its primary checkout, which is what both a
/// configured path and a discovered one start as. Linked worktrees arrive
/// afterwards, from `reconcile_worktrees`.
pub(super) fn new_repository(ids: &mut IdGen, path: PathBuf, discovered: bool) -> Repository {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    Repository {
        id: RepositoryId(ids.alloc()),
        name: name.clone(),
        discovered,
        branches: Vec::new(),
        default_branch: None,
        remote_branches: Vec::new(),
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
pub(super) fn install_discovered(
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
pub(super) fn is_excluded(excluded: &[PathBuf], path: &std::path::Path) -> bool {
    excluded.iter().any(|e| same_path(e, path))
}

pub(super) fn retain_included(excluded: &[PathBuf], found: Vec<PathBuf>) -> Vec<PathBuf> {
    found
        .into_iter()
        .filter(|path| !is_excluded(excluded, path))
        .collect()
}

