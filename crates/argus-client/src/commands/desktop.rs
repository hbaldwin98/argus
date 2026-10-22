//! The `argus desktop` and `argus desktop-next` commands: opening a desktop
//! window beside this terminal client, the Tauri one or the GPUI one.

use std::path::Path;

use anyhow::Context;

use crate::system::sibling;

/// Starts `argus-desktop` detached, so closing this terminal leaves the
/// window open, and returns at once. The window reaches the daemon itself.
pub fn run() -> anyhow::Result<()> {
    open(&sibling::path("argus-desktop"), "argus-desktop")
}

/// The same for `argus-desktop-next`, the GPUI window run beside the Tauri
/// one while it is ported.
pub fn run_next() -> anyhow::Result<()> {
    open(&sibling::path("argus-desktop-next"), "argus-gpui")
}

/// `package` is what `cargo build -p` names for the binary, which for the
/// GPUI window is not the binary's own name.
fn open(exe: &Path, package: &str) -> anyhow::Result<()> {
    match sibling::spawn_detached(exe) {
        Ok(()) => Ok(()),
        // Argus never installs anything; it says what is missing.
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Err(anyhow::anyhow!(
            "{} is neither beside argus nor on PATH. It ships in the release \
             archive beside argus; from a checkout, `cargo build -p {package}`",
            exe.file_stem().and_then(|stem| stem.to_str()).unwrap_or("the desktop window")
        )),
        Err(err) => Err(err).with_context(|| format!("could not start {}", exe.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_desktop_binary_says_where_to_get_one() {
        let err = open(Path::new("argus-desktop-that-is-not-installed"), "argus-desktop").unwrap_err();
        let message = err.to_string();
        assert!(message.contains("neither beside argus nor on PATH"), "{message}");
        assert!(message.contains("cargo build -p argus-desktop`"), "{message}");
    }

    #[test]
    fn a_missing_desktop_next_binary_names_the_gpui_package() {
        let err = open(Path::new("argus-desktop-next-that-is-not-installed"), "argus-gpui").unwrap_err();
        let message = err.to_string();
        assert!(message.contains("argus-desktop-next-that-is-not-installed is neither"), "{message}");
        assert!(message.contains("cargo build -p argus-gpui`"), "{message}");
    }
}
