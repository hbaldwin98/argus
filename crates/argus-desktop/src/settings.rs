//! What the window remembers between runs: how each repository's feature
//! index is laid out, whether an agent coming to need you chimes, and the
//! side and layout a review opens on.
//!
//! Its own file beside the config, `desktop.json`, rather than a section
//! of the terminal client's `client.toml`: whether the two clients share
//! their settings is still open (HUN-185), and a file of its own is the
//! choice that is easy to fold in later.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// `list` or `board`, by the path of a repository's primary checkout:
    /// the path outlives the daemon's ids, which are handed out anew each run.
    #[serde(default)]
    pub layouts: BTreeMap<String, String>,
    /// On unless turned off, unlike the terminal client's opt-in bell: a
    /// window is not where a person is already typing, so it is the place
    /// they are least likely to see an agent finish.
    #[serde(default = "on")]
    pub chime: bool,
    /// The review's side and layout as last chosen, which the next review
    /// opens on: as in the terminal client, a setting rather than a choice
    /// made again on each visit.
    #[serde(default)]
    pub review: ReviewLayout,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewLayout {
    #[serde(default)]
    pub staged: bool,
    #[serde(default)]
    pub split: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { layouts: BTreeMap::new(), chime: true, review: ReviewLayout::default() }
    }
}

fn on() -> bool {
    true
}

fn path() -> PathBuf {
    argus_protocol::config_dir().join("desktop.json")
}

/// The remembered settings, or none at all: a missing or unreadable file
/// is a window that remembers nothing yet, never one that will not open.
pub fn load() -> Settings {
    load_from(&path())
}

/// Best effort, written beside the file and moved over it, so a window
/// closed mid-write leaves the old settings rather than half of new ones.
pub fn save(settings: &Settings) {
    if let Err(err) = save_to(&path(), settings) {
        tracing::warn!("could not save the window's settings: {err}");
    }
}

fn load_from(path: &Path) -> Settings {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save_to(path: &Path, settings: &Settings) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let partial = path.with_extension("json.partial");
    std::fs::write(&partial, serde_json::to_vec_pretty(settings)?)?;
    std::fs::rename(&partial, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("argus-desktop-settings-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("desktop.json")
    }

    #[test]
    fn settings_saved_are_the_settings_loaded() {
        let path = scratch("round");
        let mut settings = Settings::default();
        settings.layouts.insert("/src/argus".into(), "board".into());
        settings.chime = false;
        settings.review = ReviewLayout { staged: true, split: true };
        save_to(&path, &settings).unwrap();
        assert_eq!(load_from(&path), settings);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_missing_or_broken_file_remembers_nothing() {
        let path = scratch("broken");
        assert_eq!(load_from(&path), Settings::default());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"{ not json").unwrap();
        assert_eq!(load_from(&path), Settings::default());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_file_from_before_the_chime_chimes() {
        let path = scratch("before-chime");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, br#"{ "layouts": { "/src/argus": "board" } }"#).unwrap();
        let settings = load_from(&path);
        assert!(settings.chime);
        assert_eq!(settings.review, ReviewLayout::default(), "unstaged and unified, as before");
        assert_eq!(settings.layouts["/src/argus"], "board");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
