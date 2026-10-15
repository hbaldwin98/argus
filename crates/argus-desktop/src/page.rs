//! What the page and the window's Rust say to each other.
//!
//! Everything here crosses into `assets/app.js` as JSON, so a field's name
//! is a contract with the script that reads it: the tests pin each shape.
//! Rust sends events down one channel, and the page makes every call as
//! one [`Call`], which `commands` dispatches. `cargo test` writes each type
//! as TypeScript into `page/src/bindings`, so the page is compiled against
//! these definitions rather than a copy.

use serde::{Deserialize, Serialize};

use crate::features::Layout;
use crate::keymap::{Action, Command, Group};
use crate::keys::PageKey;
use crate::settings::{Backdrop, EditorMode, Preferences, Theme};

/// Something the page asks of the window. It answers nothing: what a call
/// changed reaches the page as events. Ids are the daemon's, as numbers.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(tag = "call", rename_all = "snake_case")]
pub enum Call {
    /// Puts a pane on screen.
    Select { pane: u64 },
    /// The rows and columns the page has room for on one side of the
    /// screen.
    Resize { side: Side, rows: u16, cols: u16 },
    Key { key: PageKey },
    /// Text an input method composed.
    Text { text: String },
    Paste { text: String },
    /// The paste key: the clipboard is read in Rust, not by the page, so a
    /// paste needs no permission from the webview.
    PasteClipboard,
    /// The wheel over `pane`, in notches (negative up), at the
    /// `[row, col]` under the pointer: either side of a split, whichever
    /// has the keys.
    Wheel { pane: u64, notches: i32, at: (u16, u16) },
    /// A mouse gesture over the pane the page did not keep for a
    /// selection, at the `[row, col]` under the pointer.
    Mouse { gesture: PageMouse, button: u8, at: (u16, u16) },
    /// A drag across the pane, `from` and `to` as `[row, col]`: the cells
    /// between them go on the clipboard.
    Copy { from: (u16, u16), to: (u16, u16) },
    SpawnShell { checkout: u64 },
    /// Starts an agent from the daemon's `template` on a checkout.
    SpawnAgent { checkout: u64, template: String },
    Close { pane: u64 },
    /// The window gained or lost focus.
    Focus { focused: bool },
    SelectProject { project: u64 },
    /// Opens one of the daemon's workspaces, which every client follows.
    OpenWorkspace { workspace: u64 },
    /// Asks whether to take a project out of the window.
    RemoveProject { project: u64 },
    /// The answer to that question.
    AnswerRemoveProject { yes: bool },
    OpenAddProject,
    /// A gesture in the add-a-project dialog.
    AddProject { gesture: AddProjectGesture },
    ToggleRepository { repository: u64 },
    ChooseRepository { repository: u64 },
    /// `open` true shows the Feature stage, false goes back to the Workspace.
    ShowFeatures { open: bool },
    /// `open` true shows the Checkouts stage, false goes back to the Workspace.
    ShowCheckouts { open: bool },
    /// `open` true shows the composer, false goes back to the Workspace.
    ShowComposer { open: bool },
    /// The composer's start: a shell without a `template`, else an agent
    /// from it, opening with `prompt` when one was typed.
    Compose { checkout: u64, template: Option<String>, prompt: Option<String> },
    /// A row of the Checkouts table chosen by its key.
    SelectCheckoutRow { key: String },
    FilterCheckouts { filter: String },
    /// A key or button of the Checkouts stage, acting on its selected row.
    CheckoutsAction { action: Action },
    /// Asks whether to remove a worktree picked in the rail.
    RemoveCheckout { checkout: u64 },
    /// The answer to the Checkouts stage's question.
    AnswerCheckouts { yes: bool },
    /// A worktree for the selected row, on the new branch `name` for a
    /// checkout row; a branch row brings its own.
    NewWorktree { name: Option<String> },
    OpenReview { checkout: u64 },
    /// A key or button of the review dialog.
    ReviewAction { action: Action },
    ReviewClick { target: ReviewClick },
    ReviewRefresh,
    /// Sends the comment, or with no `body` puts the box away.
    ReviewComment { body: Option<String> },
    ReviewJump { gesture: ReviewJumpGesture },
    OpenHistory { checkout: u64 },
    /// A key of the history dialog, or with `row` a click on a row.
    HistoryAction { action: Action, row: Option<usize> },
    FilterHistory { filter: String },
    /// Opens a repository's recent agent sessions.
    OpenSessions { repository: u64 },
    /// A key or button of the sessions dialog, or with `row` a click on a
    /// row: a session's, or in the full transcript one of the person's
    /// turns.
    SessionsAction { action: Action, row: Option<usize> },
    /// What the sessions dialog's search field says now.
    SearchSessions { search: String },
    /// What the full transcript's find field says now.
    FindInTranscript { find: String },
    /// The harness and checkout the sessions dialog's menus are set to;
    /// `None` is all of them.
    FilterSessions { harness: Option<String>, checkout: Option<String> },
    Peek { slug: String },
    SetLayout { layout: Layout },
    ToggleFolded,
    SetChime { on: bool },
    /// One choice in the Settings dialog, saved as it is made.
    Setting { change: SettingChange },
    /// Holds a feature for `reason`, or lifts its hold when there is none.
    Hold { slug: String, reason: Option<String> },
    /// Archives a feature, accepting it, or reopens it with `archived` false.
    Archive { slug: String, archived: bool },
    /// A gesture in the note a new feature is written as.
    Capture { gesture: CaptureGesture },
    /// Opens a feature from the index to read and edit, or with no `slug`
    /// goes back to the index.
    OpenFeature { slug: Option<String> },
    /// Puts the open feature's cursor on a row of its outline.
    SelectWork { at: WorkAt },
    /// Reads the open feature through another lens.
    SetLens { lens: Lens },
    /// Puts the Record lens's cursor on a decision.
    SelectDecision { id: i64 },
    /// Puts the Flows lens's cursor on a flow, and on a step of it by its
    /// place among the steps, counted from zero; `None` keeps the step.
    SelectStep { diagram: i64, step: Option<usize> },
    /// Says `slug` comes after `on`, or with `waits` false takes that back.
    Wait { slug: String, on: String, waits: bool },
    /// An edit to the open feature, sent to the daemon as it stands.
    EditFeature { edit: FeatureEdit },
    /// Leaves the Feature stage for a pane working a feature.
    GoToPane { pane: u64 },
    /// The command palette opened, or with `open` false closed.
    Palette { open: bool },
    /// A command chosen in the palette.
    Command { action: Action },
    GoToCheckout { checkout: u64 },
    GoToFeature { slug: String },
    /// A gesture on the console: its tabs, its buttons, its sash.
    Console { gesture: ConsoleGesture },
    /// Heard only while the daemon is stopped.
    Reconnect,
    Minimize,
    ToggleMaximize,
    /// Closing the window ends the program, as the system's button would.
    CloseWindow,
    /// A press on the strip's empty space: the system moves the window
    /// until the button is let go.
    DragWindow,
    /// A hover over the strip's maximize button, for Windows 11's snap
    /// layouts.
    SnapLayouts,
    /// Opens a view an agent showed in the side panel.
    OpenView { pane: u64, id: String },
    /// Puts the side panel away; the views stay.
    HideViews,
    /// Puts the side panel away, or brings out the newest view.
    ToggleViews,
    /// Where a drag of the rail's edge was let go, in CSS pixels; `None`
    /// to go back to its own width.
    RailWidth { width: Option<u32> },
    /// The same, for the views panel's edge.
    ViewsWidth { width: Option<u32> },
    /// Asks the agent that showed a view to read it again.
    RefreshView { pane: u64, id: String },
    /// Closes a view for every client.
    CloseView { pane: u64, id: String },
    /// Brings an extension's view out in the side panel.
    OpenExtensionView { extension: String, view: String },
    SelectExtensionRow { key: String },
    /// One of the view's actions, by its id, on its selected row.
    ExtensionAction { action: String },
    /// A key of the view's own, moving its selection.
    ExtensionKey { action: Action },
    /// The answer to the question an action asked first.
    AnswerExtension { yes: bool },
    /// An extension's command, by its key or from the palette; `checkout`
    /// and `pane` are the rail's picked row, which only the page knows.
    ExtensionCommand { extension: String, command: String, checkout: Option<u64>, pane: Option<u64> },
    /// The answer to an extension's dialog, or `None` when it was put
    /// away without one.
    AnswerDialog { dialog: u64, answer: Option<argus_protocol::ExtAnswer> },
    /// A link in a view, for the system's opener; Rust opens only an
    /// `http` or `https` one.
    OpenUrl { url: String },
}

/// One choice made in the Settings dialog.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", tag = "set")]
pub enum SettingChange {
    Theme { theme: Theme },
    /// Percent, as the slider gives it.
    Opacity { percent: u8 },
    Backdrop { backdrop: Backdrop },
    Editor { mode: EditorMode },
    EditorCommand { command: String },
    Notify { on: bool },
    NotifyInBackground { only: bool },
    FeatureLayout { layout: Layout },
}

/// What was done in the note a new feature is written as.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", tag = "on")]
pub enum CaptureGesture {
    /// An empty note, on the repository the Feature stage reads.
    Open,
    /// What the note says now, to be read as the daemon will read it.
    Note { note: String },
    /// Create, with the note as it is when pressed: calls need not arrive
    /// in the order sent, so the last `Note` may come after.
    Create { note: String },
    /// Puts the note away unsent.
    Discard,
}

/// A row of an open feature's outline, by the id a write names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum WorkAt {
    Task { id: i64 },
    Decision { id: i64 },
}

/// One edit to the open feature. Each names what it changes by id, so an
/// edit means the row it was made on whatever the cursor did since.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", tag = "on")]
pub enum FeatureEdit {
    /// The brief, whole.
    Brief { body: String },
    /// A task's title and description as the form holds them; only what
    /// changed is sent.
    Task { id: i64, title: String, body: String },
    AddCriterion { id: i64, text: String },
    /// Checks a criterion off, or takes that back. `index` is into the
    /// task's criteria as they were drawn.
    SetCriterion { id: i64, index: usize, done: bool },
    RemoveCriterion { id: i64, index: usize },
    /// A task, under `parent` or at the top.
    AddTask { parent: Option<i64>, title: String },
    /// A decision typed on one line — `?` for a question, `~` for an answer
    /// turned down — under another decision, or raised under a task.
    AddDecision { under: Option<i64>, task: Option<i64>, line: String },
    /// Answers an open decision with one of its options, by index.
    Take { id: i64, option: usize },
    /// Answers an open decision in words.
    Answer { id: i64, answer: String, because: Option<String> },
    /// A decision's record, as the ADR document it is edited as.
    Rewrite { id: i64, document: String },
    /// Withdraws a decision, or restores one withdrawn. Never removed: a
    /// withdrawn decision stays on the record.
    Withdraw { id: i64, withdrawn: bool },
    /// A decision typed on one line that takes `id`'s place in the tree.
    Supersede { id: i64, line: String },
    /// A new flow under `title`, from a skeleton to be edited.
    AddFlow { title: String },
    /// A flow's title and Mermaid source, whole.
    Flow { id: i64, title: String, source: String },
    /// Binds step `step` of a flow, numbered from one, to a decision.
    BindStep { diagram: i64, step: usize, decision: i64 },
    /// What step `step`, numbered from one, says.
    StepText { diagram: i64, step: usize, text: String },
    /// Moves step `step` to where step `to` is, both numbered from one.
    MoveStep { diagram: i64, step: usize, to: usize },
    /// Moves a participant's lifeline to place `to`, counted from zero.
    MoveParticipant { diagram: i64, name: String, to: usize },
}

/// How an open feature is read: as work, as a record of its decisions, or
/// as its flows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum Lens {
    #[default]
    Work,
    Record,
    Flows,
}

/// What a click in the review dialog landed on.
#[derive(Debug, Clone, Copy, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", tag = "on")]
pub enum ReviewClick {
    File { file: usize },
    Row { row: usize, extend: bool },
    Side { staged: bool },
    Recipient { pane: u64 },
}

/// What was done in the add-a-project dialog.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", tag = "on")]
pub enum AddProjectGesture {
    Kind { kind: AddKind },
    /// To a folder, by the daemon's own spelling of its path: one typed,
    /// or the parent.
    Browse { path: String },
    /// Into a folder listed in the one browsed.
    Into { name: String },
    /// What the fields say now.
    Fields { name: String, skip: String, folder: String },
    /// Add, with what the fields say as it is pressed: calls need not
    /// arrive in the order sent, so the last `Fields` may come after.
    Add { name: String, skip: String, folder: String },
    Close,
}

/// What a project is added from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum AddKind {
    /// Every repository under a folder, found by the daemon's scan.
    #[default]
    Folder,
    /// The one repository a folder is.
    Repository,
    /// A repository `git init` makes.
    New,
}

/// What was done to the console.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", tag = "on")]
pub enum ConsoleGesture {
    /// Shows it docked, or hides it wherever it is.
    Toggle,
    /// The Console stage: the console fills the stage.
    Tab,
    /// Docked under the stage, from the Console stage or fullscreen.
    Dock,
    /// Fullscreen beside the sidebar, or back to where it was.
    Fullscreen,
    Hide,
    /// One of its tabs, or a split side's header: that console on screen,
    /// with the keys.
    Select { pane: u64 },
    /// "+": a new console, shown once the daemon has made it.
    Spawn,
    /// A tab's close: the daemon ends that console.
    Close { pane: u64 },
    /// The sash let go at `height` CSS pixels.
    Height { height: u32 },
    /// Splits the console in two, or puts the split away keeping the side
    /// with the keys.
    Split,
    /// A split side's close: that side leaves the split, its console stays
    /// a tab.
    CloseSide { pane: u64 },
}

/// A side of the window's screen. The console splits it in two; anything
/// else is drawn on the first.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum Side {
    /// The window's one canvas, which moves between the Workspace and the
    /// console's left side.
    #[default]
    First,
    /// The console's right side, while it is split.
    Second,
}

/// What was done in the review's jump field: a letter typed, a match
/// stepped to, Enter, or Escape.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", tag = "on")]
pub enum ReviewJumpGesture {
    Query { text: String },
    Step { by: isize },
    Choose,
    Close,
}

/// One message down the page's channel.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Event {
    Status(Status),
    Rail(Box<Rail>),
    Frame(Frame),
    // The rail and the index are boxed: they are far the largest events,
    // and every other one would otherwise be sized to carry them.
    Features(Box<Features>),
    Checkouts(Box<Checkouts>),
    /// The note a new feature is written as, whole, whenever it changes;
    /// `open` false once it closes.
    Capture(Box<CapturePage>),
    /// The review dialog, whole, whenever it changes; `open` false once
    /// it closes.
    Review(Box<ReviewPage>),
    /// The history dialog, whole, whenever it changes; `open` false once
    /// it closes.
    History(Box<HistoryPage>),
    /// The recent-sessions dialog, whole, whenever it changes; `open`
    /// false once it closes.
    Sessions(Box<SessionsPage>),
    /// The add-a-project dialog, whole, whenever it changes; `open` false
    /// once it closes.
    AddProject(Box<AddProjectPage>),
    /// The console, whole, whenever it changes.
    Console(Box<ConsolePage>),
    /// Text a pane's child put on the clipboard (OSC 52).
    Clipboard { text: String },
    /// Something the daemon said that passes: a refused request's reason.
    /// The page shows it a while and lets it go.
    Notice { text: String },
    /// The views agents showed, whole, whenever one comes, goes or is
    /// opened.
    Views(Box<ViewsPage>),
    /// An agent came to need the person somewhere they are not looking.
    Chime,
    /// The Settings dialog's choices, whenever one changes: the page draws
    /// its theme and opacity from them whether or not the dialog is open.
    Settings(Box<SettingsPage>),
    /// Every binding, grouped as the shortcuts sheet lists them, and the
    /// palette's commands, sent once a page loads: the page dispatches from
    /// it outside a pane.
    Keymap { groups: Vec<Group>, commands: Vec<Command> },
    /// What the palette can go to, whenever it changes while open; `open`
    /// false once it closes.
    Palette(Box<Palette>),
    /// A key pressed in a pane, or a command chosen in the palette, for
    /// something only the page draws: the palette or the shortcuts sheet.
    Run { action: Action },
    /// What the extensions put in the window, whole, whenever it changes.
    Extensions(Box<ExtensionsPage>),
    /// The question an extension puts to this window, whole, whenever it
    /// changes; `open` false once none is left.
    ExtensionDialog(Box<ExtDialogPage>),
}

/// The connection, as the status bar and the empty stage show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub connection: Connection,
    /// What the last failed try said: `argus bridge`'s own last words.
    pub detail: Option<String>,
    /// What stays true while this connection lasts and the person should
    /// know: the daemon is another build, or speaks another protocol.
    pub alarm: Option<String>,
    /// Whether the window chimes, for the status bar's switch.
    pub chime: bool,
}

/// The Settings dialog: what is chosen, and what shows through the window
/// on this machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SettingsPage {
    pub chosen: Preferences,
    /// The least the opacity slider goes to.
    pub min_opacity: u8,
    /// The backdrops this machine draws, in the dialog's order; one it
    /// cannot draw at all is left out.
    pub backdrops: Vec<BackdropOffer>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct BackdropOffer {
    pub backdrop: Backdrop,
    /// Why it is greyed out here, when it is.
    pub unavailable: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum Connection {
    Connecting,
    Connected,
    /// Someone ran `argus server stop`: nothing reconnects until the
    /// person asks.
    Stopped,
}

/// Everything the rail, the stage header and the status bar's counts draw.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Rail {
    /// Every project, for the switcher.
    pub projects: Vec<ProjectChoice>,
    /// The project the rail lists.
    pub project: Option<Project>,
    /// Agents across every project: stalled on a person, and working.
    pub needs_you: usize,
    pub working: usize,
    /// The pane on screen, for the stage's header.
    pub shown: Option<Shown>,
    /// Whether the daemon has sent a tree yet, so the page can tell "no
    /// projects" from "not heard yet".
    pub loaded: bool,
    /// The repository whose features the Feature stage reads.
    pub repository: Option<u64>,
    pub stage: Stage,
    /// The pane fills the window: the rail and the stage's chrome are put
    /// away until the leader and `f` again.
    pub filled: bool,
    /// The agents a checkout's + offers beside a shell: the daemon's
    /// templates, the one this window last started first, and the first
    /// its default.
    pub templates: Vec<String>,
    /// The daemon's workspaces, for the switcher's Switch workspace….
    pub workspaces: Vec<WorkspaceChoice>,
    /// A project waiting on a yes to be removed.
    pub removing: Option<Removing>,
    /// The width the person dragged the rail to, in CSS pixels; `None`
    /// leaves the page's own.
    pub width: Option<u32>,
    /// The width the person dragged the views panel to, likewise.
    pub views_width: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ProjectChoice {
    pub id: u64,
    pub name: String,
    pub needs_you: usize,
    pub working: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceChoice {
    pub id: u64,
    pub name: String,
    /// The one open now; the tree is its projects.
    pub open: bool,
    pub projects: usize,
    /// Live panes in it, open or not.
    pub panes: usize,
}

/// The question before a project is taken out of the window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Removing {
    pub project: u64,
    pub name: String,
    pub repositories: usize,
    /// Panes still running in it, which argusd will not leave stranded.
    pub panes: usize,
}

/// The add-a-project dialog: a folder browsed on the daemon's machine, what
/// adding it would find, and what to call the project.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AddProjectPage {
    pub open: bool,
    pub kind: AddKind,
    /// The folder browsed, as the daemon spells it; `None` until listed.
    pub path: Option<String>,
    pub parent: Option<String>,
    pub entries: Vec<FolderEntry>,
    /// A listing has been asked for and not yet come.
    pub browsing: bool,
    /// Why the folder lists nothing: gone, or not readable.
    pub listing_error: Option<String>,
    /// What adding the folder would add, as the daemon's scan finds it:
    /// root-relative, the folder itself named by its own name.
    pub repositories: Vec<String>,
    pub previewing: bool,
    /// How deep the daemon's scan looks, and what it always skips, once a
    /// preview has said.
    pub depth: Option<usize>,
    pub always_skipped: Vec<String>,
    /// What the name, skip and new-folder fields say.
    pub name: String,
    pub skip: String,
    pub folder: String,
    /// The name the project gets while the name field is empty.
    pub default_name: String,
    /// For a new repository: the folder `git init` runs in.
    pub target: Option<String>,
    /// Why Add cannot be pressed yet; `None` when it can.
    pub blocked: Option<String>,
    /// The add is with the daemon.
    pub adding: bool,
    /// What the daemon said when it refused the last add.
    pub refusal: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct FolderEntry {
    pub name: String,
    pub is_repo: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: u64,
    pub name: String,
    pub needs_you: usize,
    pub repositories: Vec<Repository>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    pub id: u64,
    pub name: String,
    /// Nothing running in it.
    pub quiet: bool,
    /// The first quiet repository after active ones: a rule goes above it.
    pub divider: bool,
    pub open: bool,
    /// Listed panes across its checkouts.
    pub panes: usize,
    /// The loudest status word beneath it, for its dot.
    #[cfg_attr(test, ts(type = "(\"idle\" | \"working\" | \"waiting\" | \"needs-review\" | \"done\" | \"failed\" | \"exited\") | null"))]
    pub status: Option<&'static str>,
    /// How long since it was last used, for a quiet one: `2h`, `3d`.
    pub used: Option<String>,
    /// Empty while closed.
    pub checkouts: Vec<Checkout>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Checkout {
    pub id: u64,
    pub name: String,
    /// The branch, `detached`, or empty when the path is not a git checkout.
    pub branch: String,
    /// `main` for the repository's own working directory, `worktree` for a
    /// linked one: the row always names which, since a primary checkout is
    /// named after its repository and says nothing of it.
    #[cfg_attr(test, ts(type = "\"main\" | \"worktree\""))]
    pub kind: &'static str,
    /// `GitStatus::summary`, or `None` outside git.
    pub git: Option<String>,
    pub clean: bool,
    /// `CheckoutInfo::shared_by`: how many agents share it, when several do.
    pub shared: Option<usize>,
    pub panes: Vec<Pane>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Pane {
    pub id: u64,
    pub agent: bool,
    pub title: String,
    /// The loudest status word the row stands for, its children's included.
    #[cfg_attr(test, ts(type = "\"idle\" | \"working\" | \"waiting\" | \"needs-review\" | \"done\" | \"failed\" | \"exited\""))]
    pub status: &'static str,
    pub note: Option<String>,
    pub on: bool,
    /// How many views its agent has shown that are still up.
    pub views: usize,
}

/// Which stage is up. One at a time: opening one puts the others away, and
/// any of them over the Workspace keeps the pane on screen to come back to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    #[default]
    Workspace,
    Feature,
    Checkouts,
    /// The composer: what to start, where, and its first message.
    #[serde(rename = "new")]
    Composer,
    /// The console fills the stage.
    Console,
}

/// Where the console is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum ConsolePlace {
    /// Only the sidebar's foot says it is there.
    #[default]
    Hidden,
    /// Under the Workspace, Feature or Checkouts stage.
    Docked,
    /// The Console stage.
    Tab,
    /// The whole window but the sidebar's projects: the stages' nav and
    /// the sidebar's foot are put away.
    Fullscreen,
}

/// The console as the page draws it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ConsolePage {
    pub place: ConsolePlace,
    /// Docked, in CSS pixels.
    pub height: u32,
    /// A tab per console the daemon holds, in the order they were started.
    pub tabs: Vec<ConsoleTab>,
    /// The console whose screen the window's canvas is drawing, on the
    /// first side. The canvas is the Workspace's or the console's, so
    /// while this is set the Workspace stage has none.
    pub showing: Option<u64>,
    /// The console on the second side, while the console is split and
    /// has the canvas.
    pub beside: Option<u64>,
    /// Which of `showing` and `beside` has the keys, and is typed in.
    pub typing: Option<u64>,
    /// The daemon takes `SpawnConsole`; an older one offers no "+".
    pub can_spawn: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ConsoleTab {
    pub pane: u64,
    pub title: String,
    #[cfg_attr(test, ts(type = "\"idle\" | \"working\" | \"waiting\" | \"needs-review\" | \"done\" | \"failed\" | \"exited\""))]
    pub status: &'static str,
    /// The tab chosen for one side: shown when the console has the canvas.
    pub on: bool,
}

/// The pane on screen, as the stage's header names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Shown {
    pub pane: u64,
    pub repository: String,
    pub checkout: String,
    pub branch: String,
    pub agent: bool,
    pub title: String,
    #[cfg_attr(test, ts(type = "\"idle\" | \"working\" | \"waiting\" | \"needs-review\" | \"done\" | \"failed\" | \"exited\""))]
    pub status: &'static str,
    /// The session this window resumed in it, while the pane lasts.
    pub resumed: Option<Resumed>,
}

/// What a pane was resumed from, as its header says it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Resumed {
    /// When the session was last written to, as RFC 3339.
    pub at: Option<String>,
    /// The turns it had before it was resumed.
    pub turns: Option<u32>,
}

/// The Feature stage's index: one repository's features, as a list or a
/// board, and the one peeked at.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Features {
    /// The repository's name, for the heading.
    pub repository: String,
    /// `list` or `board`, as remembered for this repository.
    #[cfg_attr(test, ts(type = "\"list\" | \"board\""))]
    pub layout: &'static str,
    /// Whether held and archived features are shown in the list.
    pub show_folded: bool,
    /// Whether the board has arrived; until then the index says so.
    pub loaded: bool,
    pub active: usize,
    pub queued: usize,
    pub done: usize,
    pub held: usize,
    pub archived: usize,
    /// Every feature, grouped Active, Queued, Done, Held, Archived. The
    /// list folds the last two away unless `show_folded`; the board shows
    /// them as columns.
    pub rows: Vec<FeatureRow>,
    pub peek: Option<Peek>,
    /// The feature opened from the index, drawn in place of it.
    pub open: Option<FeatureView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct FeatureRow {
    pub slug: String,
    pub number: i64,
    pub title: String,
    /// `new`, `queued`, `active`, `done`, `held` or `archived`.
    #[cfg_attr(test, ts(type = "\"new\" | \"queued\" | \"active\" | \"done\" | \"held\" | \"archived\""))]
    pub standing: &'static str,
    /// The heading it is listed under: `new` is listed as `queued`.
    #[cfg_attr(test, ts(type = "\"queued\" | \"active\" | \"done\" | \"held\" | \"archived\""))]
    pub group: &'static str,
    pub tasks_done: usize,
    pub tasks_total: usize,
    pub decided: usize,
    pub open: usize,
    pub branch: Option<String>,
    /// The loudest agent on its checkouts: its title and status word.
    pub agent: Option<Agent>,
    /// Numbers of the open features it comes after.
    pub after: Vec<i64>,
    /// The reason it is held.
    pub held: Option<String>,
    pub peeked: bool,
    /// Where a person may move it by hand, with why the rest are not.
    pub moves: Vec<Move>,
    /// How it came to stand where it does, and how long ago it last
    /// changed: a card's footer.
    pub footer: String,
    /// The task that comes next, once its tasks are known: the peeked
    /// feature's.
    pub next: Option<Line>,
}

/// A feature another may come after, as the peek's After… menu offers it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AfterChoice {
    pub slug: String,
    pub number: i64,
    pub title: String,
    /// The peeked feature comes after it already; choosing it takes that
    /// back.
    pub waits: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    pub pane: u64,
    pub title: String,
    #[cfg_attr(test, ts(type = "\"idle\" | \"working\" | \"waiting\" | \"needs-review\" | \"done\" | \"failed\" | \"exited\""))]
    pub status: &'static str,
    pub needs_you: bool,
}

/// One place on the board, as a card's Move to… menu offers it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Move {
    /// `queued`, `active`, `done`, `held`, `archived`, `unhold`, `reopen`.
    #[cfg_attr(test, ts(type = "\"queued\" | \"active\" | \"done\" | \"held\" | \"archived\" | \"unhold\" | \"reopen\""))]
    pub to: &'static str,
    pub label: String,
    /// Why it cannot be chosen, for a place that follows the work.
    pub disabled: Option<String>,
}

/// What the index shows of the feature under the cursor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Peek {
    pub slug: String,
    pub number: i64,
    pub title: String,
    #[cfg_attr(test, ts(type = "\"new\" | \"queued\" | \"active\" | \"done\" | \"held\" | \"archived\""))]
    pub standing: &'static str,
    pub branch: Option<String>,
    pub brief: String,
    pub held: Option<String>,
    pub links: Vec<Link>,
    /// Open questions, then agents stopped for a person.
    pub needs_you: Vec<Line>,
    /// Tasks being done, each with the agent on it.
    pub in_flight: Vec<Line>,
    pub next_up: Option<Line>,
    pub last_decided: Option<Line>,
    /// The agent pane to go to.
    pub pane: Option<u64>,
    /// The open features it may come after, and those it does.
    pub after: Vec<AfterChoice>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Link {
    pub tracker: String,
    pub key: String,
    pub url: String,
}

/// The views agents showed, and the one open in the side panel.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ViewsPage {
    /// Newest first.
    pub views: Vec<PageView>,
    /// The pane and id of the view in the side panel, if it is out.
    pub open: Option<ViewRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ViewRef {
    pub pane: u64,
    pub id: String,
}

/// One view, with what the daemon says of it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PageView {
    pub pane: u64,
    /// The title of the pane whose agent showed it.
    pub from: String,
    /// When the daemon received it, in Unix seconds.
    pub at: i64,
    /// A refresh was asked for and the agent has not shown it again yet.
    pub refreshing: bool,
    /// The ticket it is of, if the agent named one.
    pub subject: Option<Link>,
    pub view: argus_protocol::AgentView,
    /// Each diagram block's source read as lifelines and steps, which the
    /// page draws: Mermaid is parsed once, in Rust, as the Flows lens
    /// reads it.
    pub diagrams: Vec<PageDiagram>,
}

/// A diagram block, read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PageDiagram {
    /// The block's source, which is how the page finds this.
    pub source: String,
    pub participants: Vec<PageParticipant>,
    pub rows: Vec<FlowLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PageParticipant {
    pub id: String,
    pub label: String,
    pub actor: bool,
}

/// One row of a flow: a message, or where it forks or repeats.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FlowLine {
    Step {
        number: usize,
        from: String,
        /// `None` when a participant talks to itself.
        to: Option<String>,
        /// Dashed: an answer rather than a request.
        reply: bool,
        text: String,
    },
    /// `else`, `opt`, `loop` and the like, and what it says about when.
    Branch { word: String, when: String },
}

/// A labelled line: `D3 How much history…`, `2.2 argus-hook reads…`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Line {
    pub label: String,
    pub text: String,
    /// Said under it in small type: the agent and branch, a state.
    pub aside: Option<String>,
}

/// A feature opened from the index: its header, its tasks and the
/// decisions raised under them as one outline, and the row under the cursor
/// in full.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct FeatureView {
    pub slug: String,
    pub number: i64,
    pub title: String,
    #[cfg_attr(test, ts(type = "\"new\" | \"queued\" | \"active\" | \"done\" | \"held\" | \"archived\""))]
    pub standing: &'static str,
    pub branch: Option<String>,
    pub agent: Option<Agent>,
    pub brief: String,
    /// Its tickets in the team's trackers.
    pub links: Vec<Link>,
    pub tasks_done: usize,
    pub tasks_total: usize,
    pub decided: usize,
    pub open: usize,
    pub lens: Lens,
    /// Whether its tasks have arrived; until then the outline says so.
    pub loaded: bool,
    /// The Work lens: the outline, and the row under its cursor, whichever
    /// it is. The other lenses' keys move along it too, so it is always
    /// sent.
    pub rows: Vec<WorkRow>,
    pub task: Option<TaskDetail>,
    pub decision: Option<DecisionDetail>,
    /// The Record lens, while it is the one read.
    pub record: Option<RecordLens>,
    /// The Flows lens, while it is the one read.
    pub flows: Option<FlowsLens>,
}

/// Every decision of a feature in the order it was made, and the one under
/// the cursor as its record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct RecordLens {
    pub rows: Vec<RecordRow>,
    pub decision: Option<DecisionDetail>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct RecordRow {
    pub id: i64,
    pub label: String,
    pub depth: usize,
    pub text: String,
    #[cfg_attr(test, ts(type = "\"open\" | \"accepted\" | \"rejected\" | \"withdrawn\" | \"superseded\""))]
    pub state: &'static str,
    pub on: bool,
}

/// A feature's flows, the one under the cursor read as steps, and the
/// diagram drawn from them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct FlowsLens {
    /// Whether the feature's flows have arrived.
    pub loaded: bool,
    pub flows: Vec<FlowItem>,
    pub flow: Option<FlowDetail>,
    /// The decisions a step can be bound to, by what a reader calls them.
    pub decisions: Vec<Line>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct FlowItem {
    pub id: i64,
    pub title: String,
    /// `6 steps · 1 branch`.
    pub summary: String,
    pub on: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct FlowDetail {
    pub id: i64,
    pub title: String,
    /// The Mermaid it is stored as, for Edit source.
    pub source: String,
    /// Its steps, with each `else`, `opt` and `loop` between them.
    pub rows: Vec<StepRow>,
    /// The step under the cursor, read whole.
    pub step: Option<StepDetail>,
    pub diagram: Sequence,
}

/// A line of the steps list: a step, or where the flow branches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct StepRow {
    /// One-based; `None` on a branch.
    pub number: Option<usize>,
    /// `hook → argusd` on a step; `else`, `opt` or `loop` on a branch.
    pub route: String,
    pub text: String,
    /// What binds it: `D2`, `task 1.2`.
    pub tags: Vec<String>,
    pub on: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct StepDetail {
    pub number: usize,
    pub route: String,
    pub text: String,
    /// Who it runs between, as the diagram calls them.
    pub participants: Vec<String>,
    pub bound_by: Vec<Line>,
    /// The tasks its decisions are carried out by, and those it names.
    pub carried_out_by: Vec<Line>,
}

/// A flow as the page draws it: lifelines left to right, then what runs
/// down them in order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Sequence {
    pub lifelines: Vec<Lifeline>,
    pub items: Vec<SequenceItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Lifeline {
    /// What the source calls it, which a drag names.
    pub name: String,
    pub label: String,
}

/// One thing down the lifelines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum SequenceItem {
    /// A step: an arrow between two lifelines, or a lifeline to itself
    /// when `from` and `to` are one. `reply` is dashed.
    Message { step: usize, from: usize, to: usize, reply: bool, text: String, on: bool },
    /// A frame opens: `alt`, `opt`, `loop`, `par`, `critical`, `break`.
    Open { frame: String, label: String },
    /// The frame's next arm: `else` or `and`.
    Arm { label: String },
    Close,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct WorkRow {
    pub at: WorkAt,
    /// `2.1` for a task, `D3` for a decision.
    pub label: String,
    pub depth: usize,
    pub text: String,
    /// A task's `todo`, `doing` or `done`; a decision's `open`,
    /// `accepted`, `rejected`, `withdrawn` or `superseded`.
    #[cfg_attr(test, ts(type = "\"todo\" | \"doing\" | \"done\" | \"open\" | \"accepted\" | \"rejected\" | \"withdrawn\" | \"superseded\""))]
    pub state: &'static str,
    /// The open decision holding this task up.
    pub waits_on: Option<String>,
    /// A task's key in the team's tracker.
    pub key: Option<String>,
    pub on: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct TaskDetail {
    pub id: i64,
    pub label: String,
    pub title: String,
    pub body: String,
    #[cfg_attr(test, ts(type = "\"todo\" | \"doing\" | \"done\""))]
    pub state: &'static str,
    /// The task it is under, as `2 · Serve the brief`.
    pub parent: Option<String>,
    /// What done means for it, each checked off as it lands.
    pub criteria: Vec<Criterion>,
    /// The standing decisions it is built on.
    pub bound_by: Vec<Line>,
    /// Oldest first: who left it and when, then what it says.
    pub notes: Vec<Line>,
    /// The open decision holding it up.
    pub waits_on: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Criterion {
    pub text: String,
    pub done: bool,
}

/// A decision as its record reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DecisionDetail {
    pub id: i64,
    pub label: String,
    pub chose: String,
    #[cfg_attr(test, ts(type = "\"open\" | \"accepted\" | \"rejected\" | \"withdrawn\" | \"superseded\""))]
    pub state: &'static str,
    /// Withdrawn, so its button restores it.
    pub withdrawn: bool,
    /// The task it was raised under, as `task 3`.
    pub raised_under: Option<String>,
    pub context: Option<String>,
    pub detail: Option<String>,
    pub because: Option<String>,
    pub over: Option<String>,
    /// Each as it is written: `+` a gain, `-` a cost.
    pub consequences: Vec<String>,
    /// The answers an open one is choosing between.
    pub options: Vec<String>,
    /// The tasks carrying it out.
    pub carried_out_by: Vec<Line>,
    /// The record as it is edited: every section, the empty ones too.
    pub document: String,
    /// The decision that took its place, by its label.
    pub superseded_by: Option<String>,
    /// `by you` or `by an agent`.
    pub by: &'static str,
    /// The answers turned down under it, each with why.
    pub considered: Vec<Line>,
    /// The record as it is copied out, as ADR markdown.
    pub adr: String,
}

/// The note a new feature is written as, and what the daemon's own reading
/// of it makes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CapturePage {
    pub open: bool,
    /// The repository it is written in, for the heading.
    pub repository: String,
    /// The note as the window last heard it, for a page that reloaded.
    pub note: String,
    pub title: String,
    pub brief: Option<String>,
    /// Every task at every depth, in the order a reader meets them.
    pub tasks: Vec<NoteRow>,
    pub decisions: Vec<NoteDecision>,
    /// One-based lines the grammar found no place for.
    pub unread: Vec<usize>,
    /// Why it cannot be created yet; `None` when it is ready.
    pub blocked: Option<String>,
    /// Sent, and waiting on the board to hold it.
    pub creating: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct NoteRow {
    pub label: String,
    pub depth: usize,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct NoteDecision {
    pub label: String,
    pub text: String,
    /// A question, with `options`; else made, with `because`.
    pub open: bool,
    pub options: Vec<String>,
    pub because: Option<String>,
}

/// What the command palette can go to while it is open.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Palette {
    pub open: bool,
    /// Every pane of the project.
    pub panes: Vec<PalettePane>,
    /// Every checkout of the project, those of closed repositories too.
    pub checkouts: Vec<PaletteCheckout>,
    /// The features of the repository the Feature stage reads.
    pub features: Vec<PaletteFeature>,
    /// Whether that repository's board has arrived.
    pub features_loaded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PalettePane {
    pub id: u64,
    pub title: String,
    pub agent: bool,
    #[cfg_attr(test, ts(type = "\"idle\" | \"working\" | \"waiting\" | \"needs-review\" | \"done\" | \"failed\" | \"exited\""))]
    pub status: &'static str,
    /// `repository / checkout`.
    pub place: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PaletteCheckout {
    pub id: u64,
    pub name: String,
    pub repository: String,
    pub branch: String,
    /// `GitStatus::summary`, or `None` outside git.
    pub git: Option<String>,
    pub clean: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PaletteFeature {
    pub slug: String,
    pub number: i64,
    pub title: String,
    #[cfg_attr(test, ts(type = "\"new\" | \"queued\" | \"active\" | \"done\" | \"held\" | \"archived\""))]
    pub standing: &'static str,
}

/// The rows of a pane's screen that changed since the last frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    pub pane: u64,
    /// The side drawing it, so a frame is never drawn in the other
    /// side's canvas while the page has yet to hear the pane moved.
    pub side: Side,
    pub rows: u16,
    pub cols: u16,
    /// Every row is in `lines`; anything the page held is stale.
    pub full: bool,
    pub cursor: FrameCursor,
    pub alternate_screen: bool,
    /// The child asked for mouse reports, so a press goes to it and only
    /// Shift-drag selects here.
    pub mouse: bool,
    /// `[row, segments]`, each row cleared before its segments are drawn.
    pub lines: Vec<(u16, Vec<Segment>)>,
    /// Set while the view is parked above the live screen.
    pub parked: Option<Parked>,
}

/// The Checkouts stage: the chosen repository's checkouts, and the
/// branches no checkout is on while they are shown.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Checkouts {
    /// The repository it lists; `None` before one is chosen.
    pub repository: Option<String>,
    pub checkouts: usize,
    /// Local and remote branches no checkout is on, shown or not.
    pub branches: usize,
    /// The repository's main branch, as the heading names it.
    pub main: Option<String>,
    pub branches_shown: bool,
    pub filter: String,
    pub rows: Vec<CheckoutsRow>,
    /// The selected row's `key`.
    pub selected: Option<String>,
    pub detail: Option<CheckoutDetail>,
    /// A question the stage is waiting on an answer to.
    pub confirm: Option<Confirm>,
}

/// What the stage asks before doing something that cannot be taken back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Confirm {
    /// Removing a worktree: its panes end, its directory and its branch go.
    Remove { name: String, branch: String },
    /// Git refused `branch -d`: the branch holds commits nothing else has,
    /// which are what a yes gives up.
    Force { branch: String, commits: Vec<LastCommit> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CheckoutsRow {
    /// Names the row in what the page sends back: `c<id>`, `b<branch>` or
    /// `r<origin/branch>`.
    pub key: String,
    /// The checkout's id; `None` for a branch with no directory.
    pub checkout: Option<u64>,
    /// The checkout's name; `None` for a branch row.
    pub name: Option<String>,
    pub primary: bool,
    pub branch: String,
    /// The compact git state, as on the rail; a branch row has none.
    pub git: Option<String>,
    pub clean: bool,
    /// Said beside the git state: `remote only` for what a fetch turned up,
    /// `merged` for a branch main already has.
    pub note: Option<&'static str>,
    /// What was last committed here; a branch row has none.
    pub last: Option<LastCommit>,
    pub running: Vec<Running>,
    /// `CheckoutInfo::shared_by`; a branch row has no agents to share it.
    pub shared: Option<usize>,
    /// The number of the feature this checkout is on, when the board says.
    pub feature: Option<i64>,
    /// That feature is the one the Feature stage has under its cursor.
    pub viewed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Running {
    pub agent: bool,
    pub status: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct LastCommit {
    pub short: String,
    pub summary: String,
    /// How long ago, as the rail says it: `2h`.
    pub ago: String,
}

/// The selected checkout, said in full along the stage's foot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CheckoutDetail {
    pub name: String,
    pub path: String,
    /// Its upstream and how far it is from it: `origin/main · 1 behind`;
    /// `None` when git has not been read.
    pub upstream: Option<String>,
    /// `1 unstaged file`, `3 staged · 2 unstaged files`, `no changes`.
    pub changes: Option<String>,
}

/// The review dialog: one checkout's diff, a side at a time.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ReviewPage {
    pub open: bool,
    pub checkout: u64,
    pub name: String,
    pub branch: String,
    /// `unstaged`, `staged`, or `commit` for one commit against its parent.
    #[cfg_attr(test, ts(type = "\"unstaged\" | \"staged\" | \"commit\""))]
    pub base: &'static str,
    /// The commit, `short  summary`, when the review is of one.
    pub commit: Option<String>,
    /// How many files each side changes; `None` until it has been read.
    pub unstaged: Option<usize>,
    pub staged: Option<usize>,
    pub split: bool,
    pub added: usize,
    pub removed: usize,
    pub files: Vec<ReviewFile>,
    /// The file on screen, an index into `files`.
    pub file: Option<usize>,
    /// The picked lines, as `new lines 39–40`.
    pub pick: Option<String>,
    /// The file on screen's hunks and lines.
    pub rows: Vec<DiffRow>,
    pub loading: bool,
    /// Why there is no diff: nothing on this side, or a failed read.
    pub empty: Option<String>,
    pub composing: bool,
    /// The agents in the checkout a comment can go to.
    pub recipients: Vec<Recipient>,
    pub recipient: Option<u64>,
    /// The Ctrl-P field over the file list, while it is open.
    pub jump: Option<ReviewJump>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ReviewJump {
    pub query: String,
    /// The files it matches, best first, as indices into `files`.
    pub matches: Vec<usize>,
    /// The match Enter opens, an index into `files`.
    pub at: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ReviewFile {
    pub path: String,
    /// The directory part, drawn faint, ending in `/`; empty at the root.
    pub dir: String,
    pub name: String,
    pub added: usize,
    pub removed: usize,
    #[cfg_attr(test, ts(type = "\"added\" | \"modified\" | \"deleted\" | \"renamed\" | \"untracked\""))]
    pub kind: &'static str,
}

/// The history dialog: a checkout's newest commits, and the diff of the
/// file under the cursor.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    pub open: bool,
    pub name: String,
    pub branch: String,
    /// What the filter field says; the rows are the commits it matches.
    pub filter: String,
    pub loading: bool,
    /// Why there are no rows: no commits yet, nothing matching the filter, or a failed read.
    pub empty: Option<String>,
    pub rows: Vec<HistoryRowView>,
    /// The commit under the cursor, short.
    pub commit: Option<String>,
    /// The file under the cursor, whose diff is `diff`.
    pub path: Option<String>,
    pub diff: Vec<DiffRow>,
    pub diff_loading: bool,
    /// How many files the commit's diff holds, once read.
    pub files: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct HistoryRowView {
    pub row: usize,
    #[cfg_attr(test, ts(type = "\"commit\" | \"file\""))]
    pub kind: &'static str,
    pub short: String,
    /// A commit's subject, or a file's path.
    pub text: String,
    pub ago: String,
    /// How many files a commit touched, once asked.
    pub files: Option<usize>,
    pub loading: bool,
    pub expanded: bool,
    pub added: usize,
    pub removed: usize,
    pub on: bool,
}

/// The recent-sessions dialog: one repository's agent sessions, live ones
/// first, and what the one under the cursor said.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SessionsPage {
    pub open: bool,
    pub repository: String,
    /// What the search field says; the rows are the sessions it matches.
    pub search: String,
    /// The harnesses and the checkouts the listing holds, for the menus,
    /// and what each menu is set to; `None` is all.
    pub harnesses: Vec<String>,
    pub harness: Option<String>,
    pub checkouts: Vec<String>,
    pub checkout: Option<String>,
    pub loading: bool,
    /// Why there are no rows: none yet, none matching, or a daemon that
    /// cannot list them.
    pub empty: Option<String>,
    pub rows: Vec<SessionRowView>,
    /// `None` while the full transcript is open, which draws it instead.
    pub preview: Option<SessionPreview>,
    /// The full transcript, read in place of the list.
    pub reader: Option<ReaderPage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SessionRowView {
    /// What a click on it sends back.
    pub row: usize,
    pub title: String,
    pub harness: String,
    /// Its checkout, or the removed worktree it ran in.
    pub checkout: String,
    /// Running in a pane now.
    pub live: bool,
    pub turns: Option<u32>,
    /// How long since its transcript was written: `2h`, or `—`.
    pub ago: String,
    /// It ran in a worktree since removed: drawn dimmed.
    pub removed: bool,
    pub on: bool,
}

/// The session under the cursor: what it said, and what can be done with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SessionPreview {
    pub title: String,
    pub harness: String,
    pub checkout: String,
    pub session_id: String,
    /// When its transcript starts, as the harness wrote it.
    pub started: Option<String>,
    /// What it said has been asked for and not yet read.
    pub loading: bool,
    /// Said in place of the transcript: none kept, or one gone missing.
    pub notice: Option<String>,
    /// Oldest first.
    pub entries: Vec<TranscriptLine>,
    /// How many older entries the daemon left out.
    pub earlier: u32,
    /// `go` to its pane, `resume` in a new one, or only its `transcript`.
    #[cfg_attr(test, ts(type = "\"go\" | \"resume\" | \"transcript\""))]
    pub offer: &'static str,
    /// The exact command a resume runs, as the footer names it.
    pub command: Option<String>,
    /// Why it cannot be resumed, when it offers only its transcript.
    pub why: Option<String>,
    /// Its full transcript can be opened: not one its harness keeps none
    /// of, or one gone missing.
    pub readable: bool,
}

/// One session's transcript read whole: the preview's session with the
/// person's turns to jump between and a find over what was said.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ReaderPage {
    pub session: SessionPreview,
    pub repository: String,
    /// When its transcript ends, as the harness wrote it.
    pub ended: Option<String>,
    pub turns: Option<u32>,
    pub jumps: Vec<TurnJump>,
    pub find: String,
    /// The entries find matches, by their place in `session.entries`.
    pub matches: Vec<usize>,
    /// The match find is on.
    pub found: Option<usize>,
    /// The entry the last move went to, to scroll to whenever `moves`
    /// changes.
    pub target: Option<usize>,
    pub moves: u32,
}

/// One of the person's turns, in the reader's jump list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct TurnJump {
    /// Its place in the transcript's entries.
    pub entry: usize,
    /// Its first line.
    pub text: String,
    pub at: Option<String>,
    pub on: bool,
}

/// One thing said or done in a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct TranscriptLine {
    #[cfg_attr(test, ts(type = "\"user\" | \"agent\" | \"tool\""))]
    pub kind: &'static str,
    /// A turn's or reply's text, or a tool call's one-line summary.
    pub text: String,
    /// The tool's name; empty for a turn or a reply.
    pub tool: String,
    pub failed: bool,
    /// When it happened, as the harness wrote it.
    pub at: Option<String>,
}

/// A row of the diff: a hunk header, a note, a line, or in split view a
/// pair of lines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DiffRow {
    /// What a click on it sends back.
    pub row: usize,
    #[cfg_attr(test, ts(type = "\"hunk\" | \"note\" | \"line\" | \"pair\""))]
    pub kind: &'static str,
    /// A hunk's `@@` line, or a note's reason.
    pub text: String,
    /// A line, or a pair's removed side.
    pub left: Option<DiffSide>,
    /// A pair's added side.
    pub right: Option<DiffSide>,
    pub cursor: bool,
    pub picked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DiffSide {
    pub old: Option<u32>,
    pub new: Option<u32>,
    pub marker: &'static str,
    #[cfg_attr(test, ts(type = "\"added\" | \"removed\" | \"context\""))]
    pub kind: &'static str,
    /// The text as `[text, highlight]`, the highlight a short class name
    /// (`k`, `s`, `c`, …) or empty.
    pub parts: Vec<(String, &'static str)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Recipient {
    pub pane: u64,
    pub title: String,
}

/// A mouse gesture over the pane the page did not keep for a selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum PageMouse {
    Down,
    Up,
    /// Moved with a button held.
    Drag,
    /// Moved with none held.
    Move,
}

/// How far up a parked view is, and how far back the history goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Parked {
    pub offset: u32,
    pub depth: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct FrameCursor {
    pub row: u16,
    pub col: u16,
    pub visible: bool,
    /// `block`, `underline` or `bar`; `block` for a child that asked for
    /// nothing.
    #[cfg_attr(test, ts(type = "\"block\" | \"underline\" | \"bar\""))]
    pub shape: &'static str,
}

/// A stretch of one style along a row: `[col, cells, fg, bg, flags]`, an
/// array because a busy screen sends thousands. `cells` holds a string per
/// cell, since a cell may carry combining marks the page must not split.
/// Colours are packed as on the wire — `0` the default, `1 << 24 | index`,
/// `2 << 24 | rgb` — so a theme switch is a repaint the page does alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct Segment(pub u16, pub Vec<String>, pub u32, pub u32, pub u8);

pub const BOLD: u8 = 1;
pub const ITALIC: u8 = 2;
pub const UNDERLINE: u8 = 4;
pub const REVERSE: u8 = 8;
/// One cell twice as wide as the rest; its spacer is not sent.
pub const WIDE: u8 = 16;

/// What the running extensions put in the window: the views listed where
/// the person is, the one out in the side panel, their commands, and their
/// keys as the sheet lists them.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ExtensionsPage {
    pub views: Vec<ExtTab>,
    /// The view out in the side panel, which takes it from the agents'.
    pub open: Option<ExtTable>,
    pub commands: Vec<ExtCommandPage>,
    /// A heading of the sheet per extension.
    pub groups: Vec<ExtGroup>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ExtTab {
    pub extension: String,
    pub view: String,
    pub title: String,
}

/// An extension's view as the panel draws it: its table, the row selected,
/// and what can be done.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ExtTable {
    pub extension: String,
    pub view: String,
    pub title: String,
    pub columns: Vec<argus_protocol::Column>,
    pub rows: Vec<ExtTableRow>,
    pub actions: Vec<ExtTableAction>,
    /// The selected row's key.
    pub selected: Option<String>,
    /// The question an action asks before it is sent.
    pub confirm: Option<ExtConfirm>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ExtTableRow {
    pub key: String,
    pub cells: Vec<argus_protocol::Value>,
    pub status: Option<argus_protocol::Chip>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ExtTableAction {
    pub id: String,
    pub label: String,
    pub key: Option<String>,
    pub view_wide: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ExtConfirm {
    /// The action's label, for the yes button.
    pub label: String,
    pub question: String,
}

/// A command an extension added: what the palette calls it, its key, and
/// where that key is heard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ExtCommandPage {
    pub extension: String,
    pub command: String,
    pub title: String,
    pub chords: Vec<ExtChord>,
    #[cfg_attr(test, ts(type = "\"anywhere\" | \"view\" | \"checkout\" | \"pane\""))]
    pub scope: &'static str,
    /// The view its key is heard in, for one scoped to a view.
    pub view: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ExtGroup {
    pub title: String,
    pub bindings: Vec<ExtBinding>,
}

/// One key an extension added, shaped as a keymap binding is so the sheet
/// draws it with the same keycaps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ExtBinding {
    pub label: String,
    pub chords: Vec<ExtChord>,
}

/// A key as `keymap::Chord` crosses, owned: an extension's are not known
/// until it says them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ExtChord {
    pub key: String,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
    pub leader: bool,
}

/// The question an extension puts to this window.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ExtDialogPage {
    pub open: bool,
    pub extension: String,
    pub id: u64,
    pub dialog: Option<argus_protocol::ExtDialog>,
    /// It answers a command the person ran here: it takes the keys. One
    /// raised on its own waits for a click, so a pane being typed in keeps
    /// them.
    pub prompted: bool,
    /// How many more wait behind it.
    pub waiting: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn as_json(event: Event) -> serde_json::Value {
        serde_json::to_value(event).unwrap()
    }

    #[test]
    fn the_settings_event_carries_the_choices_and_what_can_show_through() {
        let event = Event::Settings(Box::new(SettingsPage {
            chosen: Preferences::default(),
            min_opacity: 30,
            backdrops: vec![BackdropOffer { backdrop: Backdrop::Mica, unavailable: Some("Mica needs Windows 11".into()) }],
        }));
        assert_eq!(
            as_json(event),
            json!({
                "type": "settings",
                "chosen": {
                    "theme": "system", "opacity": 100, "backdrop": "clear", "editor": "floating",
                    "editorCommand": "", "notify": true, "notifyInBackground": true, "featureLayout": "list",
                    "zoom": 100
                },
                "minOpacity": 30,
                "backdrops": [{"backdrop": "mica", "unavailable": "Mica needs Windows 11"}]
            })
        );
    }

    #[test]
    fn a_setting_names_what_it_sets() {
        let read = |text: &str| serde_json::from_str::<SettingChange>(text).unwrap();
        assert_eq!(read(r#"{"set":"theme","theme":"macchiato"}"#), SettingChange::Theme { theme: Theme::Macchiato });
        assert_eq!(read(r#"{"set":"editor","mode":"separately"}"#), SettingChange::Editor { mode: EditorMode::Separately });
        assert_eq!(read(r#"{"set":"notifyInBackground","only":false}"#), SettingChange::NotifyInBackground { only: false });
        assert_eq!(read(r#"{"set":"featureLayout","layout":"board"}"#), SettingChange::FeatureLayout { layout: Layout::Board });
    }

    #[test]
    fn a_status_names_its_connection_and_what_was_said() {
        let event = Event::Status(Status {
            connection: Connection::Stopped,
            detail: Some("argusd not found".into()),
            alarm: None,
            chime: true,
        });
        assert_eq!(
            as_json(event),
            json!({"type": "status", "connection": "stopped", "detail": "argusd not found", "alarm": null, "chime": true})
        );
    }

    #[test]
    fn a_frame_sends_segments_as_arrays() {
        let event = Event::Frame(Frame {
            pane: 4,
            side: Side::Second,
            rows: 2,
            cols: 10,
            full: true,
            cursor: FrameCursor { row: 1, col: 2, visible: true, shape: "bar" },
            alternate_screen: false,
            mouse: false,
            lines: vec![(0, vec![Segment(3, vec!["h".into(), "i".into()], 0, 1 << 24 | 4, BOLD)])],
            parked: Some(Parked { offset: 120, depth: 4000 }),
        });
        assert_eq!(
            as_json(event),
            json!({
                "type": "frame", "pane": 4, "side": "second", "rows": 2, "cols": 10, "full": true,
                "cursor": {"row": 1, "col": 2, "visible": true, "shape": "bar"},
                "alternateScreen": false,
                "mouse": false,
                "lines": [[0, [[3, ["h", "i"], 0, 16777220, 1]]]],
                "parked": {"offset": 120, "depth": 4000}
            })
        );
    }

    #[test]
    fn the_rail_names_its_rows_in_camel_case() {
        let rail = Rail {
            projects: vec![ProjectChoice { id: 1, name: "src".into(), needs_you: 2, working: 1 }],
            project: Some(Project {
                id: 1,
                name: "src".into(),
                needs_you: 2,
                repositories: vec![Repository {
                    id: 2,
                    name: "argus".into(),
                    quiet: false,
                    divider: false,
                    open: true,
                    panes: 1,
                    status: Some("working"),
                    used: None,
                    checkouts: vec![Checkout {
                        id: 3,
                        name: "main".into(),
                        branch: "main".into(),
                        kind: "main",
                        git: Some("clean".into()),
                        clean: true,
                        shared: None,
                        panes: vec![Pane {
                            id: 4,
                            agent: true,
                            title: "claude".into(),
                            status: "working",
                            note: None,
                            on: true,
                            views: 0,
                        }],
                    }],
                }],
            }),
            needs_you: 2,
            working: 1,
            shown: Some(Shown {
                pane: 4,
                repository: "argus".into(),
                checkout: "main".into(),
                branch: "main".into(),
                agent: true,
                title: "claude".into(),
                status: "working",
                resumed: Some(Resumed { at: Some("2026-10-14T13:52:00Z".into()), turns: Some(48) }),
            }),
            loaded: true,
            repository: Some(2),
            stage: Stage::Composer,
            filled: false,
            templates: vec!["claude".into()],
            workspaces: vec![WorkspaceChoice { id: 7, name: "personal".into(), open: true, projects: 1, panes: 1 }],
            removing: Some(Removing { project: 1, name: "src".into(), repositories: 1, panes: 0 }),
            width: Some(320),
            views_width: None,
        };
        let value = as_json(Event::Rail(Box::new(rail)));
        assert_eq!(value["type"], "rail");
        assert_eq!(value["needsYou"], 2);
        assert_eq!(value["projects"][0]["needsYou"], 2);
        let repo = &value["project"]["repositories"][0];
        assert_eq!(repo["checkouts"][0]["panes"][0]["status"], "working");
        assert_eq!(repo["checkouts"][0]["git"], "clean");
        assert_eq!(value["shown"]["pane"], 4);
        assert_eq!(value["shown"]["resumed"], json!({"at": "2026-10-14T13:52:00Z", "turns": 48}));
        assert_eq!(value["loaded"], true);
        assert_eq!(value["stage"], "new");
        assert_eq!(value["workspaces"][0]["open"], true);
        assert_eq!(value["removing"]["repositories"], 1);
    }

    #[test]
    fn the_add_dialog_names_its_fields_in_camel_case_and_reads_its_gestures() {
        let page = AddProjectPage {
            open: true,
            kind: AddKind::New,
            entries: vec![FolderEntry { name: "web".into(), is_repo: true }],
            always_skipped: vec![".git".into()],
            default_name: "web".into(),
            ..AddProjectPage::default()
        };
        let value = as_json(Event::AddProject(Box::new(page)));
        assert_eq!(value["type"], "addProject");
        assert_eq!(value["kind"], "new");
        assert_eq!(value["entries"][0]["isRepo"], true);
        assert_eq!(value["alwaysSkipped"][0], ".git");
        assert_eq!(value["defaultName"], "web");

        let call: Call = serde_json::from_value(json!({
            "call": "add_project",
            "gesture": {"on": "fields", "name": "work", "skip": "vendor", "folder": ""}
        }))
        .unwrap();
        assert!(matches!(call, Call::AddProject { gesture: AddProjectGesture::Fields { .. } }));
        let call: Call = serde_json::from_value(json!({"call": "add_project", "gesture": {"on": "kind", "kind": "repository"}})).unwrap();
        assert!(matches!(call, Call::AddProject { gesture: AddProjectGesture::Kind { kind: AddKind::Repository } }));
    }

    #[test]
    fn the_index_names_its_rows_and_moves_in_camel_case() {
        let features = Features {
            repository: "argus".into(),
            layout: "board",
            loaded: true,
            active: 1,
            rows: vec![FeatureRow {
                slug: "trim".into(),
                number: 7,
                title: "Trim".into(),
                standing: "active",
                group: "active",
                tasks_done: 3,
                tasks_total: 7,
                decided: 2,
                open: 1,
                branch: Some("main".into()),
                agent: Some(Agent { pane: 4, title: "claude".into(), status: "working", needs_you: false }),
                after: vec![],
                held: None,
                peeked: true,
                moves: vec![Move { to: "done", label: "Done".into(), disabled: Some("when every task is done".into()) }],
                footer: "claude is on it · 2h ago".into(),
                next: None,
            }],
            ..Features::default()
        };
        let value = as_json(Event::Features(Box::new(features)));
        assert_eq!(value["type"], "features");
        assert_eq!(value["showFolded"], false);
        let row = &value["rows"][0];
        assert_eq!((row["tasksDone"].clone(), row["tasksTotal"].clone()), (json!(3), json!(7)));
        assert_eq!(row["agent"]["needsYou"], false);
        assert_eq!(row["moves"][0]["disabled"], "when every task is done");
    }

    #[test]
    fn a_clipboard_and_a_notice_carry_their_text() {
        let event = Event::Clipboard { text: "copied".into() };
        assert_eq!(as_json(event), json!({"type": "clipboard", "text": "copied"}));
        let event = Event::Notice { text: "no such pane".into() };
        assert_eq!(as_json(event), json!({"type": "notice", "text": "no such pane"}));
    }

    #[test]
    fn the_palette_and_a_run_name_their_fields_in_camel_case() {
        let palette = Palette {
            open: true,
            panes: vec![PalettePane { id: 4, title: "claude".into(), agent: true, status: "waiting", place: "argus / main".into() }],
            features_loaded: true,
            ..Palette::default()
        };
        let value = as_json(Event::Palette(Box::new(palette)));
        assert_eq!(value["type"], "palette");
        assert_eq!(value["featuresLoaded"], true);
        assert_eq!(value["panes"][0]["place"], "argus / main");
        assert_eq!(as_json(Event::Run { action: Action::Shortcuts }), json!({"type": "run", "action": "shortcuts"}));
    }
}
