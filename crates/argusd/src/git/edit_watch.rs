//! Watching the working files of each checkout being worked in, so an edit
//! reaches its status, and a review open on it, as it happens rather than
//! on the next two-second sweep.
//!
//! The metadata watch beside this one (`watch`) cannot see an edit: writing
//! a file touches nothing under `.git`. This watch sees every write in the
//! checkout instead, and most of them are of no interest — a build writing
//! thousands of files into an ignored `target`, or Git's own writes under
//! `.git` — so it only gathers the paths, and [`edited`] says afterwards
//! which checkouts any of them could show up in.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use notify::{RecursiveMode, Watcher};

/// How many distinct paths one gathering keeps. A storm past it is taken to
/// touch every watched checkout, rather than held in memory to be sorted.
const KEPT: usize = 4096;

/// What arrived since the last [`EditWatch::take`].
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Gathered {
    pub paths: HashSet<PathBuf>,
    /// More arrived than were kept.
    pub overflowed: bool,
}

/// A recursive watch over a set of checkouts, kept in step with the set.
pub struct EditWatch {
    watcher: notify::RecommendedWatcher,
    watched: HashSet<PathBuf>,
    gathered: Arc<Mutex<Gathered>>,
}

impl EditWatch {
    /// `on_change` runs on the watcher's own thread after a path is
    /// gathered, so it must do nothing but wake whoever takes them.
    pub fn new(on_change: impl Fn() + Send + 'static) -> Option<Self> {
        let gathered = Arc::new(Mutex::new(Gathered::default()));
        let into = gathered.clone();
        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            // A status read opens the files it compares, and every one of
            // them would wake the read again.
            let Some(event) = res.ok().filter(|e| !matches!(e.kind, notify::EventKind::Access(_))) else {
                return;
            };
            let mut kept = into.lock().unwrap();
            let mut any = false;
            for path in event.paths.into_iter().filter(|p| !in_git_dir(p)) {
                any = true;
                if kept.paths.len() < KEPT {
                    kept.paths.insert(path);
                } else {
                    kept.overflowed = true;
                }
            }
            drop(kept);
            if any {
                on_change();
            }
        });
        match watcher {
            Ok(watcher) => Some(Self { watcher, watched: HashSet::new(), gathered }),
            Err(e) => {
                tracing::warn!("no edit watch: {e}; the poll is on its own");
                None
            }
        }
    }

    /// Watches each of `checkouts` and drops what is no longer among them.
    pub fn sync(&mut self, checkouts: &[PathBuf]) {
        let wanted: HashSet<PathBuf> = checkouts.iter().filter(|p| p.is_dir()).cloned().collect();
        for gone in self.watched.difference(&wanted) {
            let _ = self.watcher.unwatch(gone);
        }
        let added: Vec<PathBuf> = wanted.difference(&self.watched).cloned().collect();
        for path in added {
            // Recursive on Linux is a watch per directory, ignored ones
            // included, and can run out of the system's allowance; the
            // sweep still covers a checkout that could not be watched.
            if let Err(e) = self.watcher.watch(&path, RecursiveMode::Recursive) {
                tracing::debug!("could not watch {}: {e}", path.display());
                continue;
            }
            self.watched.insert(path);
        }
        self.watched.retain(|p| wanted.contains(p));
    }

    /// What arrived since last asked, leaving nothing behind.
    pub fn take(&self) -> Gathered {
        std::mem::take(&mut *self.gathered.lock().unwrap())
    }
}

/// Whether a path is Git's own: a status read writes the index, and a
/// commit thousands of objects, none of which is an edit.
fn in_git_dir(path: &Path) -> bool {
    path.components().any(|c| c == Component::Normal(".git".as_ref()))
}

/// Of `checkouts`, the ones a gathered path could show up in the status of:
/// it is inside one, deepest first, since a linked worktree may sit inside
/// its primary, and not ignored there. Every one of them on overflow.
pub fn edited<'a, K: Copy + Eq + std::hash::Hash>(
    checkouts: &'a [(K, PathBuf)],
    gathered: &Gathered,
) -> HashSet<K> {
    if gathered.overflowed {
        return checkouts.iter().map(|(k, _)| *k).collect();
    }
    // A backend may report the canonical spelling of what it was asked to
    // watch (macOS's `/private/var` for `/var`), so a path is matched
    // against both.
    let mut by_depth: Vec<(K, &'a PathBuf, Option<PathBuf>)> =
        checkouts.iter().map(|(k, root)| (*k, root, root.canonicalize().ok())).collect();
    by_depth.sort_by_key(|(_, p, _)| std::cmp::Reverse(p.components().count()));
    let mut repos: HashMap<K, Option<Ignores>> = HashMap::new();
    let mut edited = HashSet::new();
    for path in &gathered.paths {
        let inside = |root: &Path| path.strip_prefix(root).ok();
        let Some((key, root, rel)) =
            by_depth.iter().find_map(|(k, root, canon)| Some((*k, *root, inside(root).or_else(|| inside(canon.as_ref()?))?)))
        else {
            continue;
        };
        if edited.contains(&key) {
            continue;
        }
        let ignores = repos.entry(key).or_insert_with(|| Ignores::open(root));
        // A checkout that cannot be opened is read anyway: the sweep would.
        if ignores.as_mut().is_none_or(|i| !i.ignored(rel)) {
            edited.insert(key);
        }
    }
    edited
}

/// One checkout's ignore rules, with what was asked of each directory kept:
/// a build's writes are thousands of paths under one ignored directory.
struct Ignores {
    repo: git2::Repository,
    dirs: HashMap<PathBuf, bool>,
}

impl Ignores {
    fn open(root: &Path) -> Option<Ignores> {
        Some(Ignores { repo: git2::Repository::open(root).ok()?, dirs: HashMap::new() })
    }

    /// Whether `rel` is ignored, itself or by a directory it is in.
    fn ignored(&mut self, rel: &Path) -> bool {
        let dirs: Vec<&Path> = rel.ancestors().skip(1).filter(|a| !a.as_os_str().is_empty()).collect();
        for dir in dirs.into_iter().rev() {
            let repo = &self.repo;
            let ignored = *self.dirs.entry(dir.to_path_buf()).or_insert_with(|| repo.is_path_ignored(dir).unwrap_or(false));
            if ignored {
                return true;
            }
        }
        self.repo.is_path_ignored(rel).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(dir: &Path) {
        git2::Repository::init(dir).unwrap();
        std::fs::write(dir.join(".gitignore"), "target/\n*.log\n").unwrap();
    }

    fn gathered(paths: &[PathBuf]) -> Gathered {
        Gathered { paths: paths.iter().cloned().collect(), overflowed: false }
    }

    #[test]
    fn only_an_edit_status_would_see_wakes_its_checkout() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a"), dir.path().join("b"));
        for root in [&a, &b] {
            std::fs::create_dir_all(root).unwrap();
            repo(root);
        }
        let checkouts = vec![(1, a.clone()), (2, b.clone())];

        let ignored = [b.join("target/debug/deps/x.rlib"), b.join("build.log")];
        assert!(edited(&checkouts, &gathered(&ignored)).is_empty(), "an ignored file, or one in an ignored directory");
        assert!(edited(&checkouts, &gathered(&[dir.path().join("elsewhere.rs")])).is_empty(), "outside every checkout");

        let seen = edited(&checkouts, &gathered(&[a.join("src/lib.rs"), b.join("target/x")]));
        assert_eq!(seen, HashSet::from([1]));
    }

    #[test]
    fn a_worktree_inside_its_primary_is_the_one_edited() {
        let dir = tempfile::tempdir().unwrap();
        let primary = dir.path().to_path_buf();
        let inner = primary.join("wt");
        std::fs::create_dir_all(&inner).unwrap();
        repo(&primary);
        repo(&inner);
        let checkouts = vec![(1, primary.clone()), (2, inner.clone())];
        assert_eq!(edited(&checkouts, &gathered(&[inner.join("x.rs")])), HashSet::from([2]));
    }

    #[test]
    fn an_overflow_wakes_every_checkout() {
        let checkouts = vec![(1, PathBuf::from("a")), (2, PathBuf::from("b"))];
        let storm = Gathered { paths: HashSet::new(), overflowed: true };
        assert_eq!(edited(&checkouts, &storm), HashSet::from([1, 2]));
    }

    #[test]
    fn git_s_own_writes_are_not_edits() {
        assert!(in_git_dir(Path::new("/r/.git/index")));
        assert!(in_git_dir(Path::new("/r/.git/objects/ab/cd")));
        assert!(!in_git_dir(Path::new("/r/src/.gitignore")));
    }

    #[test]
    fn a_write_in_a_watched_checkout_is_gathered() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watch = EditWatch::new(move || {
            let _ = tx.send(());
        })
        .expect("a watcher on this platform");
        watch.sync(&[dir.path().to_path_buf()]);
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join("a.rs"), "x").unwrap();
        rx.recv_timeout(std::time::Duration::from_secs(5)).expect("the write wakes the watch");
        let taken = watch.take();
        assert!(taken.paths.iter().any(|p| p.ends_with("a.rs")), "{taken:?}");
        assert!(taken.paths.iter().all(|p| !in_git_dir(p)), "{taken:?}");
        assert_eq!(watch.take(), Gathered::default(), "taken once");
    }
}
