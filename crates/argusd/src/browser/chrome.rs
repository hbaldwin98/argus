//! Which browser runs, and starting it: headless, on a profile of its own
//! that goes with it, its DevTools socket on a loopback port it picks.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{bail, Context};
use argus_protocol::BROWSER_VAR;

/// How long a browser has to open its DevTools port: a cold start of
/// Chrome on a loaded machine takes a few seconds.
const STARTUP: Duration = Duration::from_secs(20);

/// A running browser. Dropped, it is killed — with every process it
/// started, on Windows, by its job — and its profile deleted.
pub(crate) struct Chrome {
    // Fields drop in order: the browser before its job, both before the
    // profile it was writing to.
    _child: tokio::process::Child,
    #[cfg(windows)]
    _job: Option<crate::pty::ProcessJob>,
    profile: PathBuf,
    /// The `ws://` DevTools endpoint of the browser as a whole.
    pub(crate) endpoint: String,
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

/// Starts the browser and waits for its DevTools endpoint.
pub(crate) async fn start() -> anyhow::Result<Chrome> {
    let program = find().context(format!(
        "no Chrome, Chromium or Edge was found; set {BROWSER_VAR} to the browser to run"
    ))?;
    let profile = tempfile::Builder::new().prefix("argus-browser-").tempdir()?.keep();
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
        .arg("--remote-debugging-port=0")
        .arg("--force-device-scale-factor=2")
        .arg(format!("--user-data-dir={}", profile.display()))
        .args(["--no-first-run", "--no-default-browser-check", "--window-size=1280,800", "about:blank"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);
    let mut child = command.spawn().with_context(|| format!("starting {}", program.display()))?;
    #[cfg(windows)]
    let job = contain(&child);
    let endpoint = match endpoint(&profile, &mut child).await {
        Ok(endpoint) => endpoint,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&profile);
            return Err(error);
        }
    };
    Ok(Chrome {
        _child: child,
        #[cfg(windows)]
        _job: job,
        profile,
        endpoint,
    })
}

/// The endpoint the browser writes into its profile as `DevToolsActivePort`
/// once its socket is open: the port, then the path, a line each.
async fn endpoint(profile: &Path, child: &mut tokio::process::Child) -> anyhow::Result<String> {
    let file = profile.join("DevToolsActivePort");
    let deadline = tokio::time::Instant::now() + STARTUP;
    while tokio::time::Instant::now() < deadline {
        if let Some(status) = child.try_wait()? {
            bail!("the browser exited as it started ({status})");
        }
        if let Some(endpoint) = std::fs::read_to_string(&file).ok().as_deref().and_then(read_active_port) {
            return Ok(endpoint);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    bail!("the browser did not open its DevTools port within {}s", STARTUP.as_secs())
}

/// `DevToolsActivePort`'s two lines as a URL; `None` while it is half
/// written.
fn read_active_port(text: &str) -> Option<String> {
    let mut lines = text.lines();
    let port: u16 = lines.next()?.trim().parse().ok()?;
    let path = lines.next()?.trim();
    path.starts_with("/devtools/browser/").then(|| format!("ws://127.0.0.1:{port}{path}"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_endpoint_once_both_lines_are_written() {
        assert_eq!(
            read_active_port("53412\n/devtools/browser/0b5e\n").as_deref(),
            Some("ws://127.0.0.1:53412/devtools/browser/0b5e")
        );
        assert_eq!(read_active_port("53412\n"), None);
        assert_eq!(read_active_port(""), None);
        assert_eq!(read_active_port("53412\n/json\n"), None);
    }
}
