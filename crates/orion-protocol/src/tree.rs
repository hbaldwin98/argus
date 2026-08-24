use serde::{Deserialize, Serialize};

use crate::ids::{CheckoutId, PaneId, ProjectId, WorkspaceId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaneKind {
    Shell,
    Agent,
    /// The user's own editor, opened on a file from the review (§6).
    Editor,
}

/// See DESIGN.md §8b. `Idle`/`Working`/`Waiting` come from agent hooks where
/// supported (§11); templates with no hook support just sit at `Idle` until
/// they `Exited` — coarse, but that's the accepted fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaneStatus {
    Idle,
    Working,
    Waiting,
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
    /// True for a repo's original working directory (as configured), false
    /// for a linked worktree Orion created. The primary checkout can't be
    /// removed — see DESIGN.md §4 Level 2.
    pub primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub id: ProjectId,
    pub name: String,
    pub checkouts: Vec<CheckoutInfo>,
}

/// A named group of projects. Exactly one workspace is *open* at a time —
/// daemon-global state, broadcast to every client — and the project tree a
/// client sees is scoped to it. Other workspaces' panes keep running in the
/// background; they are simply not shown.
///
/// Deliberately not a fourth navigation column: the left-to-right spine is
/// project → checkout → pane (DESIGN.md §4), and workspaces sit *above*
/// that as a scope switch rather than another step along it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub id: WorkspaceId,
    pub name: String,
    /// How many projects this workspace holds, so the picker can show it
    /// without the client having to hold every workspace's tree.
    pub projects: usize,
    /// Live panes across the whole workspace, open or not — the reason to
    /// show this at all is spotting an agent still working somewhere you
    /// are not currently looking.
    pub panes: usize,
    pub open: bool,
}
