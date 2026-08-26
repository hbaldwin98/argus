//! Watching each repository's Git metadata, so a branch switch, a commit,
//! or a worktree made in a shell reaches the tree when it happens rather
//! than on the next two-second tick.
//!
//! This supplements the poll rather than replacing it: editing a file
//! touches nothing under `.git`, so dirty state and changed-file counts
//! still need the sweep. What the watch covers is everything that
//! restructures the tree — HEAD, refs, the index, the worktree registry —
//! which is also everything an agent does that the user is waiting to see.
//!
//! Deliberately *not* a recursive watch of the whole Git directory:
//! `.git/objects` takes thousands of writes during a single commit or
//! fetch, and every one of them would be an event to coalesce for a change
//! that `refs` reports once. The watched set is the Git directory itself
//! (HEAD, index, packed-refs), its `refs` tree, and its `worktrees` tree,
//! which is where a linked worktree's own HEAD and index live.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use notify::{RecursiveMode, Watcher};

/// A filesystem watch over the Git metadata of every repository in the
/// tree, kept in step with a repository list that changes as projects are
/// scanned, added, and removed.
pub struct GitWatch {
    watcher: notify::RecommendedWatcher,
    watched: HashSet<PathBuf>,
}

impl GitWatch {
    /// `on_change` runs on the watcher's own thread for every event, so it
    /// must do nothing but wake whoever is going to do the work.
    pub fn new(on_change: impl Fn() + Send + 'static) -> Option<Self> {
        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if res.is_ok() {
                on_change();
            }
        });
        match watcher {
            Ok(watcher) => Some(Self {
                watcher,
                watched: HashSet::new(),
            }),
            Err(e) => {
                tracing::warn!("no git watch: {e}; the poll is on its own");
                None
            }
        }
    }

    /// Watches what `git_dirs` implies and drops what is no longer in it.
    /// Paths that don't exist yet are simply not watched — a repository
    /// cloned into a project root later is picked up the next time this
    /// runs, and until then the poll covers it.
    pub fn sync(&mut self, git_dirs: &[PathBuf]) {
        let wanted: HashSet<PathBuf> = git_dirs
            .iter()
            .flat_map(|dir| metadata_paths(dir))
            .collect();

        for gone in self.watched.difference(&wanted) {
            let _ = self.watcher.unwatch(gone);
        }
        let added: Vec<PathBuf> = wanted.difference(&self.watched).cloned().collect();
        for path in added {
            // Non-recursive on the Git directory keeps `objects` out of it;
            // `refs` and `worktrees` are small enough to take whole.
            let mode = if path.ends_with("refs") || path.ends_with("worktrees") {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };
            if let Err(e) = self.watcher.watch(&path, mode) {
                tracing::debug!("could not watch {}: {e}", path.display());
                continue;
            }
            self.watched.insert(path);
        }
        self.watched.retain(|p| wanted.contains(p));
    }
}

/// The paths under one Git directory worth watching, those that exist.
fn metadata_paths(git_dir: &Path) -> Vec<PathBuf> {
    [
        git_dir.to_path_buf(),
        git_dir.join("refs"),
        git_dir.join("worktrees"),
    ]
    .into_iter()
    .filter(|p| p.is_dir())
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_metadata_directories_that_exist_are_watched() {
        let dir = tempfile::tempdir().unwrap();
        let git_dir = dir.path().join(".git");
        std::fs::create_dir_all(git_dir.join("refs")).unwrap();

        let paths = metadata_paths(&git_dir);

        assert!(paths.contains(&git_dir), "HEAD and the index live here");
        assert!(paths.contains(&git_dir.join("refs")));
        assert!(
            !paths.contains(&git_dir.join("worktrees")),
            "a repo with no linked worktrees has no such directory"
        );
        assert!(
            !paths.contains(&git_dir.join("objects")),
            "objects is the one directory this must never watch"
        );
    }

    #[test]
    fn syncing_drops_a_repository_that_left_the_tree() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a").join(".git");
        let b = dir.path().join("b").join(".git");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let mut watch = GitWatch::new(|| {}).expect("a watcher on this platform");

        watch.sync(&[a.clone(), b.clone()]);
        assert_eq!(watch.watched.len(), 2);

        watch.sync(std::slice::from_ref(&a));
        assert_eq!(watch.watched, HashSet::from([a]));
    }

    #[test]
    fn syncing_the_same_list_twice_watches_nothing_twice() {
        let dir = tempfile::tempdir().unwrap();
        let git_dir = dir.path().join(".git");
        std::fs::create_dir_all(git_dir.join("refs")).unwrap();
        let mut watch = GitWatch::new(|| {}).expect("a watcher on this platform");

        watch.sync(std::slice::from_ref(&git_dir));
        let first = watch.watched.clone();
        watch.sync(&[git_dir]);

        assert_eq!(watch.watched, first);
    }
}
