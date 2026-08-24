use serde::{Deserialize, Serialize};

use crate::ids::{CheckoutId, PaneId, ProjectId, RepositoryId, WorkspaceId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaneKind {
    Shell,
    Agent,
    /// The user's own editor, opened on a file from the review (§6).
    Editor,
}

/// See DESIGN.md §8b. Everything but `Exited` comes from the agent itself,
/// through whatever hook mechanism its harness supports (§11); a harness
/// that reports nothing sits at `Idle` until it `Exited` — coarse, but
/// that's the accepted fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaneStatus {
    Idle,
    Working,
    /// Stopped, needing a human. [`PaneInfo::note`] says what for.
    Waiting,
    /// Work is ready for the operator to inspect.
    NeedsReview,
    /// Work is finished and has been reviewed.
    Done,
    /// Still running, but something went wrong and the agent said so.
    /// Distinct from `Exited`: the process is alive, so the row is worth
    /// going to rather than worth closing.
    Failed,
    Exited { code: Option<i32> },
}

impl PaneStatus {
    /// Whether this row is stalled on a human. What the eye should land on
    /// first when scanning a column of agents.
    pub fn needs_you(self) -> bool {
        matches!(
            self,
            PaneStatus::Waiting | PaneStatus::NeedsReview | PaneStatus::Failed
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneInfo {
    pub id: PaneId,
    pub kind: PaneKind,
    pub title: String,
    pub status: PaneStatus,
    /// One line from the agent about its current state — the question it is
    /// blocked on, or what failed. The point of a status column is knowing
    /// whether to go somewhere; the point of this is knowing why, without
    /// having to.
    #[serde(default)]
    pub note: Option<String>,
    /// The agent template this pane runs, for an agent pane. `title` starts
    /// as this and then becomes whatever the agent renames itself to, so
    /// without carrying it separately a renamed row stops saying which CLI
    /// is in it — which is exactly what you want to know when several are
    /// running side by side.
    #[serde(default)]
    pub template: Option<String>,
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

impl CheckoutInfo {
    /// The panes the tree lists. An editor belongs to the window it opened
    /// in, not to the checkout's pane list — it is a way of looking at a
    /// file, not something running here that you might come back to.
    pub fn listed_panes(&self) -> impl Iterator<Item = &PaneInfo> {
        self.panes.iter().filter(|p| p.kind != PaneKind::Editor)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckoutInfo {
    pub id: CheckoutId,
    pub name: String,
    pub path: String,
    pub panes: Vec<PaneInfo>,
    pub git: Option<GitStatus>,
    /// True for a repo's original working directory (as configured), false
    /// for a linked worktree Argus created. The primary checkout can't be
    /// removed — see DESIGN.md §4 Level 2.
    pub primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryInfo {
    pub id: RepositoryId,
    pub name: String,
    pub checkouts: Vec<CheckoutInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub id: ProjectId,
    pub name: String,
    pub repositories: Vec<RepositoryInfo>,
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
