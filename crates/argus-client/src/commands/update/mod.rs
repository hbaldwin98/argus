//! `argus update`: this installation's executables replaced with the newest
//! release's from GitHub, checked against the release's checksum, and the
//! daemon restarted onto them. `argus update --check` only says whether
//! there is a newer one.
//!
//! The executables replaced are the ones beside the running `argus`,
//! wherever they came from: a `cargo install` is updated to the release
//! like an unpacked archive is.

pub mod archive;
pub mod download;
pub mod progress;
pub mod release;
pub mod swap;

use std::path::{Path, PathBuf};

use argus_protocol::connection::transport;

use crate::connection::launch;
use download::Fetcher;
use progress::Progress;
use release::Version;

/// An error already shown to the person as a failed step, so `main` has
/// nothing to add.
#[derive(Debug)]
struct Shown;

impl std::fmt::Display for Shown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the update failed")
    }
}

impl std::error::Error for Shown {}

pub async fn run(check: bool) -> anyhow::Result<()> {
    // `ureq` blocks, so the download and the swap leave the runtime's
    // threads to it.
    let installed = tokio::task::spawn_blocking(move || fetch_and_install(check)).await?;
    let result = match installed {
        Ok(Some((progress, version))) => restart(progress, version).await,
        Ok(None) => Ok(()),
        Err(error) => Err(error),
    };
    match result {
        Err(error) if error.is::<Shown>() => std::process::exit(1),
        result => result,
    }
}

/// The new release installed, with what shows the steps after it; `None`
/// when there is nothing to install or only a check was asked for.
fn fetch_and_install(check: bool) -> anyhow::Result<Option<(Progress, Version)>> {
    let mut progress = Progress::on_stderr();
    let current = Version::this_build();
    let target = release::this_target().ok_or_else(|| {
        anyhow::anyhow!(
            "no release is built for {} on {}: build Argus from source to update it",
            std::env::consts::OS,
            std::env::consts::ARCH
        )
    })?;
    let fetcher = Fetcher::new();
    let latest = fetcher
        .text(&release::latest_url())
        .and_then(|answer| release::latest_version(&answer))
        .map_err(|error| error.context("could not find the latest release"))?;
    if latest < current {
        eprintln!("Argus {current} is newer than the latest release, {latest}.");
        return Ok(None);
    }
    if latest == current {
        eprintln!("Argus {current} is the latest release.");
        return Ok(None);
    }
    if check {
        eprintln!("Argus {latest} is out; this is {current}. Run `argus update` to install it.");
        return Ok(None);
    }
    let dir = install_dir()?;

    progress.heading(&format!("Updating Argus {current} → {latest}"));
    let name = release::archive_name(latest, target);
    progress.start_download();
    let archive = step(&mut progress, "Download", |progress| {
        fetcher.archive(&release::download_url(latest, &name), &mut |done, total| progress.downloaded(done, total))
    })?;
    let took = progress.elapsed();
    progress.done("Downloaded", &format!("{} in {:.1}s", progress::size(archive.len() as u64), took.as_secs_f64()));

    step(&mut progress, "Verify", |_| {
        let checksum = fetcher.text(&release::download_url(latest, &format!("{name}.sha256")))?;
        download::verify(&archive, &name, &checksum)
    })?;
    progress.done("Verified", "SHA-256 matches the release's");

    let package = release::package(latest, target);
    let executables = step(&mut progress, "Unpack", |_| {
        archive::executables(&archive, name.ends_with(".zip"), &package, std::env::consts::EXE_SUFFIX)
    })?;
    step(&mut progress, "Install", |_| swap::install(&dir, &executables))?;
    progress.done("Installed", &format!("{} executables in {}", executables.len(), shown(&dir)));
    Ok(Some((progress, latest)))
}

/// Restarts the daemon onto the new executables, unless that would end
/// the shell asking: a restart run inside an Argus pane kills that pane's
/// process with the old daemon.
async fn restart(mut progress: Progress, version: Version) -> anyhow::Result<()> {
    if !transport::is_daemon_listening() {
        // The next `argus` starts the new one.
    } else if std::env::var_os(argus_protocol::PANE_VAR).is_some() {
        progress.note(
            "Restart",
            "argusd still runs the old version: run `argus server restart` from a terminal outside \
             Argus, since here it would end this shell",
        );
    } else {
        launch::restart_daemon().await.map_err(|error| {
            progress.failed("Restart", &format!("{error:#}"));
            anyhow::Error::new(Shown)
        })?;
        progress.done("Restarted", "argusd, with its panes restored");
    }
    progress.heading(&format!("\nArgus {version} is installed."));
    eprintln!("An open desktop window or terminal UI runs the new version once reopened.");
    Ok(())
}

/// Runs one step, showing its failure in place of the bar.
fn step<T>(
    progress: &mut Progress,
    name: &str,
    run: impl FnOnce(&mut Progress) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    run(progress).map_err(|error| {
        progress.failed(name, &format!("{error:#}"));
        anyhow::Error::new(Shown)
    })
}

/// The folder the running `argus` is in, through any link to it, since the
/// link's own folder holds no other executable.
fn install_dir() -> anyhow::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    exe.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| anyhow::anyhow!("{} is in no folder", exe.display()))
}

/// A path as the person would write it: Windows' canonical form starts
/// `\\?\`, which nobody types.
fn shown(path: &Path) -> String {
    let text = path.display().to_string();
    text.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(text)
}
