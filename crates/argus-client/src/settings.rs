//! Client-side preferences, and where they live on disk.
//!
//! The daemon's `projects.toml` is about what exists; this is about how one
//! client draws and behaves, so it is a file of its own and is never
//! rewritten by the daemon.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Where an editor opens (DESIGN.md §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditorMode {
    /// A floating window over the columns. The default: a terminal editor
    /// in a 38%-wide column is unusable.
    Overlay,
    /// A pane in the rightmost column, alongside the tree.
    Column,
    /// Launched outside Argus entirely, with no pty — for editors that
    /// bring their own window.
    External,
}

impl EditorMode {
    pub const ALL: &'static [EditorMode] =
        &[EditorMode::Overlay, EditorMode::Column, EditorMode::External];

    pub fn label(self) -> &'static str {
        match self {
            EditorMode::Overlay => "floating window",
            EditorMode::Column => "in the column",
            EditorMode::External => "outside argus",
        }
    }

    /// What choosing this actually does, for the settings panel — a label
    /// alone leaves the reader guessing.
    pub fn detail(self) -> &'static str {
        match self {
            EditorMode::Overlay => "a large panel above the tree; best for vim, helix, emacs",
            EditorMode::Column => "shares the rightmost column with the live pane",
            EditorMode::External => "spawn and forget; for editors with their own window",
        }
    }

    pub fn next(self) -> Self {
        let i = EditorMode::ALL.iter().position(|m| *m == self).unwrap_or(0);
        EditorMode::ALL[(i + 1) % EditorMode::ALL.len()]
    }

    pub fn prev(self) -> Self {
        let i = EditorMode::ALL.iter().position(|m| *m == self).unwrap_or(0);
        EditorMode::ALL[(i + EditorMode::ALL.len() - 1) % EditorMode::ALL.len()]
    }

    /// Whether the daemon should spawn this without a pty.
    pub fn is_external(self) -> bool {
        self == EditorMode::External
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub editor: EditorMode,
    /// A preset name from `theme::THEMES`.
    pub theme: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            editor: EditorMode::Overlay,
            theme: crate::theme::THEMES[0].to_string(),
        }
    }
}

pub fn path() -> PathBuf {
    argus_protocol::config_dir().join("client.toml")
}

/// Missing or unreadable settings fall back to the defaults rather than
/// stopping the client — a corrupt preference file should cost you your
/// preferences, not your session.
pub fn load() -> Settings {
    let Ok(raw) = std::fs::read_to_string(path()) else {
        return Settings::default();
    };
    match toml::from_str(&raw) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("ignoring {}: {e}", path().display());
            Settings::default()
        }
    }
}

/// Best-effort: failing to remember a preference is not worth failing the
/// change the user just made.
pub fn save(settings: &Settings) {
    let p = path();
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match toml::to_string_pretty(settings) {
        Ok(text) => {
            if let Err(e) = std::fs::write(&p, text) {
                tracing::warn!("could not save settings to {}: {e}", p.display());
            }
        }
        Err(e) => tracing::warn!("could not serialize settings: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_editor_is_the_floating_window() {
        // The column is too narrow for a terminal editor, which is the
        // whole reason the overlay exists.
        assert_eq!(Settings::default().editor, EditorMode::Overlay);
    }

    #[test]
    fn cycling_the_editor_mode_visits_every_option_and_comes_back() {
        let mut m = EditorMode::Overlay;
        for _ in 0..EditorMode::ALL.len() {
            m = m.next();
        }
        assert_eq!(m, EditorMode::Overlay);
    }

    #[test]
    fn next_and_prev_are_inverses() {
        for m in EditorMode::ALL {
            assert_eq!(m.next().prev(), *m);
        }
    }

    #[test]
    fn only_the_external_mode_skips_the_pty() {
        assert!(EditorMode::External.is_external());
        assert!(!EditorMode::Overlay.is_external());
        assert!(!EditorMode::Column.is_external());
    }

    #[test]
    fn every_mode_explains_itself() {
        // The panel exists to be read; a bare enum name would not help.
        for m in EditorMode::ALL {
            assert!(!m.label().is_empty());
            assert!(!m.detail().is_empty());
        }
    }

    #[test]
    fn settings_survive_a_round_trip_through_toml() {
        let s = Settings {
            editor: EditorMode::External,
            theme: "latte".to_string(),
        };
        let back: Settings = toml::from_str(&toml::to_string_pretty(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn a_file_missing_a_key_keeps_the_default_for_it() {
        // Settings files outlive the versions that wrote them.
        let s: Settings = toml::from_str(r#"theme = "frappe""#).unwrap();
        assert_eq!(s.theme, "frappe");
        assert_eq!(s.editor, Settings::default().editor);
    }

    #[test]
    fn an_unparseable_value_is_an_error_rather_than_a_silent_default() {
        // `load` turns this into a warning; the parse itself must not
        // quietly invent a mode.
        assert!(toml::from_str::<Settings>(r#"editor = "telepathy""#).is_err());
    }
}
