//! Which browser runs, and starting it: headless, on a profile of its own
//! that goes with it, spoken to over a pipe it is handed rather than a port
//! any process on the machine could reach.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use anyhow::Context;
use argus_protocol::BROWSER_VAR;

use super::pipe::{self, Pipe};

/// A running browser. Dropped, it is killed — with every process it
/// started, on Windows, by its job — and its profile deleted.
pub(crate) struct Chrome {
    // Fields drop in order: the browser before its job, both before the
    // profile it was writing to.
    _child: tokio::process::Child,
    #[cfg(windows)]
    _job: Option<crate::pty::ProcessJob>,
    profile: PathBuf,
}

impl Drop for Chrome {
    /// The profile is deleted off this thread, since the browser's
    /// processes let go of its files a moment after they are killed.
    fn drop(&mut self) {
        let profile = std::mem::take(&mut self.profile);
        std::thread::spawn(move || {
            for _ in 0..40 {
                if std::fs::remove_dir_all(&profile).is_ok() || !profile.exists() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(250));
            }
        });
    }
}

/// Where this daemon's browsers keep their profiles: one folder per
/// instance, so a daemon clears its own and never another's.
fn profiles() -> PathBuf {
    let instance = argus_protocol::instance_name().unwrap_or_else(|| "default".into());
    std::env::temp_dir().join(format!("argus-browsers-{instance}"))
}

/// Deletes every profile a daemon of this instance left: one killed
/// outright never deleted its browsers', and none of them runs now.
pub(crate) fn sweep() {
    let Ok(entries) = std::fs::read_dir(profiles()) else { return };
    for entry in entries.flatten() {
        let _ = std::fs::remove_dir_all(entry.path());
    }
}

/// Starts the browser, and the pipe the daemon speaks to it over.
pub(crate) fn start() -> anyhow::Result<(Chrome, Pipe)> {
    let program = find().context(format!(
        "no Chrome, Chromium or Edge was found; set {BROWSER_VAR} to the browser to run"
    ))?;
    std::fs::create_dir_all(profiles())?;
    let profile = tempfile::Builder::new().prefix("profile-").tempdir_in(profiles())?.keep();
    let (pipe, ends) = pipe::make()?;
    // Not system::command's: the browser takes no part of the daemon's
    // environment that `quiet` strips, and it keeps its own console hidden
    // below.
    #[allow(clippy::disallowed_methods)]
    let mut command = tokio::process::Command::new(&program);
    // A headless screencast is drawn at the browser's own pixel density,
    // whatever density a page is emulated at, so it is made dense enough for
    // a high-density screen; a lower one is sent pictures scaled down to it.
    command
        .arg("--headless=new")
        .arg("--force-device-scale-factor=2")
        .arg(format!("--user-data-dir={}", profile.display()))
        .args(["--no-first-run", "--no-default-browser-check", "--window-size=1280,800"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    ends.hand_to(&mut command);
    command.arg("about:blank");
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);
    let child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&profile);
            return Err(error).with_context(|| format!("starting {}", program.display()));
        }
    };
    // The browser holds its ends now; the daemon's copies would keep the
    // pipe open after the browser closed its own.
    drop(ends);
    #[cfg(windows)]
    let job = contain(&child);
    Ok((
        Chrome {
            _child: child,
            #[cfg(windows)]
            _job: job,
            profile,
        },
        pipe,
    ))
}

/// The browser `ARGUS_BROWSER` names, or the first installed of those each
/// system keeps where its installer puts them.
fn find() -> Option<PathBuf> {
    if let Some(named) = std::env::var_os(BROWSER_VAR) {
        return Some(PathBuf::from(named));
    }
    candidates().into_iter().find(|path| path.is_file())
}

#[cfg(windows)]
fn candidates() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = ["ProgramFiles", "ProgramFiles(x86)", "LocalAppData"]
        .iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .collect();
    roots.dedup();
    let mut found = Vec::new();
    for app in [r"Google\Chrome\Application\chrome.exe", r"Chromium\Application\chrome.exe"] {
        found.extend(roots.iter().map(|root| root.join(app)));
    }
    found.extend(roots.iter().map(|root| root.join(r"Microsoft\Edge\Application\msedge.exe")));
    found
}

#[cfg(target_os = "macos")]
fn candidates() -> Vec<PathBuf> {
    [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
    ]
    .iter()
    .map(PathBuf::from)
    .collect()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn candidates() -> Vec<PathBuf> {
    let Some(path) = std::env::var_os("PATH") else { return Vec::new() };
    let dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
    ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser", "microsoft-edge"]
        .iter()
        .flat_map(|name| dirs.iter().map(move |dir| dir.join(name)))
        .collect()
}

/// A job that ends the browser and its renderers when the daemon's handle
/// to it closes, or nothing when one cannot be made.
#[cfg(windows)]
fn contain(child: &tokio::process::Child) -> Option<crate::pty::ProcessJob> {
    let job = crate::pty::ProcessJob::shell().ok()?;
    job.assign(child.raw_handle()?).ok()?;
    Some(job)
}
