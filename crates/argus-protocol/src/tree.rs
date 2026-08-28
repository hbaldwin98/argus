use serde::{Deserialize, Serialize};

use crate::ids::{CheckoutId, PaneId, ProjectId, RepositoryId, WorkspaceId};
use crate::notes::NoteCounts;

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
    Exited {
        code: Option<i32>,
    },
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

/// Another agent reporting through a pane it does not own — a CLI spawned
/// from inside the pane's own agent, which inherits the hook environment and
/// so would otherwise rewrite the row belonging to its parent. Listed under
/// the parent, and never allowed to change it: see DESIGN.md §8b.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildAgentInfo {
    /// What the child called itself, or a generic word until it says.
    pub label: String,
    pub status: PaneStatus,
    pub note: Option<String>,
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
    /// Agents running underneath this one, in the order they first
    /// reported. Read-only: the pane's own status and title stay the
    /// parent's to set.
    #[serde(default)]
    pub children: Vec<ChildAgentInfo>,
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
    /// What this checkout's own note holds, so a row can show what it owes
    /// without the client fetching a note per row.
    #[serde(default)]
    pub notes: NoteCounts,
    /// Whether a note exists at all. Distinct from an empty `notes`: a note
    /// of pure prose has nothing to count but is still there to open.
    #[serde(default)]
    pub has_note: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryInfo {
    pub id: RepositoryId,
    pub name: String,
    pub checkouts: Vec<CheckoutInfo>,
    /// Local branches no checkout of this repository is sitting on, sorted.
    /// The client decides which of them get rows — all of them only while
    /// the column is expanded, since a long branch list buries the
    /// checkouts that are the point of the column.
    #[serde(default)]
    pub branches: Vec<String>,
    /// The repository's main line of development, whatever it is named:
    /// `origin/HEAD` where the remote says, a conventional name where it
    /// doesn't. It leads the checkouts column whether or not anything is
    /// sitting on it, so that "how far is this from main" has a fixed place
    /// to be asked from.
    #[serde(default)]
    pub default_branch: Option<String>,
    /// Remote-tracking branches with no local branch of the same name, as
    /// `origin/feature`. What a fetch turns up: work that exists but isn't
    /// here yet, and can be had by switching to it or giving it a worktree.
    #[serde(default)]
    pub remote_branches: Vec<String>,
}

impl RepositoryInfo {
    /// A repository holds no note of its own — notes attach to projects and
    /// checkouts — so this is purely what its checkouts owe.
    pub fn note_rollup(&self) -> NoteCounts {
        self.checkouts.iter().map(|c| c.notes).sum()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub id: ProjectId,
    pub name: String,
    pub repositories: Vec<RepositoryInfo>,
    /// What this project's own note holds.
    #[serde(default)]
    pub notes: NoteCounts,
    #[serde(default)]
    pub has_note: bool,
}

impl ProjectInfo {
    /// The project's own note plus every checkout beneath it. What the
    /// projects column shows: one number for everything in there.
    pub fn note_rollup(&self) -> NoteCounts {
        self.repositories
            .iter()
            .map(RepositoryInfo::note_rollup)
            .sum::<NoteCounts>()
            + self.notes
    }
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
