//! Read-only git status for a checkout: branch, dirty/clean, ahead/behind
//! upstream. See DESIGN.md §4 Level 2 and §9 M2. No mutation here — `switch`
//! and worktree add/remove are a later slice.

use std::path::Path;

use orion_protocol::GitStatus;

/// Returns `None` if `path` isn't inside a git repo at all. Any other
/// failure (e.g. a transient lock) also degrades to `None` rather than
/// erroring the whole tree snapshot — status is best-effort.
pub fn status(path: &Path) -> Option<GitStatus> {
    let repo = git2::Repository::open(path).ok()?;

    let head = repo.head().ok();
    let branch = head
        .as_ref()
        .and_then(|h| h.shorthand())
        .filter(|s| *s != "HEAD") // detached HEAD shorthand is literally "HEAD"
        .map(str::to_string);

    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true).renames_head_to_index(true);
    let changed_files = repo
        .statuses(Some(&mut opts))
        .map(|s| s.iter().count())
        .unwrap_or(0);

    let (ahead, behind) = ahead_behind(&repo, head.as_ref()).unwrap_or((0, 0));

    Some(GitStatus {
        branch,
        dirty: changed_files > 0,
        changed_files,
        ahead,
        behind,
    })
}

fn ahead_behind(repo: &git2::Repository, head: Option<&git2::Reference>) -> Option<(usize, usize)> {
    let head = head?;
    let local_oid = head.target()?;
    let branch_name = head.shorthand()?;
    let branch = repo.find_branch(branch_name, git2::BranchType::Local).ok()?;
    let upstream = branch.upstream().ok()?;
    let upstream_oid = upstream.get().target()?;
    repo.graph_ahead_behind(local_oid, upstream_oid).ok()
}
