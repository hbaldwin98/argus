//! Reading the files the SQLite store replaced, once, on the first run
//! that finds them.
//!
//! The `serde(default)` shims live here rather than on the live types: the
//! store's columns are nullable, so the current code has no legacy shape to
//! describe, and every default below exists only to read a file written by
//! a version that predates one field.

use std::path::{Path, PathBuf};

use anyhow::Result;
use argus_protocol::{PaneKind, PaneStatus};
use serde::Deserialize;

use super::{SessionPane, Store};

impl Store {

    /// Folds the files this store replaced into it, once, on the first run
    /// that finds them. Each is renamed rather than deleted: an import that
    /// gets something wrong should be recoverable by hand, and a rename is
    /// also what stops the next run from importing it again.
    ///
    /// Runtime-added `[[project]]` blocks are deliberately not imported.
    /// They are already in `projects.toml`, they still load from there, and
    /// pulling them into the store would show every one of them twice.
    pub(super) fn import_legacy_files(&self, dir: &Path) -> Result<()> {
        self.import_session_json(&dir.join("session.json"))?;
        self.import_excluded_repos(&dir.join("excluded-repos"))?;
        self.import_open_workspace(&dir.join("open-workspace"))?;
        Ok(())
    }

    pub(super) fn import_session_json(&self, path: &Path) -> Result<()> {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Ok(());
        };
        match parse_session(&raw) {
            Ok(panes) => {
                self.save_panes(&panes)?;
                tracing::info!("imported {} panes from {}", panes.len(), path.display());
                retire(path);
            }
            Err(e) => {
                // Leave it in place. A file we could not read is one a later
                // version might, and overwriting it costs the user their
                // workspace for nothing.
                tracing::warn!("could not import {}: {e}", path.display());
            }
        }
        Ok(())
    }

    pub(super) fn import_excluded_repos(&self, path: &Path) -> Result<()> {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Ok(());
        };
        let paths: Vec<PathBuf> = raw
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(PathBuf::from)
            .collect();
        for p in &paths {
            self.exclude_repo(p)?;
        }
        tracing::info!(
            "imported {} exclusions from {}",
            paths.len(),
            path.display()
        );
        retire(path);
        Ok(())
    }

    pub(super) fn import_open_workspace(&self, path: &Path) -> Result<()> {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Ok(());
        };
        let name = raw.trim();
        if !name.is_empty() {
            self.set_open_workspace(name)?;
        }
        retire(path);
        Ok(())
    }
}

/// Moves an imported file aside. Best-effort: failing to rename it only
/// means the next start imports it again over identical rows.
fn retire(path: &Path) {
    let to = path.with_extension("imported");
    if let Err(e) = std::fs::rename(path, &to) {
        tracing::warn!("could not move {} aside after import: {e}", path.display());
    }
}

#[derive(Deserialize)]
struct Session {
    #[serde(default)]
    panes: Vec<Pane>,
}

#[derive(Deserialize)]
struct Pane {
    checkout_path: PathBuf,
    kind: PaneKind,
    title: String,
    #[serde(default)]
    template: Option<String>,
    #[serde(default = "idle")]
    status: PaneStatus,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    harness_session_id: Option<String>,
    #[serde(default)]
    harness: Option<String>,
}

fn idle() -> PaneStatus {
    PaneStatus::Idle
}

pub(super) fn parse_session(raw: &str) -> Result<Vec<SessionPane>, serde_json::Error> {
    let s: Session = serde_json::from_str(raw)?;
    Ok(s.panes
        .into_iter()
        .map(|p| SessionPane {
            checkout_path: p.checkout_path,
            kind: p.kind,
            title: p.title,
            template: p.template,
            status: p.status,
            note: p.note,
            harness_session_id: p.harness_session_id,
            harness: p.harness,
        })
        .collect())
}
