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

/// Watches one file — `projects.toml` — for as long as the returned
/// watcher is held.
///
/// The registration is on the directory the file sits in, not on the file:
/// an editor's save is a write to a temp file and a rename over the target,
/// and a watch on the file itself does not survive that. The filtering is
/// what keeps the difference invisible to the caller.
pub fn file(path: &Path, on_change: impl Fn() + Send + 'static) -> Option<impl Watcher> {
    let dir = path.parent()?.to_path_buf();
    let file = path.to_path_buf();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            if concerns(&event.paths, &file) {
                on_change();
            }
        }
    })
    .map_err(|e| tracing::warn!("no watch on {}: {e}", dir.display()))
    .ok()?;
    watcher
        .watch(&dir, RecursiveMode::NonRecursive)
        .map_err(|e| tracing::warn!("no watch on {}: {e}", dir.display()))
        .ok()?;
    Some(watcher)
}

/// Whether an event is about the file being watched.
///
/// It has to be asked. The daemon writes its log and its store into the
/// same directory `projects.toml` lives in, so without this every line
/// logged is an event — and reloading the config logs a line, which is a
/// loop that never runs out of fuel. Matching on the file name is exact
/// here: a non-recursive watch only ever reports entries of that one
/// directory, and comparing names sidesteps the separator and prefix
/// differences between a canonical path and what the backend hands back.
///
/// An event carrying no paths at all is a backend rescan rather than a
/// write. Nothing the daemon does causes one, so waking on it cannot loop,
/// and reading the config once too often costs less than missing an edit.
fn concerns(paths: &[PathBuf], file: &Path) -> bool {
    paths.is_empty() || paths.iter().any(|p| p.file_name() == file.file_name())
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
    fn only_the_watched_file_wakes_the_reload() {
        let config = PathBuf::from("/cfg/projects.toml");

        assert!(concerns(&[config.clone()], &config));
        assert!(
            !concerns(&[PathBuf::from("/cfg/argusd.log")], &config),
            "the daemon logging a line must not read as a config edit"
        );
        assert!(
            !concerns(&[PathBuf::from("/cfg/runtime.db")], &config),
            "nor must the store it writes beside it"
        );
        assert!(
            concerns(&[], &config),
            "a rescan says nothing about which file changed, so take it"
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
