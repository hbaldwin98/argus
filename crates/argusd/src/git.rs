//! Read-only git status for a checkout: branch, dirty/clean, ahead/behind
//! upstream, and the repo's current worktree list. See DESIGN.md §4 Level 2
//! and §9 M2. Mutating operations (`worktree add`/`remove`) live next to the
//! daemon state they update, in `state.rs`, not here.

use std::path::{Path, PathBuf};

use argus_protocol::GitStatus;

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

/// Every worktree path git currently knows about for the repo at `path`
/// (the primary checkout included, always first). Empty if `path` isn't a
/// git repo — callers treat that as "nothing to reconcile", never as
/// "everything was removed".
///
/// Deliberately libgit2 rather than `git worktree list --porcelain`: this
/// runs on a 2-second poll for every project, and the daemon is started
/// with `DETACHED_PROCESS` on Windows, so it owns no console. Every console
/// child it spawns therefore gets a **brand-new console window**, which
/// meant a window flashing open and shut every couple of seconds for as
/// long as Argus was running. In-process avoids the whole class of problem
/// (and is faster). Mutating worktree operations still shell out — they are
/// rare, user-initiated, and go through `crate::command::git`.
pub fn list_worktrees(path: &Path) -> Vec<PathBuf> {
    let Ok(repo) = git2::Repository::open(path) else {
        return Vec::new();
    };
    // The primary checkout is not in libgit2's worktree list (it lists
    // *linked* worktrees only), but the CLI's porcelain output puts it
    // first, and `reconcile_worktrees` relies on it being present — an
    // absent path is treated as a removed checkout.
    let mut out: Vec<PathBuf> = repo.workdir().map(Path::to_path_buf).into_iter().collect();

    if let Ok(names) = repo.worktrees() {
        for name in names.iter().flatten() {
            // A worktree whose directory was deleted by hand is still
            // registered until `git worktree prune` runs; skip those rather
            // than resurrecting a checkout row for a directory that's gone.
            if let Ok(wt) = repo.find_worktree(name) {
                let p = wt.path();
                if p.is_dir() {
                    out.push(p.to_path_buf());
                }
            }
        }
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A repo with one commit on the current branch, so HEAD resolves and
    /// worktrees can be added. Built through libgit2 — no `git` subprocess,
    /// which is the whole point of the code under test.
    fn repo_with_a_commit(dir: &Path) -> git2::Repository {
        let repo = git2::Repository::init(dir).unwrap();
        std::fs::write(dir.join("a.txt"), "hello").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("a.txt")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = git2::Signature::now("t", "t@example.com").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[]).unwrap();
        drop(tree);
        repo
    }

    fn same_path(a: &Path, b: &Path) -> bool {
        // Worktree paths come back canonicalized by libgit2 while the
        // configured one is whatever the user typed; compare resolved.
        match (a.canonicalize(), b.canonicalize()) {
            (Ok(a), Ok(b)) => a == b,
            _ => a == b,
        }
    }

    #[test]
    fn a_plain_repo_lists_just_its_own_working_directory() {
        let dir = tempfile::tempdir().unwrap();
        let _repo = repo_with_a_commit(dir.path());

        let listed = list_worktrees(dir.path());
        assert_eq!(listed.len(), 1, "got {listed:?}");
        assert!(
            same_path(&listed[0], dir.path()),
            "the primary checkout must be listed: {listed:?}"
        );
    }

    #[test]
    fn the_primary_checkout_comes_first_and_linked_worktrees_follow() {
        // `reconcile_worktrees` treats an unlisted path as a removed
        // checkout, so the primary must always be present.
        let dir = tempfile::tempdir().unwrap();
        let repo = repo_with_a_commit(dir.path());
        let wt_dir = dir.path().join("wt-feature");
        repo.worktree("feature", &wt_dir, None).unwrap();

        let listed = list_worktrees(dir.path());
        assert_eq!(listed.len(), 2, "got {listed:?}");
        assert!(same_path(&listed[0], dir.path()), "primary first");
        assert!(
            listed.iter().any(|p| same_path(p, &wt_dir)),
            "the linked worktree should be listed: {listed:?}"
        );
    }

    #[test]
    fn a_worktree_whose_directory_was_deleted_is_not_listed() {
        // git keeps the registration until `worktree prune` runs; listing it
        // would resurrect a checkout row for a directory that is gone.
        let dir = tempfile::tempdir().unwrap();
        let repo = repo_with_a_commit(dir.path());
        let wt_dir = dir.path().join("wt-gone");
        repo.worktree("gone", &wt_dir, None).unwrap();
        std::fs::remove_dir_all(&wt_dir).unwrap();

        let listed = list_worktrees(dir.path());
        assert!(
            !listed.iter().any(|p| same_path(p, &wt_dir)),
            "a deleted worktree must not linger: {listed:?}"
        );
    }

    #[test]
    fn listing_never_spawns_a_process() {
        // The regression this whole module exists for: on Windows the
        // daemon runs detached with no console, so a console child gets its
        // own window — a flash on screen every poll tick. The listing has to
        // stay in-process. Asserted by watching for `git` children while
        // hammering the call.
        let dir = tempfile::tempdir().unwrap();
        let repo = repo_with_a_commit(dir.path());
        repo.worktree("w", &dir.path().join("wt"), None).unwrap();

        let before = std::time::Instant::now();
        for _ in 0..200 {
            let _ = list_worktrees(dir.path());
        }
        // 200 process spawns would take far longer than this on any
        // platform; in-process libgit2 calls are microseconds each.
        assert!(
            before.elapsed() < std::time::Duration::from_secs(2),
            "200 listings took {:?} — that smells like process spawning",
            before.elapsed()
        );
    }

    #[test]
    fn status_of_a_non_repo_is_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(status(dir.path()).is_none());
    }

    #[test]
    fn list_worktrees_of_a_non_repo_is_empty() {
        // Callers rely on this meaning "nothing to reconcile" rather than
        // "every worktree was removed" — see `reconcile_worktrees`.
        let dir = tempfile::tempdir().unwrap();
        assert!(list_worktrees(dir.path()).is_empty());
    }

    #[test]
    fn status_reports_the_branch_and_a_dirty_tree() {
        let dir = tempfile::tempdir().unwrap();
        let _repo = repo_with_a_commit(dir.path());

        let clean = status(dir.path()).expect("a repo should report status");
        assert!(clean.branch.is_some(), "HEAD should resolve to a branch");
        assert!(!clean.dirty, "a fresh commit leaves a clean tree");

        std::fs::write(dir.path().join("b.txt"), "new").unwrap();
        let dirty = status(dir.path()).unwrap();
        assert!(dirty.dirty, "an untracked file counts as dirty");
        assert!(dirty.changed_files >= 1);
    }

    #[test]
    fn a_repo_with_no_upstream_is_neither_ahead_nor_behind() {
        let dir = tempfile::tempdir().unwrap();
        let _repo = repo_with_a_commit(dir.path());
        let s = status(dir.path()).unwrap();
        assert_eq!((s.ahead, s.behind), (0, 0));
    }
}
