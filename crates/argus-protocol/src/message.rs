use serde::{Deserialize, Serialize};

use crate::cell::{Cell, CellSpan, Cursor};
use crate::ids::{CheckoutId, PaneId, WorkspaceId};
use crate::review::{Review, ReviewBase};
use crate::tree::{ProjectInfo, WorkspaceInfo};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMsg {
    /// Ask the daemon to start streaming this pane's screen. The daemon
    /// replies with a full PaneSnapshot, then incremental Damage.
    Subscribe { pane: PaneId },
    /// Stop streaming a pane's screen.
    Unsubscribe { pane: PaneId },
    /// Raw input bytes to forward to the pane's pty.
    Input { pane: PaneId, bytes: Vec<u8> },
    /// The client's view of a pane has been resized.
    Resize { pane: PaneId, rows: u16, cols: u16 },
    /// Spawn a shell pane cwd'd into a checkout.
    SpawnShell { checkout: CheckoutId },
    /// Spawn an agent pane from a named template, cwd'd into a checkout.
    SpawnAgent { checkout: CheckoutId, template: String },
    /// Kill a pane's process and remove it.
    Kill { pane: PaneId },
    /// `git worktree add` a new checkout in `base`'s project, branched off
    /// `base`'s current HEAD, and add it to the tree.
    CreateWorktree { checkout: CheckoutId, branch: String },
    /// Kill every pane in a (non-primary) checkout, `git worktree remove`
    /// it, delete its branch, and drop it from the tree.
    RemoveCheckout { checkout: CheckoutId },
    /// Switch which workspace is open. Daemon-global: every connected
    /// client's tree re-scopes to it.
    OpenWorkspace { workspace: WorkspaceId },
    /// Ask for this checkout's uncommitted changes, for the review viewer
    /// (DESIGN.md §9 M4). A request rather than a subscription: a diff is
    /// expensive to compute and only interesting while it's on screen.
    Review {
        request_id: u64,
        checkout: CheckoutId,
        base: ReviewBase,
    },
    /// Accept exactly the review endpoint currently displayed by the client.
    AcknowledgeReview {
        checkout: CheckoutId,
        target_snapshot: String,
        expected_baseline: Option<String>,
    },
    /// Ask for what this checkout contains, for the fuzzy pickers.
    ListBranches { checkout: CheckoutId },
    ListFiles { checkout: CheckoutId },
    /// `git switch` this checkout to an existing branch.
    SwitchBranch { checkout: CheckoutId, branch: String },
    /// `git switch -c`: a new branch on this checkout, in place. Distinct
    /// from `CreateWorktree`, which puts the new branch in a directory of
    /// its own and leaves this one where it was.
    CreateBranch { checkout: CheckoutId, branch: String },
    /// Open `path` (repo-relative) in the user's editor as a pane.
    OpenInEditor {
        checkout: CheckoutId,
        path: String,
        line: Option<u32>,
        /// Launch it outside Argus with no pty, for an editor that brings
        /// its own window.
        external: bool,
        /// The editor to run, flags included. `None` leaves the daemon to
        /// work it out from the environment.
        command: Option<String>,
    },
    /// Add a new project rooted at an arbitrary directory — not limited to
    /// whatever's already in `projects.toml` or under the daemon's cwd.
    /// Persisted to config so it survives a daemon restart.
    AddProject { path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMsg {
    /// Full project/checkout/pane tree, sent on connect and after any change.
    Tree(Vec<ProjectInfo>),
    /// Names of the configured agent templates, sent once on connect.
    Templates(Vec<String>),
    /// Every workspace, with which one is open. Sent on connect and after
    /// any switch.
    Workspaces(Vec<WorkspaceInfo>),
    /// Full-grid snapshot of a pane, sent once right after Subscribe.
    PaneSnapshot {
        pane: PaneId,
        rows: u16,
        cols: u16,
        cells: Vec<Vec<Cell>>,
        cursor: Cursor,
    },
    /// Incremental changed spans since the last snapshot/damage for a pane.
    Damage {
        pane: PaneId,
        spans: Vec<CellSpan>,
        cursor: Cursor,
    },
    /// The answer to `ClientMsg::Review`.
    Review(Review),
    /// A failed review capture/diff, correlated so stale failures are dropped.
    ReviewFailed {
        request_id: u64,
        checkout: CheckoutId,
        message: String,
    },
    ReviewAcknowledged {
        checkout: CheckoutId,
        target_snapshot: String,
    },
    ReviewAcknowledgeFailed {
        checkout: CheckoutId,
        target_snapshot: String,
        message: String,
    },
    /// The answer to `ClientMsg::ListBranches`. `current` is the branch the
    /// checkout is on, and is the first entry of `branches`.
    Branches {
        checkout: CheckoutId,
        branches: Vec<String>,
    },
    /// The answer to `ClientMsg::ListFiles`, repo-relative.
    Files {
        checkout: CheckoutId,
        files: Vec<String>,
    },
    /// A pane's process exited.
    PaneClosed { pane: PaneId, code: Option<i32> },
    Error { message: String },
}
