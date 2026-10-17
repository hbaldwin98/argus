//! What the daemon reads from Git to keep its tree level with the disk,
//! behind one port, so a test can say what Git would have answered —
//! including that it could not answer — without building the repository
//! that would make it say so.

use std::path::{Path, PathBuf};

use argus_protocol::GitStatus;

/// The reads `sync`, `agents::follow` and startup make. Writes are not
/// here: what a write does to a repository is the thing under test, so
/// those tests use a real one.
pub(super) trait GitReads: Send + Sync {
    /// A checkout's full status: branch, dirty counts, ahead and behind.
    fn status(&self, path: &Path) -> Option<GitStatus>;
    /// Only what HEAD names, which is enough to name a row and costs no
    /// walk of the working tree.
    fn head(&self, path: &Path) -> Option<GitStatus>;
    /// Every worktree of the repository whose primary checkout is `path`.
    fn list_worktrees(&self, path: &Path) -> Vec<PathBuf>;
    /// The repositories under a project's root, as its scan rules allow.
    fn discover_repositories(&self, root: &Path, scan: &crate::git::Scan) -> Vec<PathBuf>;
}

/// The production adapter: the real repository on disk.
pub(super) struct SystemGit;

impl GitReads for SystemGit {
    fn status(&self, path: &Path) -> Option<GitStatus> {
        crate::git::status(path)
    }

    fn head(&self, path: &Path) -> Option<GitStatus> {
        crate::git::head(path)
    }

    fn list_worktrees(&self, path: &Path) -> Vec<PathBuf> {
        crate::git::list_worktrees(path)
    }

    fn discover_repositories(&self, root: &Path, scan: &crate::git::Scan) -> Vec<PathBuf> {
        crate::git::discover_repositories_within(root, scan)
    }
}

#[cfg(test)]
pub(super) use scripted::ScriptedGit;

#[cfg(test)]
mod scripted {
    use std::sync::Mutex;

    use super::*;

    /// A scripted read for one path; `None` until the test says.
    type Answer<R> = Mutex<Option<Box<dyn Fn(&Path) -> R + Send>>>;

    type Scan = Mutex<Option<Box<dyn Fn(&Path, &crate::git::Scan) -> Vec<PathBuf> + Send>>>;

    /// The test adapter: answers what the test said Git would, and asks the
    /// disk for anything it was not told — so a daemon built over a real
    /// temporary repository still starts the way production does, and a
    /// test scripts only the read it is about.
    #[derive(Default)]
    pub(in crate::state) struct ScriptedGit {
        status: Answer<Option<GitStatus>>,
        worktrees: Answer<Vec<PathBuf>>,
        repositories: Scan,
    }

    impl ScriptedGit {
        /// What every status read answers from now on, HEAD-only ones too:
        /// a scripted checkout has no working tree for the two to differ on.
        pub(in crate::state) fn reads_status(
            &self,
            answer: impl Fn(&Path) -> Option<GitStatus> + Send + 'static,
        ) {
            *lock(&self.status) = Some(Box::new(answer));
        }

        pub(in crate::state) fn lists_worktrees(
            &self,
            answer: impl Fn(&Path) -> Vec<PathBuf> + Send + 'static,
        ) {
            *lock(&self.worktrees) = Some(Box::new(answer));
        }

        pub(in crate::state) fn discovers(
            &self,
            answer: impl Fn(&Path, &crate::git::Scan) -> Vec<PathBuf> + Send + 'static,
        ) {
            *lock(&self.repositories) = Some(Box::new(answer));
        }
    }

    /// A scripted answer that panicked has said what it had to; the next
    /// read should not fail on the poison.
    fn lock<T>(slot: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
        slot.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn answer<R>(slot: &Answer<R>, path: &Path) -> Option<R> {
        lock(slot).as_ref().map(|answer| answer(path))
    }

    impl GitReads for ScriptedGit {
        fn status(&self, path: &Path) -> Option<GitStatus> {
            answer(&self.status, path).unwrap_or_else(|| SystemGit.status(path))
        }

        fn head(&self, path: &Path) -> Option<GitStatus> {
            answer(&self.status, path).unwrap_or_else(|| SystemGit.head(path))
        }

        fn list_worktrees(&self, path: &Path) -> Vec<PathBuf> {
            answer(&self.worktrees, path).unwrap_or_else(|| SystemGit.list_worktrees(path))
        }

        fn discover_repositories(&self, root: &Path, scan: &crate::git::Scan) -> Vec<PathBuf> {
            match lock(&self.repositories).as_ref() {
                Some(answer) => answer(root, scan),
                None => SystemGit.discover_repositories(root, scan),
            }
        }
    }
}
