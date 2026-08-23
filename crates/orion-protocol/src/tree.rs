use serde::{Deserialize, Serialize};

use crate::ids::{CheckoutId, PaneId, ProjectId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaneKind {
    Shell,
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaneStatus {
    Running,
    Exited { code: Option<i32> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneInfo {
    pub id: PaneId,
    pub kind: PaneKind,
    pub title: String,
    pub status: PaneStatus,
}

/// Read-only git status for a checkout, polled from the working directory
/// (see DESIGN.md §4 Level 2). `None` on `CheckoutInfo::git` means the path
/// isn't a git repo at all, not that status is merely unknown yet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitStatus {
    /// `None` for a detached HEAD.
    pub branch: Option<String>,
    pub dirty: bool,
    pub changed_files: usize,
    /// Commits ahead/behind the upstream tracking branch; both 0 if there is
    /// no upstream configured.
    pub ahead: usize,
    pub behind: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckoutInfo {
    pub id: CheckoutId,
    pub name: String,
    pub path: String,
    pub panes: Vec<PaneInfo>,
    pub git: Option<GitStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub id: ProjectId,
    pub name: String,
    pub checkouts: Vec<CheckoutInfo>,
}
