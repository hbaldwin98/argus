//! Where the daemon's own diagnostics go.
//!
//! The client starts `argusd` detached with its stderr on the null device,
//! since the daemon shares no console and anything it printed would land
//! in the middle of the TUI. So it writes to a file as well — otherwise a
//! daemon-side problem could only be looked into by abandoning the client
//! and starting `argusd` by hand, which is exactly when the interesting
//! state is gone. Stderr is kept for a daemon run from a terminal.

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

/// This run's log. Beside the config rather than in a log directory of its
/// own, so `ARGUS_CONFIG_DIR` moves it too: a throwaway daemon must not
/// write over the log of the one the user is actually running.
pub fn log_path() -> PathBuf {
    argus_protocol::config_dir().join("argusd.log")
}

/// The run before this one. A daemon that is starting has often just
/// stopped in a way somebody wants to read about.
fn previous_log_path() -> PathBuf {
    argus_protocol::config_dir().join("argusd.log.1")
}

/// Opens this run's log, moving the last one aside. Every step is
/// best-effort: logging to a file must never be why a daemon fails to
/// start, so an unwritable directory costs the log and nothing else.
fn open_log(current: &Path, previous: &Path) -> Option<File> {
    if let Some(dir) = current.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if current.exists() {
        let _ = std::fs::rename(current, previous);
    }
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(current)
        .ok()
}

/// Starts logging. `RUST_LOG` sets the filter, `info` if it says nothing.
pub fn init() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    match open_log(&log_path(), &previous_log_path()) {
        // Behind a mutex so two threads logging at once cannot interleave
        // halves of each other's lines into the file.
        Some(file) => subscriber(std::io::stderr.and(Mutex::new(file)), filter).init(),
        None => subscriber(std::io::stderr, filter).init(),
    }
}

/// The subscriber both destinations share. Separate from [`init`] so a test
/// can install it against a real file and check a line comes out the far
/// end — `init` itself can only be called once per process.
fn subscriber<W>(writer: W, filter: EnvFilter) -> impl tracing::Subscriber + Send + Sync
where
    W: for<'a> MakeWriter<'a> + Send + Sync + 'static,
{
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        // Colour belongs to a terminal, and the destination that reliably
        // is not one is the file — the one anybody will actually read.
        .with_ansi(false)
        .with_writer(writer)
        .finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn paths(dir: &Path) -> (PathBuf, PathBuf) {
        (dir.join("argusd.log"), dir.join("argusd.log.1"))
    }

    #[test]
    fn a_run_with_no_log_yet_starts_one() {
        let dir = tempfile::tempdir().unwrap();
        let (current, previous) = paths(dir.path());

        assert!(open_log(&current, &previous).is_some());

        assert!(current.exists());
        assert!(!previous.exists(), "there was no previous run to keep");
    }

    #[test]
    fn the_previous_runs_log_is_kept_rather_than_truncated() {
        // The whole point: a daemon usually restarts because something went
        // wrong, and the account of what went wrong is in the log it is
        // about to replace.
        let dir = tempfile::tempdir().unwrap();
        let (current, previous) = paths(dir.path());
        std::fs::write(&current, "what the last run said").unwrap();

        let mut log = open_log(&current, &previous).unwrap();
        log.write_all(b"this run").unwrap();
        drop(log);

        assert_eq!(
            std::fs::read_to_string(&previous).unwrap(),
            "what the last run said"
        );
        assert_eq!(std::fs::read_to_string(&current).unwrap(), "this run");
    }

    #[test]
    fn only_one_previous_run_is_kept() {
        // Two files, not a directory that grows without bound for as long
        // as the user keeps restarting.
        let dir = tempfile::tempdir().unwrap();
        let (current, previous) = paths(dir.path());

        for run in ["first", "second", "third"] {
            let mut log = open_log(&current, &previous).unwrap();
            log.write_all(run.as_bytes()).unwrap();
        }

        assert_eq!(std::fs::read_to_string(&current).unwrap(), "third");
        assert_eq!(std::fs::read_to_string(&previous).unwrap(), "second");
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            2,
            "runs must not accumulate a file each"
        );
    }

    #[test]
    fn a_logged_line_reaches_the_file() {
        // The point of the whole module: the daemon's stderr goes to the
        // null device, so unless a line lands here it is not written down
        // anywhere at all.
        let dir = tempfile::tempdir().unwrap();
        let (current, previous) = paths(dir.path());
        let file = open_log(&current, &previous).unwrap();

        tracing::subscriber::with_default(
            subscriber(Mutex::new(file), EnvFilter::new("info")),
            || tracing::info!("a line the daemon wrote"),
        );

        let logged = std::fs::read_to_string(&current).unwrap();
        assert!(logged.contains("a line the daemon wrote"), "{logged:?}");
        assert!(
            !logged.contains('\u{1b}'),
            "a file should not be given colour: {logged:?}"
        );
    }

    #[test]
    fn a_log_that_cannot_be_opened_is_not_a_startup_failure() {
        // A read-only or otherwise unusable config directory costs the log
        // and nothing else — the daemon still has panes to run.
        let dir = tempfile::tempdir().unwrap();
        let blocked = dir.path().join("not-a-directory");
        std::fs::write(&blocked, "").unwrap();
        let (current, previous) = paths(&blocked);

        assert!(open_log(&current, &previous).is_none());
    }
}
