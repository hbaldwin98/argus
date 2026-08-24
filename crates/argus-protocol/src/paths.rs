//! Where Argus keeps its files. Shared so the daemon and the client agree
//! on it — and so `ARGUS_CONFIG_DIR` points both of them at the same place.

use std::path::PathBuf;

/// `ARGUS_CONFIG_DIR` overrides the platform location. Tests need it, so
/// they never read or scribble on the real user's config, and it makes a
/// throwaway instance alongside a real one possible.
pub fn config_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("ARGUS_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    directories::ProjectDirs::from("", "", "argus")
        .map(|d| d.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}
