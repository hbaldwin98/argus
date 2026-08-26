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

/// What removing the checkout at `worktree` from the repository at
/// `primary` would run into, decided before anything is touched.
///
/// Removing a worktree has to kill its panes first — on Windows a shell
/// sitting in the directory is itself what stops the directory being
/// deleted — so any refusal git discovers on its own arrives after the
/// user's agents are already dead. Every refusal that can be seen coming is
/// therefore made here, while the panes are still running.
pub enum Removal {
    /// Nothing here stands in git's way.
    Ready,
    /// The directory is already gone and only the registration is left, which
    /// `git worktree remove` refuses rather than cleans up.
    Stale,
    /// Git would refuse, for this reason.
    Blocked(String),
}

pub fn removal(primary: &Path, worktree: &Path) -> Removal {
    let Ok(repo) = git2::Repository::open(primary) else {
        return Removal::Blocked(format!("{} is not a git repository", primary.display()));
    };
    let registered = repo.worktrees().ok().and_then(|names| {
        names
            .iter()
            .flatten()
            .filter_map(|name| repo.find_worktree(name).ok())
            .find(|wt| same_path(wt.path(), worktree))
    });
    let Some(worktree_ref) = registered else {
        // Either a directory that reached the tree some other way — the
        // primary of another repository, say — or a registration git has
        // already pruned. Only the second is safe to treat as done.
        return if worktree.is_dir() {
            Removal::Blocked(format!(
                "{} is not a linked worktree of this repository",
                worktree.display()
            ))
        } else {
            Removal::Stale
        };
    };
    if let Ok(git2::WorktreeLockStatus::Locked(reason)) = worktree_ref.is_locked() {
        let why = reason.unwrap_or_else(|| "no reason given".to_string());
        return Removal::Blocked(format!("this worktree is locked: {}", why.trim()));
    }
    if worktree.is_dir() {
        Removal::Ready
    } else {
        Removal::Stale
    }
}

/// The Git directory of the repository at `path` — `<path>/.git` for an
/// ordinary checkout, wherever it points for anything else. `None` if
/// `path` is not a repository.
pub fn git_dir(path: &Path) -> Option<PathBuf> {
    git2::Repository::open(path)
        .ok()
        .map(|repo| repo.path().to_path_buf())
}

/// Whether the repository at `path` already has a local branch by this
/// name — the difference between giving an existing branch a worktree and
/// starting a new one.
pub fn has_local_branch(path: &Path, branch: &str) -> bool {
    git2::Repository::open(path)
        .and_then(|repo| repo.find_branch(branch, git2::BranchType::Local).map(|_| ()))
        .is_ok()
}

/// Worktree paths come back from libgit2 canonicalized while a configured
/// one is however the user spelled it; compare them resolved.
fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// Directories a project scan never walks into: Git's own storage, the
/// worktrees Argus creates (they belong to the repository above them), and
/// two build/dependency trees big enough to dominate the walk on their own.
const SKIPPED: [&str; 4] = [".git", ".argus", "node_modules", "target"];

/// How far below the root a repository can sit and still be found. A project
/// root is a directory holding repositories, not a filesystem to crawl, and
/// the cap keeps a mistyped root (`C:\` or `/`) from turning reconciliation
/// into a disk scan.
const MAX_DEPTH: usize = 8;

/// Every distinct Git repository at or beneath `root`, ordered by path so
/// the repository column doesn't reshuffle between scans.
///
/// The walk stops at each repository it finds: what lives inside a
/// repository — a submodule, a vendored checkout, a linked worktree — belongs
/// to that repository rather than beside it as a sibling row. Linked
/// worktrees are never rows of their own for the same reason, wherever they
/// sit, and neither are bare repositories, which have nothing to check out.
/// Directory symlinks are not followed, so a scan can neither cycle nor
/// wander out of the root.
///
/// Like the rest of this module it is libgit2 rather than `git`: it runs on
/// the daemon's reconciliation tick, and see `list_worktrees` for what
/// spawning a process there costs on Windows.
pub fn discover_repositories(root: &Path) -> Vec<PathBuf> {
    // (identity, working directory). The identity is the repository's Git
    // directory, resolved, so two paths that reach one repository collapse
    // to a single row; the working directory is kept as the walk spelled it,
    // which is the user's own root with directory names appended, rather
    // than whatever canonicalization would make of it.
    let mut found: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut pending = vec![(root.to_path_buf(), 0usize)];

    while let Some((dir, depth)) = pending.pop() {
        match look(&dir) {
            Site::Repository { identity } => {
                if !found.iter().any(|(seen, _)| *seen == identity) {
                    found.push((identity, dir));
                }
                continue;
            }
            Site::Boundary => continue,
            Site::Plain => {}
        }
        if depth == MAX_DEPTH {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            // `file_type` here describes the link itself, not its target.
            if kind.is_symlink() || !kind.is_dir() {
                continue;
            }
            let name = entry.file_name();
            if SKIPPED.iter().any(|skip| name == *skip) {
                continue;
            }
            pending.push((entry.path(), depth + 1));
        }
    }

    let mut out: Vec<PathBuf> = found.into_iter().map(|(_, dir)| dir).collect();
    // `pending` is a stack, so the walk order is the filesystem's. Sorting
    // is what makes the result the same list every tick.
    out.sort();
    out
}

/// What a scan makes of one directory.
enum Site {
    /// A repository with a working directory of its own, identified by its
    /// resolved Git directory.
    Repository { identity: PathBuf },
    /// Git, but not a row: a linked worktree, whose repository already
    /// stands for it, or a bare repository, which has no checkout. Either
    /// way nothing underneath is a sibling repository.
    Boundary,
    /// Not Git.
    Plain,
}

fn look(dir: &Path) -> Site {
    // `Repository::open` does not search parent directories, so this asks
    // whether `dir` *is* a repository, not whether it sits inside one.
    let Ok(repo) = git2::Repository::open(dir) else {
        return Site::Plain;
    };
    if repo.is_worktree() || repo.workdir().is_none() {
        return Site::Boundary;
    }
    let git_dir = repo.path();
    Site::Repository {
        identity: git_dir
            .canonicalize()
            .unwrap_or_else(|_| git_dir.to_path_buf()),
    }
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
    fn a_directory_that_is_not_this_repos_worktree_is_never_removed() {
        // Whatever else it is, deleting it is not `git worktree remove`'s
        // job — and the caller kills panes on anything but a refusal.
        let dir = tempfile::tempdir().unwrap();
        let _repo = repo_with_a_commit(dir.path());
        let elsewhere = dir.path().join("just-a-directory");
        std::fs::create_dir(&elsewhere).unwrap();

        assert!(matches!(
            removal(dir.path(), &elsewhere),
            Removal::Blocked(_)
        ));
        assert!(matches!(
            removal(dir.path(), dir.path()),
            Removal::Blocked(_)
        ));
    }

    #[test]
    fn a_worktree_is_ready_to_remove_until_something_holds_it() {
        let dir = tempfile::tempdir().unwrap();
        let repo = repo_with_a_commit(dir.path());
        let wt_dir = dir.path().join("wt-feature");
        repo.worktree("feature", &wt_dir, None).unwrap();

        assert!(matches!(removal(dir.path(), &wt_dir), Removal::Ready));

        repo.find_worktree("feature")
            .unwrap()
            .lock(Some("mid-rebase"))
            .unwrap();
        let Removal::Blocked(why) = removal(dir.path(), &wt_dir) else {
            panic!("a locked worktree is git's own refusal");
        };
        assert!(why.contains("mid-rebase"), "got {why:?}");
    }

    #[test]
    fn a_registration_left_by_a_deleted_directory_is_stale_rather_than_blocked() {
        let dir = tempfile::tempdir().unwrap();
        let repo = repo_with_a_commit(dir.path());
        let wt_dir = dir.path().join("wt-gone");
        repo.worktree("gone", &wt_dir, None).unwrap();
        std::fs::remove_dir_all(&wt_dir).unwrap();

        assert!(matches!(removal(dir.path(), &wt_dir), Removal::Stale));
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

    /// Discovery hands back the walk's own spelling of each path while
    /// tempdir paths and libgit2's differ on some platforms, so compare
    /// resolved.
    fn discovers(root: &Path, expected: &[PathBuf]) {
        let found = discover_repositories(root);
        assert_eq!(
            found.len(),
            expected.len(),
            "expected {expected:?}, found {found:?}"
        );
        for (got, want) in found.iter().zip(expected) {
            assert!(
                same_path(got, want),
                "expected {expected:?}, found {found:?}"
            );
        }
    }

    #[test]
    fn every_repository_under_a_root_is_found_in_path_order() {
        // The order is the promise: the repository column is drawn from this
        // list every tick and must not reshuffle.
        let dir = tempfile::tempdir().unwrap();
        for name in ["beta", "alpha", "gamma"] {
            let child = dir.path().join(name);
            std::fs::create_dir(&child).unwrap();
            let _repo = repo_with_a_commit(&child);
        }

        discovers(
            dir.path(),
            &[
                dir.path().join("alpha"),
                dir.path().join("beta"),
                dir.path().join("gamma"),
            ],
        );
    }

    #[test]
    fn a_root_that_is_itself_a_repository_is_the_only_repository() {
        // The common case, and the one that has to keep behaving exactly as
        // `repos = ["~/src/argus"]` always did: one repository, at the root.
        let dir = tempfile::tempdir().unwrap();
        let _repo = repo_with_a_commit(dir.path());
        std::fs::create_dir_all(dir.path().join("crates/argusd/src")).unwrap();

        discovers(dir.path(), &[dir.path().to_path_buf()]);
    }

    #[test]
    fn a_repository_several_directories_down_is_found() {
        // How a checkout tree that mirrors a host actually looks:
        // ~/src/github.com/owner/repo.
        let dir = tempfile::tempdir().unwrap();
        let deep = dir.path().join("github.com/owner/repo");
        std::fs::create_dir_all(&deep).unwrap();
        let _repo = repo_with_a_commit(&deep);

        discovers(dir.path(), &[deep]);
    }

    #[test]
    fn a_plain_directory_holds_no_repositories() {
        // A root can be empty and stay a perfectly good project: a
        // repository cloned into it later is picked up by the next scan.
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("notes/drafts")).unwrap();

        assert!(discover_repositories(dir.path()).is_empty());
    }

    #[test]
    fn a_directory_inside_a_repository_is_not_a_repository_of_its_own() {
        // libgit2 will happily walk up to the enclosing repository if asked
        // to; discovery must ask the other question — is *this* directory a
        // repository — or every subdirectory becomes a row.
        let dir = tempfile::tempdir().unwrap();
        let _repo = repo_with_a_commit(dir.path());
        let inner = dir.path().join("src");
        std::fs::create_dir(&inner).unwrap();

        assert!(discover_repositories(&inner).is_empty());
    }

    #[test]
    fn build_and_dependency_directories_are_never_walked() {
        // Real repositories do turn up under node_modules and target. They
        // are not the user's projects, and walking those trees would dwarf
        // the rest of the scan.
        let dir = tempfile::tempdir().unwrap();
        let mine = dir.path().join("mine");
        std::fs::create_dir(&mine).unwrap();
        let _repo = repo_with_a_commit(&mine);

        for buried in ["node_modules/dep", "target/vendored", ".git/hidden"] {
            let path = dir.path().join(buried);
            std::fs::create_dir_all(&path).unwrap();
            let _repo = repo_with_a_commit(&path);
        }

        discovers(dir.path(), &[mine]);
    }

    #[test]
    fn worktrees_argus_created_are_not_repositories_of_their_own() {
        // `.argus/worktrees/<branch>` is a checkout of the repository beside
        // it. Listing it as a repository would split one repository's
        // checkouts across two rows.
        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("orion");
        std::fs::create_dir(&repo_dir).unwrap();
        let repo = repo_with_a_commit(&repo_dir);
        std::fs::create_dir_all(repo_dir.join(".argus/worktrees")).unwrap();
        repo.worktree("feature", &repo_dir.join(".argus/worktrees/feature"), None)
            .unwrap();

        discovers(dir.path(), &[repo_dir]);
    }

    #[test]
    fn a_linked_worktree_anywhere_is_not_a_repository_of_its_own() {
        // Same rule away from `.argus`, where the skip list can't help: the
        // worktree has to identify itself as one.
        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("orion");
        std::fs::create_dir(&repo_dir).unwrap();
        let repo = repo_with_a_commit(&repo_dir);
        repo.worktree("feature", &dir.path().join("elsewhere"), None)
            .unwrap();

        discovers(dir.path(), &[repo_dir]);
    }

    #[test]
    fn a_bare_repository_is_not_a_repository_row() {
        // Nothing to check out, so nothing a pane could open.
        let dir = tempfile::tempdir().unwrap();
        git2::Repository::init_bare(dir.path().join("mirror.git")).unwrap();

        assert!(discover_repositories(dir.path()).is_empty());
    }

    #[test]
    fn directory_symlinks_are_not_followed() {
        // A link pointing at an ancestor is a cycle, and one pointing out of
        // the root drags in repositories the user never named.
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let _repo = repo_with_a_commit(&real);

        let link = dir.path().join("link");
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_dir(&real, &link).is_ok();
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&real, &link).is_ok();
        if !made {
            // Windows needs Developer Mode or elevation to make one.
            return;
        }

        discovers(dir.path(), &[real]);
    }
}
