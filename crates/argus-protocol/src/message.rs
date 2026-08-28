use serde::{Deserialize, Serialize};

use crate::cell::{Cell, CellSpan, Cursor, MouseTracking};
use crate::ids::{CheckoutId, PaneId, ProjectId, RepositoryId, WorkspaceId};
use crate::review::{CommitFile, CommitInfo, Review, ReviewAnchor, ReviewBase};
use crate::tree::{ProjectInfo, WorkspaceInfo};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMsg {
    /// Ask the daemon to start streaming this pane's screen. The daemon
    /// replies with a full PaneSnapshot, then incremental Damage.
    Subscribe {
        pane: PaneId,
    },
    /// Stop streaming a pane's screen.
    Unsubscribe {
        pane: PaneId,
    },
    /// Raw input bytes to forward to the pane's pty.
    Input {
        pane: PaneId,
        bytes: Vec<u8>,
    },
    /// Text pasted as one event. The daemon adds bracketed-paste delimiters
    /// only when the child requested them.
    Paste {
        pane: PaneId,
        text: String,
    },
    /// The client's view of a pane has been resized.
    Resize {
        pane: PaneId,
        rows: u16,
        cols: u16,
    },
    /// Spawn a shell pane cwd'd into a checkout.
    SpawnShell {
        checkout: CheckoutId,
    },
    /// Spawn an agent pane from a named template, cwd'd into a checkout.
    SpawnAgent {
        checkout: CheckoutId,
        template: String,
    },
    /// Kill a pane's process and remove it.
    Kill {
        pane: PaneId,
    },
    /// `git worktree add` a new checkout in `base`'s project, branched off
    /// `base`'s current HEAD, and add it to the tree.
    CreateWorktree {
        checkout: CheckoutId,
        branch: String,
    },
    /// Kill every pane in a (non-primary) checkout, `git worktree remove`
    /// it, delete its branch, and drop it from the tree.
    RemoveCheckout {
        checkout: CheckoutId,
    },
    /// Switch which workspace is open. Daemon-global: every connected
    /// client's tree re-scopes to it.
    OpenWorkspace {
        workspace: WorkspaceId,
    },
    /// Declare a new, empty workspace and open it. Persisted to config, so
    /// grouping projects never requires hand-editing `projects.toml`.
    CreateWorkspace {
        name: String,
    },
    /// Ask for this checkout's uncommitted changes, for the review viewer
    /// (DESIGN.md §9 M4). A request rather than a subscription: a diff is
    /// expensive to compute and only interesting while it's on screen.
    Review {
        request_id: u64,
        checkout: CheckoutId,
        base: ReviewBase,
        /// When set, the parent of this commit against the commit itself,
        /// ignoring `base` except as the flag [`ReviewBase::Commit`].
        #[serde(default)]
        commit: Option<String>,
    },
    /// Newest commits on this checkout's HEAD, identities only. What each
    /// one changed is [`ClientMsg::ListCommitFiles`], asked for one commit
    /// at a time.
    ListCommits {
        request_id: u64,
        checkout: CheckoutId,
    },
    /// The paths one commit touched, for a history row the viewer has just
    /// drilled into. Its own message because summarizing a commit means
    /// diffing it against its parent: affordable once, ruinous a hundred
    /// times over while the overlay is still opening.
    ListCommitFiles {
        checkout: CheckoutId,
        commit: String,
    },
    /// Persist a review comment, then notify the selected live agent.
    ReviewComment {
        checkout: CheckoutId,
        recipient: PaneId,
        anchor: Box<ReviewAnchor>,
        body: String,
    },
    /// Ask for what this checkout contains, for the fuzzy pickers.
    ListBranches {
        checkout: CheckoutId,
    },
    ListFiles {
        checkout: CheckoutId,
    },
    /// `git switch` this checkout to an existing branch.
    SwitchBranch {
        checkout: CheckoutId,
        branch: String,
    },
    /// `git switch -c`: a new branch on this checkout, in place. Distinct
    /// from `CreateWorktree`, which puts the new branch in a directory of
    /// its own and leaves this one where it was.
    CreateBranch {
        checkout: CheckoutId,
        branch: String,
    },
    /// `git branch -d`: drop a local branch nothing is sitting on. Refused
    /// while it holds commits no other branch has, because the row is the
    /// only thing left pointing at them — and answered with
    /// `BranchNotMerged`, which is what `force` comes back as. Forced, it
    /// is `git branch -D` and those commits stop being reachable.
    DeleteBranch {
        checkout: CheckoutId,
        branch: String,
        force: bool,
    },
    /// `git fetch --all --prune`: bring the remote-tracking branches up to
    /// date without touching the working tree, which is what makes the
    /// remote's branches visible as rows.
    Fetch {
        checkout: CheckoutId,
    },
    /// `git pull --ff-only`: move this checkout up to its upstream. Refused
    /// by git itself where that would need a merge.
    Pull {
        checkout: CheckoutId,
    },
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
    /// Drop a project from the panel and from `projects.toml`. Nothing on
    /// disk is touched — the directories stay exactly where they are, and
    /// adding the project again brings the same tree back.
    RemoveProject {
        project: ProjectId,
    },
    /// Drop one repository from its project's panel row. The scan that
    /// found it would otherwise put it straight back, so the path is
    /// remembered as excluded until the project is removed or the
    /// exclusion file is edited.
    RemoveRepository {
        repository: RepositoryId,
    },
    /// Add a new project rooted at an arbitrary directory — not limited to
    /// whatever's already in `projects.toml` or under the daemon's cwd.
    /// Persisted to config so it survives a daemon restart.
    AddProject {
        path: String,
    },
    /// List the subdirectories of `path`, for the directory browser
    /// behind "add project" and "add repository". An empty path means
    /// "wherever a browse should start" — the daemon decides, since only
    /// it knows its own cwd. `request_id` correlates the reply: a browse
    /// walks the filesystem, and a slow listing must not land in a
    /// directory the user has already navigated away from.
    ListDirectories {
        request_id: u64,
        path: String,
    },
    /// Add one repository to a project that already exists, by path. For a
    /// directory the project's root would never scan — anything under the
    /// root arrives on its own — so the path is written into the project's
    /// `repos` list and taken at its word, Git repository or not.
    AddRepository {
        project: ProjectId,
        path: String,
    },
    /// Make a repository that does not exist yet: create `path` if it is
    /// not there, `git init` it, and add it to `project` the way
    /// [`ClientMsg::AddRepository`] would. The one gesture in Argus that
    /// creates a repository rather than finding one — everything else
    /// takes the checkouts on disk as given.
    InitRepository {
        project: ProjectId,
        path: String,
    },
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
        /// What mouse reporting the child has asked for. Defaulted on the
        /// wire so an older daemon reads as "none", which is the safe
        /// answer: no mouse bytes get forwarded.
        #[serde(default)]
        mouse: MouseTracking,
        /// Whether the child is on the alternate screen. A wheel over that
        /// pane becomes a cursor key when mouse reporting is off — which is
        /// how Claude, Codex, and Cursor Agent scroll. Defaulted off so an
        /// older daemon never injects arrows into a shell.
        #[serde(default)]
        alternate_screen: bool,
    },
    /// Incremental changed spans since the last snapshot/damage for a pane.
    Damage {
        pane: PaneId,
        spans: Vec<CellSpan>,
        cursor: Cursor,
        #[serde(default)]
        mouse: MouseTracking,
        #[serde(default)]
        alternate_screen: bool,
    },
    /// The answer to `ClientMsg::Review`.
    Review(Review),
    /// A failed review capture/diff, correlated so stale failures are dropped.
    ReviewFailed {
        request_id: u64,
        checkout: CheckoutId,
        message: String,
    },
    /// The durable write succeeded. Delivery only describes the immediate
    /// terminal notification; an undelivered comment remains readable.
    ReviewCommentSaved {
        id: u64,
        delivered: bool,
    },
    /// The answer to `ClientMsg::ListBranches`. `current` is the branch the
    /// checkout is on, and is the first entry of `branches`.
    Branches {
        checkout: CheckoutId,
        branches: Vec<String>,
    },
    /// The answer to `ClientMsg::ListDirectories`.
    Directories(DirListing),
    /// The answer to `ClientMsg::ListFiles`, repo-relative.
    Files {
        checkout: CheckoutId,
        files: Vec<String>,
    },
    /// The answer to `ClientMsg::ListCommits`.
    Commits {
        request_id: u64,
        checkout: CheckoutId,
        commits: Vec<CommitInfo>,
    },
    /// The answer to `ClientMsg::ListCommitFiles`. Correlated by `commit`
    /// rather than a request id: the oid already names the row these files
    /// belong to, and a second answer for it is the same answer.
    CommitFiles {
        checkout: CheckoutId,
        commit: String,
        files: Vec<CommitFile>,
    },
    /// A failed summary of one commit, correlated like `CommitFiles`.
    CommitFilesFailed {
        checkout: CheckoutId,
        commit: String,
        message: String,
    },
    /// A failed history walk. Correlated like `ReviewFailed` rather than
    /// folded into `Error`, so a client that has moved on drops it instead
    /// of showing an alert for a list it no longer wants.
    CommitsFailed {
        request_id: u64,
        checkout: CheckoutId,
        message: String,
    },
    /// A pane's process exited.
    PaneClosed {
        pane: PaneId,
        code: Option<i32>,
    },
    /// `git branch -d` was refused because the branch holds commits no
    /// other branch does. Correlated rather than folded into `Error` so
    /// the client can offer the forced deletion, which is the only thing
    /// the user can do about it, instead of only reciting git's refusal.
    BranchNotMerged {
        checkout: CheckoutId,
        branch: String,
    },
    Error {
        message: String,
    },
}

/// One directory's subdirectories, as the browser needs to draw them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirListing {
    pub request_id: u64,
    /// The directory that was listed, absolute and canonicalized — what
    /// the client shows as the breadcrumb and what it sends back when the
    /// user picks it.
    pub path: String,
    /// The directory above it, absent at a filesystem root.
    pub parent: Option<String>,
    pub entries: Vec<DirEntry>,
    /// Why the listing is empty, when it is empty for a reason worth
    /// saying: a directory that has gone away, or one this user may not
    /// read. Navigating into a dead end should say so rather than look
    /// like an empty directory.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirEntry {
    /// The last segment only. The browser draws it under the breadcrumb,
    /// and fuzzy-matches it, so the parent path would only be noise.
    pub name: String,
    /// Whether it is a Git repository — the thing the user is usually
    /// hunting for, and the difference between a project root and a
    /// repository inside one.
    pub is_repo: bool,
}
