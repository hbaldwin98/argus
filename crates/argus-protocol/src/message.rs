//! The two message enums: what a client asks for, and what the daemon
//! sends back. Every field is documented with what it means rather than
//! what it is, because this is the contract two binaries are built
//! against.

use serde::{Deserialize, Serialize};

use crate::views::agent_view::ShownView;
use crate::panes::cell::{Cell, CellSpan, Cursor, MouseTracking};
use crate::code::{CodeFile, CodeMatches, CodeTree, CodeVersion, SearchOptions};
use crate::panes::damage::{CellRun, Scroll};
use crate::decisions::{DecisionBoard, DecisionChange, DecisionWrite};
use crate::extensions::{ExtAnswer, ExtCommand, ExtDescription, ExtDialog, ExtView};
use crate::features::FeatureAction;
use crate::tree::focus::{Focus, View};
use crate::hello::Hello;
use crate::panes::keys::KeyboardMode;
use crate::tree::ids::{CheckoutId, PaneId, ProjectId, RepositoryId, WorkspaceId};
use crate::review::{CommitFile, CommitInfo, Review, ReviewAnchor, ReviewBase, ReviewContext};
use crate::diagrams::{DiagramAction, DiagramList};
use crate::agents::sessions::{Resume, Session};
use crate::agents::transcript::Conversation;
use crate::tasks::{TaskAction, TaskList};
use crate::reconcile::ReconcileAction;
use crate::tracker_links::MapChange;
use crate::tree::{AgentTelemetry, PaneInfo, ProjectInfo, WorkspaceInfo};

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
    /// A file from the client's machine, pasted or dropped into a pane: a
    /// screenshot off the clipboard, an image dragged onto the terminal.
    /// The daemon keeps it where the pane's host can read it and pastes
    /// that path, so a harness that takes an image by path gets one
    /// whether the daemon runs here or over ssh, where the client's
    /// clipboard and files are out of its reach. Only to a daemon that
    /// listed `PASTE_FILE`.
    PasteFile {
        pane: PaneId,
        /// What the client calls it; the daemon keeps the extension, which
        /// is how a harness tells an image from text.
        name: String,
        bytes: Vec<u8>,
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
        /// The new conversation's first message, handed to the agent the way
        /// its harness takes one. A daemon whose harness takes none refuses
        /// the start rather than dropping the message.
        #[serde(default)]
        prompt: Option<String>,
        /// Names the request, so the daemon can answer with what it made
        /// ([`ServerMsg::Created`]). Zero, the default, asks for no answer:
        /// what a client that predates it sends.
        #[serde(default)]
        request_id: u64,
    },
    /// Spawn a console: a shell in the home directory that belongs to no
    /// checkout. Only to a daemon that listed `CONSOLES`.
    SpawnConsole {
        /// Names the request, as for [`ClientMsg::SpawnShell`].
        #[serde(default)]
        request_id: u64,
    },
    /// Kill a pane's process and remove it.
    Kill {
        pane: PaneId,
    },
    /// A person naming a pane or a console. It is the pane's own title, so
    /// its agent may still retitle it with `argus-hook title`.
    RenamePane {
        pane: PaneId,
        title: String,
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
    ///
    /// Always answered under `request_id`, by a `ServerMsg::Review` or its
    /// refusal (`ServerMsg::ReviewFailed`, or `ServerMsg::Refused` to a
    /// client that listed `REQUEST_REFUSALS`). One a connection asks again
    /// — the same checkout, side and commit — while the first still waits
    /// for the daemon's turn is answered by the same diff under both ids;
    /// any other is read on its own, so asks for two commits never cancel
    /// each other.
    Review {
        request_id: u64,
        checkout: CheckoutId,
        base: ReviewBase,
        /// When set, the parent of this commit against the commit itself,
        /// ignoring `base` except as the flag [`ReviewBase::Commit`].
        #[serde(default)]
        commit: Option<String>,
    },
    /// Lines `start..=end` of a reviewed file's new side, for a fold the
    /// viewer opened. Sent only for a file whose diff named its
    /// [`FileDiff::new_blob`](crate::FileDiff::new_blob), so a daemon too old
    /// to answer is never asked. `path` chooses the grammar.
    ReviewContext {
        request_id: u64,
        checkout: CheckoutId,
        blob: String,
        path: String,
        start: u32,
        end: u32,
    },
    /// Every file of one version of a checkout, and the ones that version
    /// changed against its base. Only to a daemon that listed `CODE`.
    ListCode {
        request_id: u64,
        checkout: CheckoutId,
        version: CodeVersion,
    },
    /// One file of one version, highlighted, with what it changed and what
    /// it defines. Only to a daemon that listed `CODE`.
    ReadCode {
        request_id: u64,
        checkout: CheckoutId,
        path: String,
        version: CodeVersion,
    },
    /// Where `query` occurs in the checkout's working tree, read as
    /// `options` say. Only to a daemon that listed `CODE`.
    SearchCode {
        request_id: u64,
        checkout: CheckoutId,
        query: String,
        #[serde(default)]
        options: SearchOptions,
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
    /// The agent sessions kept for any of a repository's checkouts, newest
    /// first. Read off the harnesses' own files when asked, so a request
    /// rather than something the tree carries. Only to a daemon that
    /// listed `SESSIONS`.
    ListSessions {
        request_id: u64,
        repository: RepositoryId,
    },
    /// What one listed session said, read whole from its harness's file.
    /// `checkout` is where it was listed, which a harness's transcript path
    /// may be named after. Only to a daemon that listed `SESSIONS`.
    ReadSession {
        request_id: u64,
        checkout: CheckoutId,
        harness: String,
        session_id: String,
    },
    /// How each harness that can reopen one exact session does it,
    /// answered with [`ServerMsg::Resumes`]: what a client names before it
    /// asks for one. Only to a daemon that listed `SESSIONS`.
    ListResumes {
        request_id: u64,
    },
    /// Reopens a listed session in a new agent pane on `checkout`, from the
    /// template [`ServerMsg::Resumes`] named for its harness, answered with
    /// [`ServerMsg::Created`]. Refused for a harness no template can resume
    /// with. Only to a daemon that listed `SESSIONS`.
    ResumeSession {
        request_id: u64,
        checkout: CheckoutId,
        harness: String,
        session_id: String,
    },
    /// Persist a review comment, then notify the selected live agent.
    /// Answered with [`ServerMsg::ReviewCommentSaved`] under `request_id`,
    /// or refused by it ([`ServerMsg::Refused`]).
    ReviewComment {
        checkout: CheckoutId,
        recipient: PaneId,
        anchor: Box<ReviewAnchor>,
        body: String,
        /// Names the comment, so its save and its refusal are told by this
        /// id. Zero, the default, names nothing: what a client that
        /// predates it sends.
        #[serde(default)]
        request_id: u64,
    },
    /// Read a project's decision board. Whole rather than scoped: a
    /// decision hanging off three others means nothing without them.
    GetDecisions {
        project: ProjectId,
        checkout: CheckoutId,
    },
    /// Change a feature, in the same words an agent uses. `checkout` is
    /// the one whose board the person is looking at — and, for a transfer,
    /// the one the feature moves from. The answer is the pushed board every
    /// client already receives.
    Feature {
        project: ProjectId,
        checkout: CheckoutId,
        action: FeatureAction,
        /// Names the change, so a refusal of it is told by this id
        /// ([`ServerMsg::Refused`]). Zero, the default, names nothing: what
        /// a client that predates it sends.
        #[serde(default)]
        request_id: u64,
    },
    /// Write the repository's tracker map, or approve or turn down the
    /// draft an agent proposed for it.
    TrackerMap {
        project: ProjectId,
        checkout: CheckoutId,
        change: MapChange,
    },
    /// Ask an agent to reconcile a feature with its tickets, carry out the
    /// lines of its proposal the person ticked, or turn it down. Answered
    /// by the board push, or an `Error` saying why not.
    Reconcile {
        project: ProjectId,
        checkout: CheckoutId,
        slug: String,
        action: ReconcileAction,
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
    /// Record a decision under a feature: the person's `argus-hook decide`.
    /// The answer is the pushed board.
    Decide {
        project: ProjectId,
        checkout: CheckoutId,
        feature: String,
        write: DecisionWrite,
    },
    /// Answer, reword, withdraw or restore a decision under a feature.
    ChangeDecision {
        project: ProjectId,
        checkout: CheckoutId,
        feature: String,
        change: DecisionChange,
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
    ///
    /// `name`, `skip` and `init` are sent only to a daemon that listed
    /// [`crate::hello::ADD_PROJECT`]: an older one reads past them and
    /// would add something other than what was asked.
    AddProject {
        path: String,
        /// Names the request, so the daemon can answer with what it made
        /// ([`ServerMsg::Created`]). Zero, the default, asks for no answer:
        /// what a client that predates it sends.
        #[serde(default)]
        request_id: u64,
        /// What the panel calls it; `None` names it after the directory.
        #[serde(default)]
        name: Option<String>,
        /// Folders its scan never walks into, beyond the ones every scan
        /// skips — a project's `exclude`, in the same bare-name or
        /// root-relative form.
        #[serde(default)]
        skip: Vec<String>,
        /// Make `path` a repository first: created if missing, and `git
        /// init` unless it already is one, so adding an existing repository
        /// this way never initialises it again.
        #[serde(default)]
        init: bool,
    },
    /// The repositories `AddProject` would find under `path` with `skip`,
    /// for the add dialog to show before anything is added. Nothing is
    /// written. Answered with [`ServerMsg::ProjectPreview`], by
    /// `request_id`, since the scan walks the disk and an answer for a
    /// folder the person has since left must not land.
    PreviewProject {
        request_id: u64,
        path: String,
        skip: Vec<String>,
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
    /// What this client is showing, sent whenever that changes and again
    /// when its terminal regains focus. Only to a daemon that listed
    /// `FOCUS`.
    View(View),
    /// Start being told where the person is working: answered with a
    /// [`ServerMsg::Focus`] now and another each time it changes. Only to a
    /// daemon that listed `FOCUS`.
    WatchFocus,
    /// Start being sent the views agents show: every one shown now, and
    /// each one shown or closed after. Only to a daemon that listed
    /// `AGENT_VIEWS`; a client that never asks is never sent one.
    WatchViews,
    /// The person closed a view.
    CloseView { pane: PaneId, id: String },
    /// Ask the agent that showed a view to read it again and show it anew.
    /// The view stays as it is until it does.
    RefreshView { pane: PaneId, id: String },
    /// The person pressed one of a view's actions. The agent is told, as a
    /// refresh is asked of it, and the view stays as it is until it shows
    /// it again.
    PressView { pane: PaneId, press: crate::views::press::ViewPress },
    /// Start being sent the saved views: every one now, as one
    /// [`ServerMsg::SavedViews`], and each change after. Only to a daemon
    /// that listed `SAVED_VIEWS`.
    WatchSavedViews,
    /// Ask the agent that last saved a view to build it again and save it
    /// anew. The view stays as it is until it does. Refused with a
    /// [`ServerMsg::SavedViewFailed`] when that agent is not running.
    RefreshSavedView { id: String },
    /// The person says a saved view is out of date. Cleared by the next save.
    MarkSavedViewStale { id: String },
    /// The person deletes a saved view. Links and embeds that name it are
    /// left to draw as missing.
    DeleteSavedView { id: String },
    /// A press of an action in a saved view (`id`, or one embedded in it),
    /// relayed to the agent that last saved it as [`ClientMsg::PressView`]
    /// is, against what the view offers.
    PressSavedView { id: String, press: crate::views::press::ViewPress },
    /// An action taken in an extension's view, on `row` unless the action
    /// is view-wide. Forwarded to the extension as it stands; the view
    /// changes only when the extension publishes it again. Only to a daemon
    /// that listed `EXTENSIONS`.
    ExtensionAction {
        extension: String,
        view: String,
        row: Option<String>,
        action: String,
    },
    /// An extension's command run, with what this client is showing, which
    /// the daemon reads against its own tree before the extension is told.
    /// Only to a daemon that listed `EXTENSIONS`.
    ExtensionCommand {
        extension: String,
        command: String,
        context: View,
    },
    /// The answer to an extension's dialog, or `None` for one put away
    /// without an answer. Only to a daemon that listed `EXTENSION_DIALOGS`.
    ExtensionDialogAnswer {
        dialog: u64,
        answer: Option<ExtAnswer>,
    },
    /// Ask what each running extension is and publishes, answered with one
    /// [`ServerMsg::ExtensionsDescribed`]. Only to a daemon that listed
    /// `DESCRIBE_EXTENSIONS`.
    DescribeExtensions,
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
    /// Every console, whole, after each tree. Only to a client that listed
    /// `CONSOLES`. Apart from the tree because a console belongs to no
    /// project, and every client reads the tree as projects.
    Consoles(Vec<PaneInfo>),
    /// One agent's telemetry, whole, after it reported. Only to a client
    /// that listed `PANE_TELEMETRY`; any other is sent the whole tree,
    /// which carries the same record.
    PaneTelemetry {
        pane: PaneId,
        telemetry: AgentTelemetry,
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
        /// The kitty keyboard flags the child has pushed. Defaulted on the
        /// wire so an older daemon reads as "none", which keeps every key
        /// in its legacy encoding.
        #[serde(default)]
        keyboard: KeyboardMode,
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
        #[serde(default)]
        keyboard: KeyboardMode,
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
        #[serde(default)]
        keyboard: KeyboardMode,
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
        #[serde(default)]
        keyboard: KeyboardMode,
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
    /// A review that could not be read, a checkout that is gone included,
    /// correlated so stale failures are dropped. To a client that did not
    /// list `REQUEST_REFUSALS`; one that did is sent `Refused`.
    ReviewFailed {
        request_id: u64,
        checkout: CheckoutId,
        message: String,
    },
    /// The answer to `ClientMsg::ListCode`.
    CodeTree(CodeTree),
    /// The answer to `ClientMsg::ReadCode`.
    Code(Box<CodeFile>),
    /// The answer to `ClientMsg::SearchCode`.
    CodeMatches(CodeMatches),
    /// Any of the three that could not be answered, correlated by its id.
    /// To a client that did not list `REQUEST_REFUSALS`; one that did is
    /// sent `Refused`.
    CodeFailed {
        request_id: u64,
        checkout: CheckoutId,
        message: String,
    },
    /// The answer to `ClientMsg::ReviewContext`.
    ReviewContext(ReviewContext),
    /// A fold that could not be read, correlated like `ReviewFailed`. To a
    /// client that did not list `REQUEST_REFUSALS`; one that did is sent
    /// `Refused`.
    ReviewContextFailed {
        request_id: u64,
        checkout: CheckoutId,
        message: String,
    },
    /// The durable write succeeded. Delivery only describes the immediate
    /// terminal notification; an undelivered comment remains readable.
    ReviewCommentSaved {
        id: u64,
        delivered: bool,
        /// The `request_id` the comment was sent under: the client's, so
        /// the box that sent it knows its save. Zero from a daemon that
        /// predates it, and to a client that named none.
        #[serde(default)]
        request_id: u64,
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
    /// The answer to `ClientMsg::PreviewProject`.
    ProjectPreview(ProjectPreview),
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
    /// of showing an alert for a list it no longer wants. To a client that
    /// did not list `REQUEST_REFUSALS`; one that did is sent `Refused`.
    CommitsFailed {
        request_id: u64,
        checkout: CheckoutId,
        message: String,
    },
    /// The answer to `ClientMsg::ListSessions`: at most
    /// [`crate::agents::sessions::MAX_SESSIONS`], newest first.
    Sessions {
        request_id: u64,
        repository: RepositoryId,
        sessions: Vec<Session>,
    },
    /// The answer to `ClientMsg::ReadSession`.
    SessionTranscript {
        request_id: u64,
        session_id: String,
        conversation: Box<Conversation>,
    },
    /// The answer to `ClientMsg::ListResumes`: for each harness that can
    /// reopen one exact session, its name and the argv that would, with
    /// `{session_id}` left to fill. A harness not named cannot.
    Resumes {
        request_id: u64,
        resumes: Vec<Resume>,
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
        /// The commits that would stop being reachable, newest first and at
        /// most a screenful: what the person is being asked to give up.
        #[serde(default)]
        commits: Vec<crate::CommitInfo>,
    },
    /// A refusal of a request that named none, or any refusal to a client
    /// that did not list `REQUEST_REFUSALS`, in words for the person.
    Error {
        message: String,
    },
    /// A request that named itself, turned down: to its client alone, and
    /// only to one that listed `REQUEST_REFUSALS` (`request_refusal`).
    Refused(crate::request_refusal::RequestRefusal),
    /// What a request that named itself made: a pane, a checkout, a
    /// project or a repository. `None` only to a client that did not list
    /// `REQUEST_REFUSALS`, followed by the reason as an
    /// [`ServerMsg::Error`]; one that did is sent `Refused` instead.
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
    /// Where the person is working, to a client that sent
    /// [`ClientMsg::WatchFocus`]: what the most recently active client is
    /// showing, or `None` while no client reports a view.
    Focus(Option<Box<Focus>>),
    /// A view an agent showed, to a client that sent
    /// [`ClientMsg::WatchViews`]; it replaces one with the same pane and id.
    ViewShown(Box<ShownView>),
    /// A view is gone: closed by its agent or a person, pushed out by a
    /// newer one, or its pane ended.
    ViewClosed { pane: PaneId, id: String },
    /// A [`ClientMsg::PressView`] that was not relayed to the agent: the
    /// view is no longer held, no longer offers the action, or its agent
    /// is not running to be told. Sent only to the client that pressed, and
    /// only to one that listed `VIEW_ACTIONS`, so the person is not left
    /// waiting on an agent that was never asked. `reason` is a sentence for
    /// them.
    ViewPressFailed { pane: PaneId, view: String, action: String, reason: String },
    /// Every saved view, whole, to a client that sent
    /// [`ClientMsg::WatchSavedViews`]. Replaces what it held.
    SavedViews(Vec<crate::views::saved::SavedView>),
    /// A saved view that was saved, changed or marked stale; replaces the
    /// one with its id.
    SavedViewChanged(Box<crate::views::saved::SavedView>),
    /// A saved view that was deleted.
    SavedViewRemoved { id: String },
    /// A request about a saved view that was refused: it is gone, or its
    /// agent is not running to be asked. Sent only to the client that
    /// asked; `reason` is a sentence for the person.
    SavedViewFailed { id: String, reason: String },
    /// Every view every running extension publishes, whole, after the
    /// daemon's greeting and whenever one changes. Only to a client that
    /// listed `EXTENSIONS`.
    ExtensionViews(Vec<ExtView>),
    /// Every running extension's commands, whole, keys refused at
    /// registration already taken off, sent as views are.
    ExtensionCommands(Vec<ExtCommand>),
    /// A line an extension wants on the status bar.
    ExtensionNotice {
        extension: String,
        text: String,
    },
    /// A question an extension asks, sent only to the one client the person
    /// is at that can draw it, so only one window asks. `prompted` when it
    /// answers a command the person ran there, which is when it may take
    /// the keys. Only to a client that listed `EXTENSION_DIALOGS`.
    ExtensionDialog {
        extension: String,
        id: u64,
        dialog: ExtDialog,
        prompted: bool,
    },
    /// A dialog that is no longer asked: its extension went, or another
    /// answer reached it first.
    ExtensionDialogWithdrawn {
        id: u64,
    },
    /// Every running extension, to the client that sent
    /// [`ClientMsg::DescribeExtensions`] and to no other.
    ExtensionsDescribed(Vec<ExtDescription>),
    /// The daemon accepted a restart request and will exit after this frame
    /// reaches the client.
    Restarting,
    /// The daemon accepted a stop request and will exit after this frame
    /// reaches the client.
    Stopping,
}

/// What a creating request made.
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

/// What adding a folder as a project would find, as the daemon's own scan
/// finds it. The scan's rules come with it, so a client says what the
/// daemon it reached does rather than what its own build remembers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectPreview {
    pub request_id: u64,
    /// The folder scanned, as asked.
    pub path: String,
    /// Each repository found, relative to `path` with `/` separators, in
    /// the order the panel would list them; empty for `path` itself.
    pub repositories: Vec<String>,
    /// How many folders below `path` the scan looks.
    pub depth: usize,
    /// The folders every scan skips, whatever the project says.
    pub always_skipped: Vec<String>,
    /// Why there is nothing to show: a folder that is not there.
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire<T: Serialize>(msg: &T) -> Vec<u8> {
        rmp_serde::to_vec_named(msg).unwrap()
    }

    /// `SpawnShell` as a client built before requests were named sent it,
    /// `Scrollback` as one built before lines were numbered, `AddProject`
    /// as one built before a project could be named, and `Feature` and
    /// `ReviewComment` as one built before a change to a feature or a
    /// comment could name itself.
    #[derive(Serialize, Deserialize)]
    enum OlderClientMsg {
        SpawnShell { checkout: CheckoutId },
        Scrollback { pane: PaneId, offset: u32 },
        AddProject { path: String, request_id: u64 },
        Feature { project: ProjectId, checkout: CheckoutId, action: FeatureAction },
        ReviewComment { checkout: CheckoutId, recipient: PaneId, anchor: Box<ReviewAnchor>, body: String },
    }

    /// `ReviewCommentSaved` as a daemon built before comments were named
    /// sent it.
    #[derive(Serialize, Deserialize)]
    enum OlderServerMsg {
        ReviewCommentSaved { id: u64, delivered: bool },
    }

    fn anchor() -> Box<ReviewAnchor> {
        Box::new(ReviewAnchor {
            base: ReviewBase::Unstaged,
            commit: None,
            path: "a.rs".into(),
            old_path: None,
            old_start: None,
            old_end: None,
            new_start: Some(2),
            new_end: Some(2),
            text: vec!["+new".into()],
        })
    }

    #[test]
    fn an_add_from_an_older_client_names_nothing_skips_nothing_and_inits_nothing() {
        let older = wire(&OlderClientMsg::AddProject {
            path: "/src".into(),
            request_id: 3,
        });
        let read: ClientMsg = rmp_serde::from_slice(&older).unwrap();
        let ClientMsg::AddProject { path, request_id, name, skip, init } = read else {
            panic!("not an add: {read:?}");
        };
        assert_eq!((path.as_str(), request_id, name, skip.len(), init), ("/src", 3, None, 0, false));
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
    fn a_feature_change_from_an_older_client_names_no_request() {
        let older = wire(&OlderClientMsg::Feature {
            project: ProjectId(1),
            checkout: CheckoutId(2),
            action: FeatureAction::Drop { slug: "trim".into() },
        });
        let read: ClientMsg = rmp_serde::from_slice(&older).unwrap();
        assert!(matches!(read, ClientMsg::Feature { request_id: 0, .. }), "{read:?}");
    }

    #[test]
    fn an_older_daemon_reads_a_named_feature_change_and_ignores_the_name() {
        let newer = wire(&ClientMsg::Feature {
            project: ProjectId(1),
            checkout: CheckoutId(2),
            action: FeatureAction::Drop { slug: "trim".into() },
            request_id: 5,
        });
        let read: OlderClientMsg = rmp_serde::from_slice(&newer).unwrap();
        assert!(matches!(
            read,
            OlderClientMsg::Feature { project: ProjectId(1), checkout: CheckoutId(2), action: FeatureAction::Drop { slug } }
                if slug == "trim"
        ));
    }

    #[test]
    fn a_comment_from_an_older_client_names_no_request() {
        let older = wire(&OlderClientMsg::ReviewComment {
            checkout: CheckoutId(2),
            recipient: PaneId(3),
            anchor: anchor(),
            body: "tidy".into(),
        });
        let read: ClientMsg = rmp_serde::from_slice(&older).unwrap();
        assert!(
            matches!(&read, ClientMsg::ReviewComment { checkout: CheckoutId(2), recipient: PaneId(3), body, request_id: 0, .. }
                if body == "tidy"),
            "{read:?}"
        );
    }

    #[test]
    fn an_older_daemon_reads_a_named_comment_and_ignores_the_name() {
        let newer = wire(&ClientMsg::ReviewComment {
            checkout: CheckoutId(2),
            recipient: PaneId(3),
            anchor: anchor(),
            body: "tidy".into(),
            request_id: 6,
        });
        let read: OlderClientMsg = rmp_serde::from_slice(&newer).unwrap();
        assert!(matches!(
            read,
            OlderClientMsg::ReviewComment { checkout: CheckoutId(2), recipient: PaneId(3), anchor: a, body }
                if body == "tidy" && a == anchor()
        ));
    }

    #[test]
    fn a_save_from_an_older_daemon_names_no_request() {
        let older = wire(&OlderServerMsg::ReviewCommentSaved { id: 12, delivered: true });
        let read: ServerMsg = rmp_serde::from_slice(&older).unwrap();
        assert!(
            matches!(read, ServerMsg::ReviewCommentSaved { id: 12, delivered: true, request_id: 0 }),
            "{read:?}"
        );
    }

    #[test]
    fn an_older_client_reads_a_save_and_ignores_the_name() {
        let newer = wire(&ServerMsg::ReviewCommentSaved { id: 12, delivered: true, request_id: 41 });
        let read: OlderServerMsg = rmp_serde::from_slice(&newer).unwrap();
        assert!(matches!(read, OlderServerMsg::ReviewCommentSaved { id: 12, delivered: true }));
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

    #[test]
    fn a_description_of_the_extensions_survives_the_wire() {
        let about = crate::ExtAbout { summary: "Docker containers".into(), guide: Some("# Docker".into()) };
        let described = vec![crate::ExtDescription {
            name: "docker".into(),
            about: Some(about),
            commands: vec![ExtCommand {
                extension: "docker".into(),
                name: "up".into(),
                title: "Start a service".into(),
                key: Some(crate::ExtKey { alt: true, ..crate::ExtKey::plain("s") }),
                scope: crate::CommandScope::Checkout,
                help: Some("docker compose up -d, for one service".into()),
            }],
            views: Vec::new(),
        }];
        let bytes = wire(&ServerMsg::ExtensionsDescribed(described.clone()));
        let read: ServerMsg = rmp_serde::from_slice(&bytes).unwrap();
        assert!(matches!(read, ServerMsg::ExtensionsDescribed(read) if read == described));
        let asked: ClientMsg = rmp_serde::from_slice(&wire(&ClientMsg::DescribeExtensions)).unwrap();
        assert!(matches!(asked, ClientMsg::DescribeExtensions));
    }
}
