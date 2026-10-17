//! Watching one file for as long as the watcher is held, through an
//! editor's save and without waking on the daemon's own writes beside it.

use std::path::{Path, PathBuf};

use notify::{RecursiveMode, Watcher};

/// Watches one file — `projects.toml` — for as long as the returned
/// watcher is held.
///
/// The registration is on the directory the file sits in, not on the file:
/// an editor's save is a write to a temp file and a rename over the target,
/// and a watch on the file itself does not survive that. The filtering is
/// what keeps the difference invisible to the caller.
pub fn watch(path: &Path, on_change: impl Fn() + Send + 'static) -> Option<impl Watcher> {
    let dir = path.parent()?.to_path_buf();
    let file = path.to_path_buf();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        // Opening and reading are reported too, and a reload reads the
        // file: woken by its own read, it would never stop reloading.
        if let Ok(event) = res {
            let read = matches!(event.kind, notify::EventKind::Access(_));
            if !read && concerns(&event.paths, &file) {
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
/// It has to be asked: the daemon writes its log and its store into the
/// directory `projects.toml` lives in, and reloading the config logs a
/// line — a loop that never runs out of fuel. Matched on file name, since
/// a non-recursive watch only reports entries of that one directory and
/// names sidestep the separator differences between a canonical path and
/// what the backend hands back.
///
/// An event carrying no paths is a backend rescan, not a write. Nothing
/// the daemon does causes one, so waking on it cannot loop.
fn concerns(paths: &[PathBuf], file: &Path) -> bool {
    paths.is_empty() || paths.iter().any(|p| p.file_name() == file.file_name())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_the_watched_file_does_not_wake_the_reload() {
        // A reload reads the file. Were a read a wakeup, the first edit
        // would start a reload that wakes the next, four times a second
        // for as long as the daemon ran.
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("projects.toml");
        std::fs::write(&config, "# before").unwrap();
        let woke = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = woke.clone();
        let _watch = watch(&config, move || {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
        .unwrap();
        // FSEvents hands over what happened shortly before the watch
        // started, so the write that made the file can still arrive.
        std::thread::sleep(std::time::Duration::from_millis(300));
        woke.store(0, std::sync::atomic::Ordering::SeqCst);

        let _ = std::fs::read_to_string(&config).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert_eq!(woke.load(std::sync::atomic::Ordering::SeqCst), 0, "a read is not a change");

        std::fs::write(&config, "# after").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(woke.load(std::sync::atomic::Ordering::SeqCst) > 0, "a write is");
    }

    #[test]
    fn only_the_watched_file_wakes_the_reload() {
        let config = PathBuf::from("/cfg/projects.toml");

        assert!(concerns(std::slice::from_ref(&config), &config));
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
}
