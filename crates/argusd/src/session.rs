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

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use argus_protocol::{PaneKind, PaneStatus};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPane {
    pub checkout_path: PathBuf,
    pub kind: PaneKind,
    /// What the row said when the daemon stopped — which, since an agent
    /// can rename its own pane, is not necessarily what to start.
    pub title: String,
    /// The agent template to spawn. Absent in a file written before agents
    /// could rename themselves, where the title was the template name.
    #[serde(default)]
    pub template: Option<String>,
    /// Last agent-reported state and its explanation. Defaults preserve
    /// compatibility with session files written before statuses were saved.
    #[serde(default = "idle_status")]
    pub status: PaneStatus,
    #[serde(default)]
    pub note: Option<String>,
}

fn idle_status() -> PaneStatus {
    PaneStatus::Idle
}

impl SessionPane {
    /// What to start again. The title only stands in for a file old enough
    /// to predate renaming.
    pub fn template(&self) -> &str {
        self.template.as_deref().unwrap_or(&self.title)
    }
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

/// What was running last time. `None` means the existing file could not be
/// read safely and must not be overwritten during this daemon run.
pub fn load() -> Option<Session> {
    if std::env::var_os(NO_RESTORE).is_some() {
        tracing::info!("{NO_RESTORE} is set; starting with nothing running");
        return Some(Session::default());
    }
    let p = path();
    let raw = match std::fs::read_to_string(&p) {
        Ok(raw) => raw,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Some(Session::default()),
        Err(e) => {
            tracing::warn!(
                "could not read {}: {e}; session recording is disabled",
                p.display()
            );
            return None;
        }
    };
    match serde_json::from_str(&raw) {
        Ok(s) => Some(s),
        Err(e) => {
            tracing::warn!(
                "could not parse {}: {e}; session recording is disabled",
                p.display()
            );
            None
        }
    }
}

pub fn save(session: &Session) {
    let p = path();
    if let Err(e) = save_to(&p, session) {
        tracing::warn!("could not record the session in {}: {e}", p.display());
    }
}

fn save_to(path: &Path, session: &Session) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "session path has no parent"))?;
    std::fs::create_dir_all(parent)?;

    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(temp.as_file_mut(), session)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    temp.as_file_mut().write_all(b"\n")?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;

    // A synced file plus rename protects its contents. Syncing the directory
    // also makes the new directory entry durable on Unix filesystems.
    #[cfg(unix)]
    std::fs::File::open(parent)?.sync_all()?;
    Ok(())
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
            template: None,
            status: PaneStatus::Idle,
            note: None,
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
    fn save_replaces_the_session_without_leaving_a_partial_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        let old = Session {
            panes: vec![pane("/old", PaneKind::Shell, "shell")],
        };
        let new = Session {
            panes: vec![pane("/new", PaneKind::Agent, "claude")],
        };

        save_to(&path, &old).unwrap();
        save_to(&path, &new).unwrap();

        let stored: Session =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(stored, new);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn a_file_from_an_older_version_still_loads() {
        // Session files outlive the versions that wrote them, and losing
        // your panes to a schema change would be a poor trade.
        let s: Session = serde_json::from_str("{}").unwrap();
        assert!(s.panes.is_empty());
    }

    #[test]
    fn a_pane_from_before_status_persistence_defaults_to_idle() {
        let s: Session = serde_json::from_str(
            r#"{"panes":[{"checkout_path":"/repo","kind":"Agent","title":"claude"}]}"#,
        )
        .unwrap();
        assert_eq!(s.panes[0].status, PaneStatus::Idle);
        assert_eq!(s.panes[0].note, None);
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
                template: None,
                status: PaneStatus::Idle,
                note: None,
            }],
        };
        assert_eq!(restorable(&s, &known).count(), 1);
    }

    #[test]
    fn a_renamed_agent_comes_back_as_the_template_it_was() {
        // Regression: an agent that renamed its own pane to what it was
        // working on would otherwise be looked up as a template by that
        // name, and never come back at all.
        let p = SessionPane {
            checkout_path: PathBuf::from("/repo"),
            kind: PaneKind::Agent,
            title: "fixing the pty deadlock".to_string(),
            template: Some("claude".to_string()),
            status: PaneStatus::NeedsReview,
            note: Some("ready to inspect".to_string()),
        };
        assert_eq!(p.template(), "claude");
    }

    #[test]
    fn a_file_written_before_renaming_still_names_its_template() {
        let p = pane("/repo", PaneKind::Agent, "claude");
        assert_eq!(p.template(), "claude", "the title was the template then");
    }

    #[test]
    fn an_empty_session_restores_nothing_without_complaining() {
        let known = vec![PathBuf::from("/repo")];
        assert_eq!(restorable(&Session::default(), &known).count(), 0);
    }
}
