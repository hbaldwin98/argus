//! Where the window's own diagnostics go.
//!
//! A window started from a launcher or by `argus desktop` has no console,
//! so it writes to a file beside the config, as `argusd` does, keeping the
//! run before as `.1`. Stderr is kept for a window run from a terminal.
//! A panic is written there too, since it is the one way the window dies.

use std::backtrace::Backtrace;
use std::fs::{File, OpenOptions};
use std::panic::PanicHookInfo;
use std::path::Path;
use std::sync::Mutex;

use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

/// Starts logging to `<stem>.log` and `<stem>.log.1`, a stem per window so
/// two windows open at once do not overwrite one another's log.
/// `RUST_LOG` sets the filter, `info` if it says nothing.
/// Nothing here can stop the window starting: an unwritable directory
/// costs the log and nothing else.
pub fn init(stem: &str) {
    let dir = argus_protocol::config_dir();
    let current = dir.join(format!("{stem}.log"));
    let previous = dir.join(format!("{stem}.log.1"));
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let fmt = tracing_subscriber::fmt().with_env_filter(filter).with_ansi(false);
    let _ = match open_log(&current, &previous) {
        Some(file) => fmt.with_writer(std::io::stderr.and(Mutex::new(file))).finish().try_init(),
        None => fmt.with_writer(std::io::stderr).finish().try_init(),
    };
    log_panics();
}

/// Writes a panic to the log before it ends the window. A release build
/// aborts on any panic, a background task's included, and the default
/// hook writes only to a stderr a launched window does not have: without
/// this a panic closes the window and leaves no word of why.
fn log_panics() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!("{}\n{}", panic_words(info), Backtrace::force_capture());
        default(info);
    }));
}

fn panic_words(info: &PanicHookInfo<'_>) -> String {
    let message = info
        .payload()
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
        .unwrap_or("a panic with no message");
    let thread = std::thread::current();
    let thread = thread.name().unwrap_or("unnamed");
    match info.location() {
        Some(at) => format!("panicked on thread {thread} at {at}: {message}"),
        None => format!("panicked on thread {thread}: {message}"),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::Arc;

    #[derive(Clone, Default)]
    struct Kept(Arc<Mutex<Vec<u8>>>);

    impl Write for Kept {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_panic_is_logged_with_where_it_happened() {
        log_panics();
        let kept = Kept::default();
        let writer = kept.clone();
        let subscriber = tracing_subscriber::fmt().with_ansi(false).with_writer(move || writer.clone()).finish();
        let line = line!() + 2;
        let caught = tracing::subscriber::with_default(subscriber, || {
            std::panic::catch_unwind(|| panic!("the tree had no project {}", 7))
        });
        assert!(caught.is_err());
        let log = String::from_utf8(kept.0.lock().unwrap().clone()).unwrap();
        assert!(log.contains("ERROR"), "{log}");
        assert!(log.contains("the tree had no project 7"), "{log}");
        assert!(log.contains(&format!("logging.rs:{line}")), "{log}");
    }
}
