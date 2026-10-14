//! Where the window's own diagnostics go.
//!
//! A window started from a launcher or by `argus desktop` has no console,
//! so it writes to a file beside the config, as `argusd` does, keeping the
//! run before as `.1`. Stderr is kept for a window run from a terminal.

use std::fs::{File, OpenOptions};
use std::path::Path;
use std::sync::Mutex;

use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

/// Starts logging. `RUST_LOG` sets the filter, `info` if it says nothing.
/// Nothing here can stop the window starting: an unwritable directory
/// costs the log and nothing else.
pub fn init() {
    let dir = argus_protocol::config_dir();
    let current = dir.join("argus-desktop.log");
    let previous = dir.join("argus-desktop.log.1");
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let fmt = tracing_subscriber::fmt().with_env_filter(filter).with_ansi(false);
    let _ = match open_log(&current, &previous) {
        Some(file) => fmt.with_writer(std::io::stderr.and(Mutex::new(file))).finish().try_init(),
        None => fmt.with_writer(std::io::stderr).finish().try_init(),
    };
}

fn open_log(current: &Path, previous: &Path) -> Option<File> {
    if let Some(dir) = current.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if current.exists() {
        let _ = std::fs::rename(current, previous);
    }
    OpenOptions::new().create(true).write(true).truncate(true).open(current).ok()
}
