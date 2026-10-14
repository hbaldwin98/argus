//! The `argus desktop` command: opening the desktop window beside this
//! terminal client.

use std::path::Path;

use anyhow::Context;

use crate::sibling;

/// Starts `argus-desktop` detached, so closing this terminal leaves the
/// window open, and returns at once. The window reaches the daemon itself.
pub fn run() -> anyhow::Result<()> {
    open(&sibling::path("argus-desktop"))
}

fn open(exe: &Path) -> anyhow::Result<()> {
    match sibling::spawn_detached(exe) {
        Ok(()) => Ok(()),
        // Argus never installs anything; it says what is missing.
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Err(anyhow::anyhow!(
            "argus-desktop is neither beside argus nor on PATH. It ships in the release \
             archive beside argus; from a checkout, `cargo build -p argus-desktop`"
        )),
        Err(err) => Err(err).with_context(|| format!("could not start {}", exe.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_desktop_binary_says_where_to_get_one() {
        let err = open(Path::new("argus-desktop-that-is-not-installed")).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("neither beside argus nor on PATH"), "{message}");
        assert!(message.contains("cargo build -p argus-desktop"), "{message}");
    }
}
