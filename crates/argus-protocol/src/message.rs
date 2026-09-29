//! The two message enums: what a client asks for, and what the daemon
//! sends back. Every field is documented with what it means rather than
//! what it is, because this is the contract two binaries are built
//! against.

use serde::{Deserialize, Serialize};

use crate::cell::{Cell, CellSpan, Cursor, MouseTracking};
use crate::damage::{CellRun, Scroll};
use crate::decisions::DecisionBoard;
use crate::features::{FeatureState, FeatureWrite};
use crate::hello::Hello;
use crate::ids::{CheckoutId, PaneId, ProjectId, RepositoryId, WorkspaceId};
use crate::review::{CommitFile, CommitInfo, Review, ReviewAnchor, ReviewBase};
use crate::diagrams::{DiagramAction, DiagramList};
use crate::tasks::{TaskAction, TaskList};
use crate::transcript::{Earlier, Update};
use crate::tree::{AgentTelemetry, ProjectInfo, WorkspaceInfo, WorkspaceTree};

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
    /// Ask for the rows sitting `offset` lines above a pane's live screen.
    /// `0` is the live screen itself, which is how the client says it has
    /// scrolled back to the bottom.
    ///
    /// `top` asks by line instead: the number a `ScrollbackRuns` gave the
    /// first row of a view, which a line keeps while more output pushes it
    /// back, so a parked view scrolled further moves from where it was
    /// rather than from wherever the live screen has got to. A daemon that
    /// numbers no lines reads the offset.
    Scrollback {
        pane: PaneId,
        offset: u32,
        #[serde(default)]
        top: Option<u64>,
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
        /// Names the request, so the daemon can answer with what it made
        /// ([`ServerMsg::Created`]). Zero, the default, asks for no answer:
        /// what a client that predates it sends.
        #[serde(default)]
        request_id: u64,
    },
    /// Spawn an agent pane from a named template, cwd'd into a checkout.
    SpawnAgent {
        checkout: CheckoutId,
        template: String,
        /// Names the request, so the daemon can answer with what it made
        /// ([`ServerMsg::Created`]). Zero, the default, asks for no answer:
        /// what a client that predates it sends.
        #[serde(default)]
        request_id: u64,
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
        /// Names the request, so the daemon can answer with what it made
        /// ([`ServerMsg::Created`]). Zero, the default, asks for no answer:
        /// what a client that predates it sends.
        #[serde(default)]
        request_id: u64,
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
    /// Read a project's decision board. Whole rather than scoped: a
    /// decision hanging off three others means nothing without them.
    GetDecisions {
        project: ProjectId,
        checkout: CheckoutId,
    },
    /// Accept a feature, or reopen one that was accepted.
    ///
    /// The only state there is: everything the old columns claimed is now
    /// read off the panes on the feature's checkouts and the state of its
    /// tasks. An agent makes the same move only on a person's word
    /// (`FeatureAction::Done`). The answer is the pushed board every client
    /// already receives.
    MoveFeature {
        project: ProjectId,
        checkout: CheckoutId,
        slug: String,
        state: FeatureState,
        detail: Option<String>,
    },
    /// Hold a feature with a reason, or lift its hold with `None`.
    HoldFeature {
        project: ProjectId,
        checkout: CheckoutId,
        slug: String,
        reason: Option<String>,
    },
    /// Record that a feature comes after another, or take that back.
    WaitFeature {
        project: ProjectId,
        checkout: CheckoutId,
        slug: String,
        on: String,
        waits: bool,
    },
    /// Open a feature from the board.
    ///
    /// The human's equivalent of `argus-hook feature open`, and the only
    /// difference is that it records no origin checkout: a feature a
    /// person wrote down was not cut anywhere yet.
    OpenFeature {
        project: ProjectId,
        checkout: CheckoutId,
        write: FeatureWrite,
    },
    /// Rename a feature. The title only — the slug is frozen at creation,
    /// because every decision, task and checkout scope points at it.
    RenameFeature {
        project: ProjectId,
        checkout: CheckoutId,
        slug: String,
        title: String,
    },
    /// Remove a feature, its tasks, and any checkout pointed at it.
    ///
    /// Its decisions survive as unfiled: the board is append-only because
    /// it records what was believed at the time, and that outlives the
    /// feature it was believed about.
    RemoveFeature {
        project: ProjectId,
        checkout: CheckoutId,
        slug: String,
    },
    /// Move a feature's current assignment between checkouts in one repository.
    TransferFeature {
        project: ProjectId,
        source: CheckoutId,
        destination: CheckoutId,
        slug: String,
    },
    /// Replace a feature's brief.
    ///
    /// Whole-body, unlike `argus-hook feature note`, which appends. The
    /// append rule protected a tree structure the brief does not have: it
    /// is prose, and a document nobody can correct rots.
    SetFeatureBody {
        project: ProjectId,
        checkout: CheckoutId,
        slug: String,
        body: String,
    },
    /// Read or change one feature's tasks, in the same words an agent
    /// uses. Scoped to the feature, unlike the decision board: a task means
    /// nothing outside the feature it is under, so there is no
    /// whole-project list to want. A read is answered with
    /// `ServerMsg::Tasks`; a change reaches every client as a push instead.
    Task {
        project: ProjectId,
        checkout: CheckoutId,
        feature: String,
        action: TaskAction,
    },
    /// Read or change one feature's sequence diagrams. A read is answered
    /// with [`ServerMsg::SequenceDiagrams`]; a write is pushed like tasks.
    SequenceDiagram {
        project: ProjectId,
        checkout: CheckoutId,
        feature: String,
        action: DiagramAction,
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
        /// Names the request, so the daemon can answer with what it made
        /// ([`ServerMsg::Created`]). Zero, the default, asks for no answer:
        /// what a client that predates it sends.
        #[serde(default)]
        request_id: u64,
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
        /// Names the request, so the daemon can answer with what it made
        /// ([`ServerMsg::Created`]). Zero, the default, asks for no answer:
        /// what a client that predates it sends.
        #[serde(default)]
        request_id: u64,
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
        /// Names the request, so the daemon can answer with what it made
        /// ([`ServerMsg::Created`]). Zero, the default, asks for no answer:
        /// what a client that predates it sends.
        #[serde(default)]
        request_id: u64,
    },
    /// Make a repository that does not exist yet: create `path` if it is
    /// not there, `git init` it, and add it to `project` the way
    /// [`ClientMsg::AddRepository`] would. The one gesture in Argus that
    /// creates a repository rather than finding one — everything else
    /// takes the checkouts on disk as given. `path` must be absolute so a
    /// daemon working directory can never decide where files are created.
    InitRepository {
        project: ProjectId,
        path: String,
        /// Names the request, so the daemon can answer with what it made
        /// ([`ServerMsg::Created`]). Zero, the default, asks for no answer:
        /// what a client that predates it sends.
        #[serde(default)]
        request_id: u64,
    },
    /// Ask the daemon to flush this connection, stop accepting clients, and
    /// exit so a fresh daemon can restore the persisted session.
    Restart,
    /// Ask the daemon to stop cleanly without starting a replacement.
    Stop,
    /// Start following a pane's conversation. Answered with a fresh
    /// [`ServerMsg::Transcript`], then updates. Only to a daemon that listed
    /// `TRANSCRIPTS`.
    WatchTranscript {
        pane: PaneId,
    },
    /// Stop following a pane's conversation.
    UnwatchTranscript {
        pane: PaneId,
    },
    /// Ask for the part of a conversation before `before`, which a
    /// [`ServerMsg::Transcript`] or an earlier answer handed out.
    EarlierTranscript {
        pane: PaneId,
        before: Earlier,
    },
    /// Say `text` to an agent: typed as one paste and Enter once the agent
    /// is idle, or straight away with `now`, which is how a person steers
    /// one mid-turn. Answered with [`ServerMsg::Sent`] to this client. Only
    /// to a daemon that listed `OUTBOX`.
    SendToAgent {
        pane: PaneId,
        text: String,
        now: bool,
        request_id: u64,
    },
    /// Take back a message still waiting to be typed.
    CancelQueued {
        pane: PaneId,
        id: u64,
    },
    /// Interrupt what the agent is doing, with its harness's own key.
    Interrupt {
        pane: PaneId,
    },
    /// Answer a question the agent's harness posed in its transcript
    /// (`Body::Question`), by the question's entry id and a choice's id.
    Answer {
        pane: PaneId,
        question: String,
        choice: String,
    },
    /// This client's greeting: the first message it sends, and the only one
    /// it sends before knowing the daemon can take more than the floor. A
    /// daemon from before the handshake hangs up on it, which is how the
    /// client finds out. See `hello`.
    Hello(Hello),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMsg {
    /// The daemon's answer to a client's greeting, sent before anything
    /// else — or, when the greeting arrived after the daemon stopped
    /// waiting for one, as soon as it does. Never sent to a client that did
    /// not greet, which could not read it.
    Hello(Hello),
    /// Full project/checkout/pane tree, sent on connect and after any change.
    Tree(Vec<ProjectInfo>),
    /// Every workspace's tree, in place of `Tree` after the greeting, to a
    /// client that listed `WIDE_TREE`.
    WideTree(Vec<WorkspaceTree>),
    /// One agent's telemetry, whole, after it reported. Only to a client
    /// that listed `PANE_TELEMETRY`; any other is sent the whole tree,
    /// which carries the same record.
    PaneTelemetry {
        pane: PaneId,
        telemetry: AgentTelemetry,
    },
    /// Updates to a watched pane's conversation.
    ///
    /// `fresh` says to drop whatever is held for the pane first: the answer
    /// to a watch, and what a client that fell behind is sent instead of the
    /// updates it missed. `earlier` is where to ask for what came before;
    /// `None` means the start of the conversation is held.
    Transcript {
        pane: PaneId,
        fresh: bool,
        earlier: Option<Earlier>,
        updates: Vec<Update>,
    },
    /// The answer to [`ClientMsg::EarlierTranscript`]: what came before
    /// `before`, oldest first, to go ahead of what is held.
    EarlierTranscript {
        pane: PaneId,
        before: Earlier,
        earlier: Option<Earlier>,
        updates: Vec<Update>,
    },
    /// What became of a [`ClientMsg::SendToAgent`], to the client that
    /// sent it.
    Sent {
        request_id: u64,
        pane: PaneId,
        sent: Sent,
    },
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
    /// `PaneSnapshot` for a client that greeted with `CELL_RUNS`: the grid
    /// as runs, every cell not in one a default blank.
    PaneRows {
        pane: PaneId,
        rows: u16,
        cols: u16,
        runs: Vec<CellRun>,
        cursor: Cursor,
        mouse: MouseTracking,
        alternate_screen: bool,
    },
    /// `Damage` for a client that greeted with `CELL_RUNS`: the region that
    /// scrolled, applied first, then the runs that changed after it.
    RowDamage {
        pane: PaneId,
        scroll: Option<Scroll>,
        runs: Vec<CellRun>,
        cursor: Cursor,
        mouse: MouseTracking,
        alternate_screen: bool,
    },
    /// Text a pane's child asked to put on the clipboard with OSC 52. The
    /// pane's terminal is the daemon's, so the request has to be carried to
    /// the client, whose terminal and desktop are the user's.
    Clipboard {
        pane: PaneId,
        text: String,
    },
    /// The answer to `ClientMsg::Scrollback`. `offset` is what the daemon
    /// could actually reach after clamping and `depth` how far back the
    /// buffer goes, so the client can stop at the top rather than asking
    /// for rows that do not exist.
    ScrollbackRows {
        pane: PaneId,
        offset: u32,
        depth: u32,
        cells: Vec<Vec<Cell>>,
    },
    /// `ScrollbackRows` for a client that greeted with `CELL_RUNS`: the
    /// rows as runs, every cell not in one a default blank.
    ScrollbackRuns {
        pane: PaneId,
        offset: u32,
        depth: u32,
        rows: u16,
        cols: u16,
        runs: Vec<CellRun>,
        /// The number of the first row, counting every line that has ever
        /// gone up past the live screen: what `ClientMsg::Scrollback` takes
        /// as `top`. Absent from a daemon that numbers no lines.
        #[serde(default)]
        top: Option<u64>,
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
    /// One feature's tasks: the answer to a `TaskAction::List`, and what
    /// every client receives whenever that list changes.
    Tasks(Box<TaskList>),
    /// One feature's sequence diagrams: the answer to a list, and what
    /// every client receives when that list changes.
    SequenceDiagrams(Box<DiagramList>),
    /// The answer to `ClientMsg::GetDecisions`, and what every client
    /// receives when a board changes — a decision tree is meant to be
    /// watched being built, not polled.
    Decisions(Box<DecisionBoard>),
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
    /// What a request that named itself made: a pane, a checkout, a
    /// project or a repository — or `None` when it was refused, whose
    /// reason arrives as an [`ServerMsg::Error`] as it always has.
    ///
    /// Answered only to the client that asked, and only when it gave a
    /// non-zero `request_id`, so a client that predates this never receives
    /// a message it cannot read. A client matches what was made by this id
    /// rather than by where a new row appears in the next tree: the tree is
    /// broadcast for everything, and the first to arrive need not be the
    /// one carrying the row.
    Created {
        request_id: u64,
        created: Option<Created>,
    },
    /// The daemon accepted a restart request and will exit after this frame
    /// reaches the client.
    Restarting,
    /// The daemon accepted a stop request and will exit after this frame
    /// reaches the client.
    Stopping,
}

/// What a creating request made.
/// The most one message to an agent may hold. A prompt, not a document:
/// a document belongs in a file the agent is pointed at.
pub const MAX_SEND_BYTES: usize = 16 * 1024;

/// What became of a message to an agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sent {
    /// Typed into the agent's terminal.
    Typed,
    /// Waiting for the agent to be idle, under this id.
    Queued { id: u64 },
    /// Not taken, and why.
    Refused { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Created {
    Pane(PaneId),
    Checkout(CheckoutId),
    Project(ProjectId),
    Repository(RepositoryId),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn wire<T: Serialize>(msg: &T) -> Vec<u8> {
        rmp_serde::to_vec_named(msg).unwrap()
    }

    /// `SpawnShell` as a client built before requests were named sent it,
    /// and `Scrollback` as one built before lines were numbered.
    #[derive(Serialize, Deserialize)]
    enum OlderClientMsg {
        SpawnShell { checkout: CheckoutId },
        Scrollback { pane: PaneId, offset: u32 },
    }

    #[test]
    fn a_scrollback_request_by_offset_alone_still_reads() {
        let older = wire(&OlderClientMsg::Scrollback {
            pane: PaneId(3),
            offset: 12,
        });
        let read: ClientMsg = rmp_serde::from_slice(&older).unwrap();
        assert!(matches!(
            read,
            ClientMsg::Scrollback {
                pane: PaneId(3),
                offset: 12,
                top: None
            }
        ));
    }

    #[test]
    fn an_older_daemon_reads_a_request_by_line_as_one_by_offset() {
        let newer = wire(&ClientMsg::Scrollback {
            pane: PaneId(3),
            offset: 12,
            top: Some(4000),
        });
        let read: OlderClientMsg = rmp_serde::from_slice(&newer).unwrap();
        assert!(matches!(
            read,
            OlderClientMsg::Scrollback {
                pane: PaneId(3),
                offset: 12
            }
        ));
    }

    #[test]
    fn a_request_from_an_older_client_asks_for_no_answer() {
        let older = wire(&OlderClientMsg::SpawnShell {
            checkout: CheckoutId(4),
        });
        let read: ClientMsg = rmp_serde::from_slice(&older).unwrap();
        assert!(matches!(
            read,
            ClientMsg::SpawnShell {
                checkout: CheckoutId(4),
                request_id: 0
            }
        ));
    }

    #[test]
    fn an_older_daemon_reads_a_named_request_and_ignores_the_name() {
        let newer = wire(&ClientMsg::SpawnShell {
            checkout: CheckoutId(4),
            request_id: 9,
        });
        let read: OlderClientMsg = rmp_serde::from_slice(&newer).unwrap();
        assert!(matches!(
            read,
            OlderClientMsg::SpawnShell {
                checkout: CheckoutId(4)
            }
        ));
    }

    #[test]
    fn what_was_made_survives_the_wire() {
        for created in [
            Some(Created::Pane(PaneId(1))),
            Some(Created::Checkout(CheckoutId(2))),
            Some(Created::Project(ProjectId(3))),
            Some(Created::Repository(RepositoryId(4))),
            None,
        ] {
            let bytes = wire(&ServerMsg::Created {
                request_id: 7,
                created,
            });
            let read: ServerMsg = rmp_serde::from_slice(&bytes).unwrap();
            assert!(
                matches!(read, ServerMsg::Created { request_id: 7, created: c } if c == created),
                "{created:?}"
            );
        }
    }
}
