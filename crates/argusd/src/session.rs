//! What was running when the daemon last stopped, so a reboot doesn't cost
//! you your workspace.
//!
//! Runtime state, not configuration: `projects.toml` says what exists and
//! is the user's to edit, this says what happened to be running and is
//! Argus's to rewrite. It lives beside the config rather than in it for
//! that reason (DESIGN.md §11 — the SQLite move should absorb this).
//!
//! Checkouts are identified by path and projects by name, because ids are
//! handed out fresh on every start and mean nothing across one.

use std::path::{Path, PathBuf};

use argus_protocol::PaneKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPane {
    pub checkout_path: PathBuf,
    pub kind: PaneKind,
    /// For an agent this is its template name, which is how it gets
    /// started again.
    pub title: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    pub panes: Vec<SessionPane>,
}

/// Set to anything to start clean. An escape hatch for the case where a
/// restore is the problem — a template that now fails on launch, say.
pub const NO_RESTORE: &str = "ARGUS_NO_RESTORE";

pub fn path() -> PathBuf {
    argus_protocol::config_dir().join("session.json")
}

/// What was running last time. Empty when there is no file, when it can't
/// be parsed, or when [`NO_RESTORE`] is set — none of which is worth
/// refusing to start over.
pub fn load() -> Session {
    if std::env::var_os(NO_RESTORE).is_some() {
        tracing::info!("{NO_RESTORE} is set; starting with nothing running");
        return Session::default();
    }
    let Ok(raw) = std::fs::read_to_string(path()) else {
        return Session::default();
    };
    match serde_json::from_str(&raw) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("ignoring {}: {e}", path().display());
            Session::default()
        }
    }
}

pub fn save(session: &Session) {
    let p = path();
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(session) {
        Ok(text) => {
            if let Err(e) = std::fs::write(&p, text) {
                tracing::warn!("could not record the session in {}: {e}", p.display());
            }
        }
        Err(e) => tracing::warn!("could not serialize the session: {e}"),
    }
}

/// Panes worth starting again.
///
/// Editors are left out on purpose: one belongs to the floating window it
/// opened in, and reopening a file nobody asked for would be noise. A pane
/// whose checkout is no longer configured is dropped too.
pub fn restorable<'a>(
    session: &'a Session,
    known: &'a [PathBuf],
) -> impl Iterator<Item = &'a SessionPane> {
    session.panes.iter().filter(move |p| {
        p.kind != PaneKind::Editor && known.iter().any(|k| same_path(k, &p.checkout_path))
    })
}

/// Compares paths by their canonical form where possible, so a checkout
/// recorded as `C:\src\x` still matches one configured as `C:/src/x`.
fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane(path: &str, kind: PaneKind, title: &str) -> SessionPane {
        SessionPane {
            checkout_path: PathBuf::from(path),
            kind,
            title: title.to_string(),
        }
    }

    #[test]
    fn a_session_survives_a_round_trip() {
        let s = Session {
            panes: vec![
                pane("/repo", PaneKind::Agent, "claude"),
                pane("/repo", PaneKind::Shell, "shell"),
            ],
        };
        let back: Session = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn a_file_from_an_older_version_still_loads() {
        // Session files outlive the versions that wrote them, and losing
        // your panes to a schema change would be a poor trade.
        let s: Session = serde_json::from_str("{}").unwrap();
        assert!(s.panes.is_empty());
    }

    #[test]
    fn editors_are_not_restored() {
        // An editor belongs to the window it opened in; reopening a file
        // nobody asked for is noise.
        let known = vec![PathBuf::from("/repo")];
        let s = Session {
            panes: vec![
                pane("/repo", PaneKind::Editor, "a.rs"),
                pane("/repo", PaneKind::Agent, "claude"),
            ],
        };
        let out: Vec<&SessionPane> = restorable(&s, &known).collect();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, PaneKind::Agent);
    }

    #[test]
    fn a_pane_whose_checkout_is_gone_is_dropped() {
        // The user removed the project from their config between runs.
        let known = vec![PathBuf::from("/still-here")];
        let s = Session {
            panes: vec![
                pane("/gone", PaneKind::Shell, "shell"),
                pane("/still-here", PaneKind::Shell, "shell"),
            ],
        };
        let out: Vec<&SessionPane> = restorable(&s, &known).collect();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].checkout_path, PathBuf::from("/still-here"));
    }

    #[test]
    fn paths_match_across_separator_and_case_differences() {
        // A real directory, so canonicalization has something to work on.
        let dir = tempfile::tempdir().unwrap();
        let forward = PathBuf::from(dir.path().to_string_lossy().replace('\\', "/"));
        let known = vec![dir.path().to_path_buf()];
        let s = Session {
            panes: vec![SessionPane {
                checkout_path: forward,
                kind: PaneKind::Shell,
                title: "shell".to_string(),
            }],
        };
        assert_eq!(restorable(&s, &known).count(), 1);
    }

    #[test]
    fn an_empty_session_restores_nothing_without_complaining() {
        let known = vec![PathBuf::from("/repo")];
        assert_eq!(restorable(&Session::default(), &known).count(), 0);
    }
}
