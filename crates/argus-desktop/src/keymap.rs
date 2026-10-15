//! The window's keys, said once: each binding's keys, where it works, what
//! it does and what it is called.
//!
//! Dispatch reads this table on both sides — `session` for a key pressed in
//! a pane, the page for one pressed anywhere else — and it crosses to the
//! page whole, grouped as the shortcuts sheet will list it, so the sheet and
//! the palette are drawn from the bindings dispatch runs rather than from a
//! copy that could drift, as the terminal client's `app/mode/keymap` is.
//!
//! Inside a pane every key is the program's except the leader, Ctrl-Space as
//! in the terminal client: the key after it is the window's, and a second
//! leader sends one through.

use serde::{Deserialize, Serialize};

use crate::keys::PageKey;

/// One key, as the page reports it. `key` is `KeyboardEvent.key`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Chord {
    pub key: &'static str,
    pub ctrl: bool,
    pub alt: bool,
    /// Only asked of a named key: a character carries its own shift (`A` is
    /// shift-a), and which characters need shift depends on the layout.
    pub shift: bool,
    pub meta: bool,
    /// Pressed after the leader.
    pub leader: bool,
}

/// Where a binding works: the shortcuts sheet's groups, in its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum Place {
    /// Wherever the keyboard is not in a pane or a text field.
    Anywhere,
    /// In a pane, where `session` dispatches; everywhere else the page does.
    Pane,
    /// Everywhere, a pane, a text field and an open dialog included: the
    /// page hears these before anything else can.
    Window,
    /// A checkout or pane clicked in the sidebar, which holds the keys
    /// until a click elsewhere: the terminal client's rail cursor, where a
    /// pane is closed with a bare x rather than typed into.
    Rail,
    /// The Feature stage's index.
    Feature,
    /// The feature under the cursor, in the list or on the board.
    Card,
    /// A feature opened from the index: its outline and the row under the
    /// cursor.
    Outline,
    /// A field being written in — a feature's note or an edit, a message
    /// to an agent, a review comment, a name or a reason — which holds
    /// every other key.
    Writing,
    /// The Checkouts stage's table.
    Checkouts,
    /// A checkout's + menu while it is open, which holds the keys.
    Start,
    /// The review dialog while it is open, which holds the keys.
    Review,
    /// The review's jump-to-a-file field, which holds every other key.
    Jump,
    /// The history dialog while it is open, which holds the keys.
    History,
    /// The recent-sessions dialog while it is open, which holds the keys.
    Sessions,
    /// A session's full transcript, open in place of that dialog's list,
    /// which holds the keys until it closes.
    Transcript,
    /// A field that filters the list under it, which holds every other key.
    Filter,
    /// The palette's search field while it is open, which holds the keys.
    Palette,
}

/// What a binding does. The page runs those of the places it dispatches,
/// `session` those of a pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum Action {
    Workspace,
    Feature,
    /// The composer: a shell or an agent, where, and its first message.
    Compose,
    CloseLayer,
    /// The leader itself, sent to the program.
    Leader,
    Fill,
    Close,
    Paste,
    PageUp,
    PageDown,
    Next,
    Previous,
    First,
    Last,
    Layout,
    Folded,
    Filter,
    Move,
    Hold,
    Archive,
    GoToPane,
    /// A new feature, written as one note.
    NewFeature,
    /// Create the note's feature, or save the edit being written.
    Save,
    /// Edit what the cursor is on: a task's title and description, or a
    /// decision as its record.
    Edit,
    /// Edit the open feature's brief.
    Brief,
    AddTask,
    /// A subtask under the task at the cursor, or a sub-decision under the
    /// decision.
    Under,
    /// Raise a decision under the task at the cursor.
    Decide,
    /// Answer the open decision at the cursor in words.
    Answer,
    /// Withdraw the decision at the cursor, or restore it.
    Withdraw,
    StartShell,
    /// The daemon's default agent: its first template.
    StartAgent,
    /// What the highlighted item of a menu does, or Enter on a row.
    Choose,
    Checkouts,
    /// Show or hide the branches no checkout is on.
    Branches,
    Fetch,
    Pull,
    NewWorktree,
    Delete,
    /// Open the selected checkout's review.
    OpenReview,
    /// Start a range at the cursor, or let it go.
    Pick,
    Comment,
    /// Unstaged or staged.
    Side,
    Split,
    NextFile,
    PreviousFile,
    /// Open the selected checkout's history.
    OpenHistory,
    /// Unfold a commit to its files.
    Unfold,
    Fold,
    /// The commit under the cursor as a review.
    AsReview,
    /// The file at the cursor in the person's editor.
    OpenEditor,
    /// Open the chosen repository's recent agent sessions.
    OpenSessions,
    /// Resume the session under the cursor in a new pane.
    Resume,
    /// The session under the cursor's full transcript, read in place of
    /// the list.
    Transcript,
    /// The next or previous of the full transcript's find matches.
    NextMatch,
    PreviousMatch,
    /// Put the session's id on the clipboard.
    CopyId,
    /// Filter by the next harness the listing holds.
    FilterHarness,
    /// Filter by the next checkout the listing holds.
    FilterCheckout,
    /// Open the command palette.
    Palette,
    /// Open the shortcuts sheet, or close it.
    Shortcuts,
    /// Open the Settings dialog.
    Settings,
    /// Leave a filter field, giving the list it filtered the keys.
    Leave,
    /// The Console stage: the console fills the stage.
    Console,
    /// Show the console docked, or hide it wherever it is.
    ToggleConsole,
    /// The console fills the window but the sidebar's projects, and back.
    ConsoleFullscreen,
    /// A new console: a shell in the home directory.
    NewConsole,
    /// Split the console in two, or put the split away.
    SplitConsole,
    /// The split's other side takes the keys.
    OtherSide,
    /// The whole window larger or smaller, the panes' text with the rest,
    /// or back to its own size.
    ZoomIn,
    ZoomOut,
    ZoomReset,
    /// Show the views agents showed, or put them away.
    Views,
}

#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Binding {
    pub chords: &'static [Chord],
    #[serde(skip)]
    pub place: Place,
    pub action: Action,
    /// What the sheet and the palette call it.
    pub label: &'static str,
}

/// One heading of the sheet and the bindings under it.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub place: Place,
    pub title: &'static str,
    pub bindings: Vec<&'static Binding>,
}

/// A command the palette lists: what it runs, what it is called there, and
/// the keys that run it outside the palette, from the binding in `place`.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Command {
    pub action: Action,
    pub label: &'static str,
    pub place: Option<Place>,
    pub chords: &'static [Chord],
}

impl Chord {
    const fn key(key: &'static str) -> Chord {
        Chord { key, ctrl: false, alt: false, shift: false, meta: false, leader: false }
    }

    const fn ctrl(self) -> Chord {
        Chord { ctrl: true, ..self }
    }

    const fn alt(self) -> Chord {
        Chord { alt: true, ..self }
    }

    const fn shift(self) -> Chord {
        Chord { shift: true, ..self }
    }

    const fn meta(self) -> Chord {
        Chord { meta: true, ..self }
    }

    const fn after_leader(self) -> Chord {
        Chord { leader: true, ..self }
    }

    /// Whether `page` is this key, the leader aside. The page's `matches`
    /// in `page/src/keymap.ts` is this rule again, since the page must
    /// decide before Rust hears the key whether the webview may have it.
    pub fn matches(&self, page: &PageKey) -> bool {
        let character = self.key.chars().count() == 1;
        page.key == self.key
            && (character || page.shift == self.shift)
            && page.ctrl == self.ctrl
            && page.alt == self.alt
            && page.meta == self.meta
    }
}

impl Place {
    fn title(self) -> &'static str {
        match self {
            Place::Anywhere => "Anywhere",
            Place::Pane => "In a pane",
            Place::Window => "Everywhere",
            Place::Rail => "Sidebar",
            Place::Feature => "Feature",
            Place::Card => "Board card",
            Place::Outline => "An open feature",
            Place::Writing => "Writing in a field",
            Place::Checkouts => "Checkouts",
            Place::Start => "A checkout's + menu",
            Place::Review => "Review",
            Place::Jump => "Jumping to a file",
            Place::History => "History",
            Place::Sessions => "Recent sessions",
            Place::Transcript => "A full transcript",
            Place::Filter => "A filter or search field",
            Place::Palette => "Palette",
        }
    }
}

pub const LEADER: Chord = Chord::key(" ").ctrl();

/// The command key is the paste chord on a Mac. Elsewhere Ctrl-V is, as in
/// the terminal client, with Ctrl-Shift-V and Alt-V, the keys a terminal on
/// Windows keeps for itself.
const PASTE: &[Chord] = if cfg!(target_os = "macos") {
    &[Chord::key("v").meta()]
} else {
    &[Chord::key("v").ctrl(), Chord::key("V").ctrl(), Chord::key("v").alt()]
};

/// The command key on a Mac, as its browsers zoom; Ctrl elsewhere, as
/// terminals size their text, which takes Ctrl-minus from a pane's program.
/// `+` is `=` shifted, and the keypad's own.
const ZOOM_IN: &[Chord] = if cfg!(target_os = "macos") {
    &[Chord::key("=").meta(), Chord::key("+").meta()]
} else {
    &[Chord::key("=").ctrl(), Chord::key("+").ctrl()]
};
const ZOOM_OUT: &[Chord] = if cfg!(target_os = "macos") {
    &[Chord::key("-").meta()]
} else {
    &[Chord::key("-").ctrl()]
};
const ZOOM_RESET: &[Chord] = if cfg!(target_os = "macos") {
    &[Chord::key("0").meta()]
} else {
    &[Chord::key("0").ctrl()]
};

const fn bind(place: Place, chords: &'static [Chord], action: Action, label: &'static str) -> Binding {
    Binding { chords, place, action, label }
}

use Action::*;
use Place::{Anywhere, Card, History, Jump, Outline, Pane, Rail, Review, Sessions, Start, Window, Writing};
use Place::Checkouts as CheckoutsTable;

pub static BINDINGS: &[Binding] = &[
    bind(Anywhere, &[Chord::key("1")], Workspace, "Workspace"),
    bind(Anywhere, &[Chord::key("2")], Feature, "Feature"),
    bind(Anywhere, &[Chord::key("3")], Action::Checkouts, "Checkouts"),
    bind(Anywhere, &[Chord::key("4")], Action::Console, "Console"),
    bind(Anywhere, &[Chord::key("`")], ToggleConsole, "show or hide the console"),
    bind(Anywhere, &[Chord::key("n").ctrl()], Compose, "start a shell or an agent"),
    bind(Anywhere, &[Chord::key("Escape")], CloseLayer, "close the menu or dialog"),
    bind(Anywhere, &[Chord::key("k").ctrl()], Action::Palette, "search actions, panes, features and checkouts"),
    bind(Anywhere, &[Chord::key("?")], Shortcuts, "these shortcuts"),
    bind(Anywhere, &[Chord::key(",").ctrl()], Action::Settings, "settings"),
    bind(Anywhere, &[Chord::key(".").ctrl()], Views, "show or hide the views agents showed"),
    bind(Pane, &[LEADER.after_leader()], Leader, "send ctrl-space to the program"),
    bind(Pane, &[Chord::key("f").after_leader()], Fill, "fill the window with the pane, and back"),
    bind(Pane, &[Chord::key("1").after_leader()], Workspace, "Workspace"),
    bind(Pane, &[Chord::key("2").after_leader()], Feature, "Feature"),
    bind(Pane, &[Chord::key("x").after_leader()], Close, "close the pane, ending its program"),
    bind(Pane, &[Chord::key("3").after_leader()], Action::Checkouts, "Checkouts"),
    bind(Pane, &[Chord::key("4").after_leader()], Action::Console, "Console"),
    bind(Pane, &[Chord::key("`").after_leader()], ToggleConsole, "show or hide the console"),
    bind(Pane, &[Chord::key("z").after_leader()], ConsoleFullscreen, "the console fills the window, and back"),
    bind(Pane, &[Chord::key("\\").after_leader()], SplitConsole, "split the console in two, and back"),
    bind(Pane, &[Chord::key("o").after_leader()], OtherSide, "the keys to the split's other side"),
    bind(Pane, &[Chord::key("n").after_leader()], Compose, "start a shell or an agent"),
    bind(Pane, &[Chord::key("Tab").after_leader()], OpenReview, "review the pane's checkout"),
    bind(Pane, PASTE, Paste, "paste"),
    bind(Pane, &[Chord::key("PageUp").shift()], PageUp, "back through the history"),
    bind(Pane, &[Chord::key("PageDown").shift()], PageDown, "forward to the live screen"),
    bind(Pane, &[Chord::key("k").ctrl().after_leader(), Chord::key("k").after_leader()], Action::Palette, "search actions, panes, features and checkouts"),
    bind(Pane, &[Chord::key("?").after_leader()], Shortcuts, "these shortcuts"),
    bind(Pane, &[Chord::key(",").after_leader()], Action::Settings, "settings"),
    bind(Window, ZOOM_IN, ZoomIn, "larger"),
    bind(Window, ZOOM_OUT, ZoomOut, "smaller"),
    bind(Window, ZOOM_RESET, ZoomReset, "the usual size"),
    bind(Pane, &[Chord::key("v").after_leader()], Views, "show or hide the views agents showed"),
    bind(Rail, &[Chord::key("j"), Chord::key("ArrowDown")], Next, "next checkout or pane"),
    bind(Rail, &[Chord::key("k"), Chord::key("ArrowUp")], Previous, "previous checkout or pane"),
    bind(Rail, &[Chord::key("s"), Chord::key("S")], StartShell, "start a shell on its checkout"),
    bind(Rail, &[Chord::key("a"), Chord::key("A")], StartAgent, "start the default agent on its checkout"),
    bind(Rail, &[Chord::key("x"), Chord::key("X")], Close, "close the pane, or remove the checkout"),
    bind(Rail, &[Chord::key("Enter")], Choose, "open the checkout's + menu, or type into the pane"),
    bind(Place::Feature, &[Chord::key("j"), Chord::key("ArrowDown")], Next, "next feature"),
    bind(Place::Feature, &[Chord::key("k"), Chord::key("ArrowUp")], Previous, "previous feature"),
    bind(Place::Feature, &[Chord::key("Home")], First, "first feature"),
    bind(Place::Feature, &[Chord::key("End")], Last, "last feature"),
    bind(Place::Feature, &[Chord::key("v"), Chord::key("V")], Layout, "list or board"),
    bind(Place::Feature, &[Chord::key("z")], Folded, "show held and archived, or fold them"),
    bind(Place::Feature, &[Chord::key("/")], Filter, "filter by title or number"),
    bind(Place::Feature, &[Chord::key("N")], NewFeature, "write a new feature as one note"),
    bind(Card, &[Chord::key("Enter")], Choose, "open it"),
    bind(Card, &[Chord::key("m"), Chord::key("M")], Move, "move to…"),
    bind(Card, &[Chord::key("h"), Chord::key("H")], Hold, "hold, or lift its hold"),
    bind(Card, &[Chord::key("A")], Archive, "archive, or reopen"),
    bind(Card, &[Chord::key("g"), Chord::key("G")], GoToPane, "go to its agent's pane"),
    bind(Outline, &[Chord::key("j"), Chord::key("ArrowDown")], Next, "next task or decision"),
    bind(Outline, &[Chord::key("k"), Chord::key("ArrowUp")], Previous, "previous task or decision"),
    bind(Outline, &[Chord::key("Home")], First, "first row"),
    bind(Outline, &[Chord::key("End")], Last, "last row"),
    bind(Outline, &[Chord::key("e"), Chord::key("E")], Edit, "edit the task, or the decision as its record"),
    bind(Outline, &[Chord::key("b"), Chord::key("B")], Brief, "edit the brief"),
    bind(Outline, &[Chord::key("a"), Chord::key("A")], AddTask, "new task"),
    bind(Outline, &[Chord::key("s"), Chord::key("S")], Under, "a subtask or sub-decision under it"),
    bind(Outline, &[Chord::key("d"), Chord::key("D")], Decide, "raise a decision under the task"),
    bind(Outline, &[Chord::key("o"), Chord::key("O")], Answer, "answer the open decision in words"),
    bind(Outline, &[Chord::key("x"), Chord::key("X")], Withdraw, "withdraw the decision, or restore it"),
    bind(Outline, &[Chord::key("g"), Chord::key("G")], GoToPane, "go to its agent's pane"),
    bind(Writing, &[Chord::key("Enter").ctrl(), Chord::key("Enter").meta()], Save, "create, send or save what is written"),
    bind(Writing, &[Chord::key("Enter")], Choose, "save a one-line field"),
    bind(Writing, &[Chord::key("Escape")], CloseLayer, "discard what was written"),
    bind(CheckoutsTable, &[Chord::key("j"), Chord::key("ArrowDown")], Next, "next row"),
    bind(CheckoutsTable, &[Chord::key("k"), Chord::key("ArrowUp")], Previous, "previous row"),
    bind(CheckoutsTable, &[Chord::key("Home")], First, "first row"),
    bind(CheckoutsTable, &[Chord::key("End")], Last, "last row"),
    bind(CheckoutsTable, &[Chord::key("Enter")], Choose, "open"),
    bind(CheckoutsTable, &[Chord::key("b"), Chord::key("B")], Branches, "show branches"),
    bind(CheckoutsTable, &[Chord::key("/")], Filter, "filter"),
    bind(CheckoutsTable, &[Chord::key("f"), Chord::key("F")], Fetch, "fetch"),
    bind(CheckoutsTable, &[Chord::key("p"), Chord::key("P")], Pull, "pull"),
    bind(CheckoutsTable, &[Chord::key("n"), Chord::key("N")], NewWorktree, "new worktree"),
    bind(CheckoutsTable, &[Chord::key("D")], Delete, "delete"),
    bind(CheckoutsTable, &[Chord::key("r"), Chord::key("R")], OpenReview, "review"),
    bind(CheckoutsTable, &[Chord::key("h"), Chord::key("H")], OpenHistory, "history"),
    bind(CheckoutsTable, &[Chord::key("s"), Chord::key("S")], StartShell, "start a shell"),
    bind(CheckoutsTable, &[Chord::key("a"), Chord::key("A")], StartAgent, "start the default agent"),
    bind(Start, &[Chord::key("s"), Chord::key("S")], StartShell, "start a shell"),
    bind(Start, &[Chord::key("a"), Chord::key("A")], StartAgent, "start the default agent"),
    bind(Start, &[Chord::key("j"), Chord::key("ArrowDown")], Next, "next item"),
    bind(Start, &[Chord::key("k"), Chord::key("ArrowUp")], Previous, "previous item"),
    bind(Start, &[Chord::key("Enter")], Choose, "start the highlighted one"),
    bind(Review, &[Chord::key("j"), Chord::key("ArrowDown")], Next, "next line"),
    bind(Review, &[Chord::key("k"), Chord::key("ArrowUp")], Previous, "previous line"),
    bind(Review, &[Chord::key("Home")], First, "first line"),
    bind(Review, &[Chord::key("End")], Last, "last line"),
    bind(Review, &[Chord::key("]")], NextFile, "next file"),
    bind(Review, &[Chord::key("[")], PreviousFile, "previous file"),
    bind(Review, &[Chord::key("v"), Chord::key("V")], Pick, "pick a range"),
    bind(Review, &[Chord::key("c"), Chord::key("C")], Comment, "comment"),
    bind(Review, &[Chord::key("b"), Chord::key("B")], Side, "staged or unstaged"),
    bind(Review, &[Chord::key("s"), Chord::key("S")], Split, "split or unified"),
    bind(Review, &[Chord::key("p").ctrl()], Filter, "jump to a file"),
    bind(Review, &[Chord::key("e"), Chord::key("E")], OpenEditor, "open in your editor"),
    bind(Jump, &[Chord::key("ArrowDown"), Chord::key("n").ctrl()], Next, "next file"),
    bind(Jump, &[Chord::key("ArrowUp"), Chord::key("p").ctrl()], Previous, "previous file"),
    bind(Jump, &[Chord::key("Enter")], Choose, "open the file"),
    bind(Jump, &[Chord::key("Escape")], CloseLayer, "close the jump, keeping the review"),
    bind(History, &[Chord::key("j"), Chord::key("ArrowDown")], Next, "next row"),
    bind(History, &[Chord::key("k"), Chord::key("ArrowUp")], Previous, "previous row"),
    bind(History, &[Chord::key("Home")], First, "newest"),
    bind(History, &[Chord::key("End")], Last, "oldest"),
    bind(History, &[Chord::key("l"), Chord::key("ArrowRight")], Unfold, "open a commit"),
    bind(History, &[Chord::key("h"), Chord::key("ArrowLeft")], Fold, "fold it"),
    bind(History, &[Chord::key("Enter")], Choose, "open a commit, or a file as a review"),
    bind(History, &[Chord::key("o"), Chord::key("O")], AsReview, "open as a review"),
    bind(History, &[Chord::key("/")], Filter, "filter commits"),
    bind(Sessions, &[Chord::key("j"), Chord::key("ArrowDown")], Next, "next session"),
    bind(Sessions, &[Chord::key("k"), Chord::key("ArrowUp")], Previous, "previous session"),
    bind(Sessions, &[Chord::key("Home")], First, "first session"),
    bind(Sessions, &[Chord::key("End")], Last, "last session"),
    bind(Sessions, &[Chord::key("Enter")], Choose, "go to its pane, resume it, or read it whole"),
    bind(Sessions, &[Chord::key("r"), Chord::key("R")], Resume, "resume in a new pane"),
    bind(Sessions, &[Chord::key("o"), Chord::key("O")], Action::Transcript, "open the full transcript"),
    bind(Sessions, &[Chord::key("/")], Filter, "search sessions"),
    bind(Sessions, &[Chord::key("h"), Chord::key("H")], FilterHarness, "filter by the next harness"),
    bind(Sessions, &[Chord::key("c"), Chord::key("C")], FilterCheckout, "filter by the next checkout"),
    bind(Place::Transcript, &[Chord::key("j"), Chord::key("ArrowDown")], Next, "your next turn"),
    bind(Place::Transcript, &[Chord::key("k"), Chord::key("ArrowUp")], Previous, "your previous turn"),
    bind(Place::Transcript, &[Chord::key("Home")], First, "your first turn"),
    bind(Place::Transcript, &[Chord::key("End")], Last, "your last turn"),
    bind(Place::Transcript, &[Chord::key("/"), Chord::key("f").ctrl()], Filter, "find in the transcript"),
    bind(Place::Transcript, &[Chord::key("n")], NextMatch, "next match"),
    bind(Place::Transcript, &[Chord::key("N")], PreviousMatch, "previous match"),
    bind(Place::Transcript, &[Chord::key("y"), Chord::key("Y")], CopyId, "copy the session id"),
    bind(Place::Transcript, &[Chord::key("r"), Chord::key("R")], Resume, "resume in a new pane"),
    bind(Place::Transcript, &[Chord::key("Enter")], Choose, "go to its pane, while it runs"),
    bind(Place::Filter, &[Chord::key("Enter"), Chord::key("Escape")], Leave, "back to the list, filtered"),
    bind(Place::Palette, &[Chord::key("ArrowDown")], Next, "next result"),
    bind(Place::Palette, &[Chord::key("ArrowUp")], Previous, "previous result"),
    bind(Place::Palette, &[Chord::key("Enter")], Choose, "run it, or go to it"),
    bind(Place::Palette, &[Chord::key("Escape")], CloseLayer, "close the palette"),
];

/// What the palette offers to run, in its order, each with the place whose
/// binding it shows as its key.
static COMMANDS: &[(Action, Option<Place>, &str)] = &[
    (Workspace, Some(Anywhere), "Go to the Workspace"),
    (Action::Feature, Some(Anywhere), "Go to the Feature stage"),
    (Action::Checkouts, Some(Anywhere), "Go to the Checkouts stage"),
    (Action::Console, Some(Anywhere), "Go to the Console"),
    (ToggleConsole, Some(Anywhere), "Show or hide the console"),
    (ConsoleFullscreen, Some(Pane), "Console fullscreen beside the sidebar, and back"),
    (NewConsole, None, "Start a console shell in the home directory"),
    (SplitConsole, Some(Pane), "Split the console in two, and back"),
    (OtherSide, Some(Pane), "Give the keys to the split console's other side"),
    (Compose, Some(Anywhere), "Start a shell or an agent…"),
    (Shortcuts, Some(Anywhere), "Show the keyboard shortcuts"),
    (Action::Settings, Some(Anywhere), "Open the settings"),
    (ZoomIn, Some(Window), "Zoom in"),
    (ZoomOut, Some(Window), "Zoom out"),
    (ZoomReset, Some(Window), "Reset the zoom"),
    (Fill, Some(Pane), "Fill the window with the pane, and back"),
    (Close, Some(Pane), "Close the pane, ending its program"),
    (StartShell, Some(CheckoutsTable), "Start a shell on this checkout"),
    (StartAgent, Some(CheckoutsTable), "Start the default agent on this checkout"),
    (OpenReview, Some(CheckoutsTable), "Review this checkout's changes"),
    (OpenHistory, Some(CheckoutsTable), "Read this checkout's history"),
    (OpenSessions, None, "Recent agent sessions of this repository"),
    (Fetch, Some(CheckoutsTable), "Fetch this checkout"),
    (Pull, Some(CheckoutsTable), "Pull this checkout"),
    (Branches, Some(CheckoutsTable), "Show or hide the branches no checkout is on"),
    (Layout, Some(Place::Feature), "Show the features as a list or a board"),
    (Folded, Some(Place::Feature), "Show held and archived features, or fold them"),
    (NewFeature, Some(Place::Feature), "Write a new feature as one note"),
];

const ORDER: [Place; 17] = [
    Anywhere,
    Pane,
    Window,
    Rail,
    Place::Feature,
    Card,
    Outline,
    Writing,
    CheckoutsTable,
    Start,
    Review,
    Jump,
    History,
    Sessions,
    Place::Transcript,
    Place::Filter,
    Place::Palette,
];

/// The table as the sheet lists it: a group per place, in the sheet's order.
pub fn groups() -> Vec<Group> {
    ORDER
        .iter()
        .map(|&place| Group {
            place,
            title: place.title(),
            bindings: BINDINGS.iter().filter(|b| b.place == place).collect(),
        })
        .collect()
}

/// The palette's commands, each with the keys of its binding.
pub fn commands() -> Vec<Command> {
    COMMANDS
        .iter()
        .map(|&(action, place, label)| Command {
            action,
            label,
            place,
            chords: place
                .and_then(|place| BINDINGS.iter().find(|b| b.place == place && b.action == action))
                .map_or(&[], |b| b.chords),
        })
        .collect()
}

/// Whether the palette lists `action` as a command.
pub fn is_command(action: Action) -> bool {
    COMMANDS.iter().any(|&(a, _, _)| a == action)
}

pub fn is_leader(page: &PageKey) -> bool {
    LEADER.matches(page)
}

/// What a key pressed in a pane does, if a binding takes it: one after the
/// leader when `after_leader`, else one the pane keeps from its program.
pub fn in_pane(page: &PageKey, after_leader: bool) -> Option<Action> {
    BINDINGS
        .iter()
        .filter(|b| b.place == Pane)
        .find(|b| b.chords.iter().any(|c| c.leader == after_leader && c.matches(page)))
        .map(|b| b.action)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(key: &str) -> PageKey {
        PageKey { key: key.into(), ..PageKey::default() }
    }

    #[test]
    fn every_binding_has_a_label_and_a_key() {
        for binding in BINDINGS {
            assert!(!binding.label.is_empty(), "{:?}", binding.action);
            assert!(!binding.chords.is_empty(), "{}", binding.label);
        }
    }

    #[test]
    fn every_place_has_a_group_and_every_binding_is_in_one() {
        let groups = groups();
        let listed: usize = groups.iter().map(|g| g.bindings.len()).sum();
        assert_eq!(listed, BINDINGS.len());
        assert!(groups.iter().all(|g| !g.bindings.is_empty()), "an empty heading");
        let titles: Vec<_> = groups.iter().map(|g| g.title).collect();
        assert_eq!(
            titles,
            [
                "Anywhere",
                "In a pane",
                "Everywhere",
                "Sidebar",
                "Feature",
                "Board card",
                "An open feature",
                "Writing in a field",
                "Checkouts",
                "A checkout's + menu",
                "Review",
                "Jumping to a file",
                "History",
                "Recent sessions",
                "A full transcript",
                "A filter or search field",
                "Palette"
            ]
        );
    }

    /// The sheet is the groups as they cross to the page, so a key is on
    /// it exactly when it crossed: each chord of each binding, under the
    /// heading of the place it works in.
    #[test]
    fn every_bound_key_appears_in_the_sheet() {
        let sheet = serde_json::to_value(groups()).unwrap();
        let sheet = sheet.as_array().unwrap();
        for binding in BINDINGS {
            let heading = sheet
                .iter()
                .find(|g| g["title"] == binding.place.title())
                .unwrap_or_else(|| panic!("no heading for {}", binding.label));
            for chord in binding.chords {
                let chord = serde_json::to_value(chord).unwrap();
                let listed = heading["bindings"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|b| b["label"] == binding.label && b["chords"].as_array().unwrap().contains(&chord));
                assert!(listed, "{chord} for {} is missing from {}", binding.label, heading["title"]);
            }
        }
    }

    #[test]
    fn every_command_with_a_place_shows_a_key_bound_there() {
        for command in commands() {
            if command.place.is_some() {
                assert!(!command.chords.is_empty(), "{} has no key in its place", command.label);
            }
        }
        let labels: Vec<_> = commands().iter().map(|c| c.label).collect();
        for (i, label) in labels.iter().enumerate() {
            assert!(!labels[i + 1..].contains(label), "{label} is listed twice");
        }
    }

    /// Ctrl-K and ? reach the palette and the sheet wherever the page has
    /// the keys; in a pane they are the program's until the leader.
    #[test]
    fn the_palette_and_the_sheet_open_from_anywhere_and_after_the_leader() {
        let anywhere = |key: PageKey| {
            BINDINGS
                .iter()
                .filter(|b| b.place == Anywhere)
                .find(|b| b.chords.iter().any(|c| c.matches(&key)))
                .map(|b| b.action)
        };
        let ctrl_k = PageKey { ctrl: true, ..page("k") };
        assert_eq!(anywhere(ctrl_k.clone()), Some(Action::Palette));
        assert_eq!(anywhere(PageKey { shift: true, ..page("?") }), Some(Shortcuts));
        assert_eq!(in_pane(&ctrl_k, false), None, "a shell's kill-line");
        assert_eq!(in_pane(&ctrl_k, true), Some(Action::Palette));
        assert_eq!(in_pane(&page("?"), false), None);
        assert_eq!(in_pane(&page("?"), true), Some(Shortcuts));
    }

    #[test]
    fn the_settings_open_with_ctrl_comma_and_after_the_leader_with_comma() {
        let ctrl_comma = PageKey { ctrl: true, ..page(",") };
        let anywhere = BINDINGS
            .iter()
            .filter(|b| b.place == Anywhere)
            .find(|b| b.chords.iter().any(|c| c.matches(&ctrl_comma)))
            .map(|b| b.action);
        assert_eq!(anywhere, Some(Action::Settings));
        assert_eq!(in_pane(&ctrl_comma, false), None, "the program's");
        assert_eq!(in_pane(&page(","), true), Some(Action::Settings));
        assert!(is_command(Action::Settings), "and the palette lists it");
    }

    /// The page hears Anywhere, Feature and Card together, so a key may
    /// be bound once among them; a pane's keys are a table of their own,
    /// and an open + menu hears only its own and Anywhere's. The palette's
    /// field hears only its own, as every text field does: the rest are
    /// typed into it.
    #[test]
    fn no_key_is_bound_twice_where_it_is_heard() {
        let heard_together: [&[Place]; 14] = [
            &[Anywhere, Place::Feature, Card],
            &[Anywhere, Outline],
            &[Writing],
            &[Pane],
            &[Anywhere, Rail],
            &[Anywhere, CheckoutsTable],
            &[Anywhere, Start],
            &[Anywhere, Review],
            &[Anywhere, History],
            &[Anywhere, Sessions],
            &[Anywhere, Place::Transcript],
            &[Jump],
            &[Place::Filter],
            &[Place::Palette],
        ];
        for places in heard_together {
            let chords: Vec<&Chord> = BINDINGS
                .iter()
                .filter(|b| places.contains(&b.place))
                .flat_map(|b| b.chords)
                .collect();
            for (i, chord) in chords.iter().enumerate() {
                assert!(!chords[i + 1..].contains(chord), "{chord:?} is bound twice");
            }
        }
    }

    /// As in the terminal client's rail: j and k move the pick, s and a
    /// start on its checkout, and a bare x closes a picked pane.
    #[test]
    fn a_picked_rail_row_moves_starts_and_closes_without_the_leader() {
        let rail = |key: &str| {
            BINDINGS
                .iter()
                .filter(|b| b.place == Rail)
                .find(|b| b.chords.iter().any(|c| c.matches(&page(key))))
                .map(|b| b.action)
        };
        assert_eq!(rail("s"), Some(StartShell));
        assert_eq!(rail("a"), Some(StartAgent));
        assert_eq!(rail("x"), Some(Close));
        assert_eq!(rail("Enter"), Some(Choose));
        assert_eq!(rail("j"), Some(Next));
        assert_eq!(rail("ArrowUp"), Some(Previous));
    }

    /// The sessions dialog's keys reach the page in its own group, each
    /// under the action the page sends back, so dispatch, the palette and
    /// the sheet all read them from here.
    #[test]
    fn the_sessions_dialogs_keys_are_listed_in_its_group() {
        let groups = groups();
        let group = groups.iter().find(|g| g.place == Sessions).expect("a Recent sessions heading");
        let action = |key: &str| {
            group.bindings.iter().find(|b| b.chords.iter().any(|c| c.matches(&page(key)))).map(|b| b.action)
        };
        let expected = [
            ("j", Next),
            ("ArrowDown", Next),
            ("k", Previous),
            ("ArrowUp", Previous),
            ("Home", First),
            ("End", Last),
            ("Enter", Choose),
            ("r", Resume),
            ("R", Resume),
            ("o", Transcript),
            ("O", Transcript),
            ("/", Filter),
            ("h", FilterHarness),
            ("H", FilterHarness),
            ("c", FilterCheckout),
            ("C", FilterCheckout),
        ];
        for (key, wanted) in expected {
            assert_eq!(action(key), Some(wanted), "{key}");
        }
        assert_eq!(group.bindings.len(), 10);
        let escape = groups.iter().find(|g| g.place == Anywhere).unwrap();
        assert!(
            escape.bindings.iter().any(|b| b.action == CloseLayer && b.chords.iter().any(|c| c.matches(&page("Escape")))),
            "Escape closes it, as every dialog"
        );
        assert!(commands().iter().any(|c| c.action == OpenSessions), "the palette opens it");
    }

    /// The note and the edits are text fields: Ctrl-Enter saves and
    /// Escape discards, and every other key is typed. An open feature hears
    /// its own keys and Anywhere's, so its j and k move its outline, not
    /// the index under it.
    #[test]
    fn a_note_saves_with_ctrl_enter_and_an_open_feature_has_its_own_keys() {
        let find = |place: Place, key: &PageKey| {
            BINDINGS
                .iter()
                .filter(|b| b.place == place)
                .find(|b| b.chords.iter().any(|c| c.matches(key)))
                .map(|b| b.action)
        };
        assert_eq!(find(Writing, &PageKey { ctrl: true, ..page("Enter") }), Some(Save));
        assert_eq!(find(Writing, &page("Escape")), Some(CloseLayer));
        assert_eq!(find(Place::Feature, &PageKey { shift: true, ..page("N") }), Some(NewFeature));
        assert_eq!(find(Card, &page("Enter")), Some(Choose));
        for (key, action) in [("j", Next), ("e", Edit), ("a", AddTask), ("s", Under), ("d", Decide), ("o", Answer), ("x", Withdraw)] {
            assert_eq!(find(Outline, &page(key)), Some(action), "{key}");
        }
        assert!(commands().iter().any(|c| c.action == NewFeature && !c.chords.is_empty()), "the palette writes one too");
    }

    /// A key the page compared by hand would run without being on the
    /// sheet, so no page module but `keymap` reads a key's name; `input`
    /// hands a pane's keys to Rust whole, which runs them from this table.
    #[test]
    fn the_page_reads_keys_only_through_the_keymap() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/page/src");
        let mut read = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_str().unwrap().to_owned();
            if !name.ends_with(".ts") || name == "keymap.ts" || name == "input.ts" {
                continue;
            }
            read += 1;
            let source = std::fs::read_to_string(&path).unwrap();
            for (n, line) in source.lines().enumerate() {
                // A modifier alone is a click's too (shift extends a pick),
                // so the name of the key is what gives a keypress away.
                let by_hand = line.contains("keyCode")
                    || ["e.key", "event.key"].iter().any(|k| {
                        line.match_indices(k).any(|(i, _)| {
                            !line[..i].ends_with(|c: char| c.is_alphanumeric() || c == '_' || c == '.')
                        })
                    });
                assert!(!by_hand, "{name}:{} reads a key by hand: {}", n + 1, line.trim());
            }
        }
        assert!(read > 10, "read only {read} page modules from {dir}");
    }

    /// The fields' own keys are on the sheet: a message or a comment sends
    /// with Ctrl- or Cmd-Enter, the review's jump steps as in fzf, and a
    /// filter gives its list the keys back.
    #[test]
    fn a_fields_keys_are_listed_where_it_is_typed_in() {
        let find = |place: Place, key: &PageKey| {
            BINDINGS
                .iter()
                .filter(|b| b.place == place)
                .find(|b| b.chords.iter().any(|c| c.matches(key)))
                .map(|b| b.action)
        };
        assert_eq!(find(Writing, &PageKey { meta: true, ..page("Enter") }), Some(Save));
        assert_eq!(find(Writing, &page("Enter")), Some(Choose));
        assert_eq!(find(Jump, &PageKey { ctrl: true, ..page("n") }), Some(Next));
        assert_eq!(find(Jump, &PageKey { ctrl: true, ..page("p") }), Some(Previous));
        assert_eq!(find(Jump, &page("ArrowDown")), Some(Next));
        assert_eq!(find(Jump, &page("Escape")), Some(CloseLayer));
        assert_eq!(find(Jump, &page("n")), None, "a bare n is typed");
        assert_eq!(find(Place::Filter, &page("Enter")), Some(Leave));
        assert_eq!(find(Place::Filter, &page("Escape")), Some(Leave));
        assert_eq!(find(Place::Filter, &page("j")), None, "a j is typed");
    }

    /// The page hears the window's keys before any place, a pane's
    /// program and a text field included, so no place may want them too.
    #[test]
    fn the_zoom_keys_are_the_windows_wherever_the_keys_are() {
        let window: Vec<&Chord> = BINDINGS.iter().filter(|b| b.place == Window).flat_map(|b| b.chords).collect();
        for binding in BINDINGS.iter().filter(|b| b.place != Window) {
            for chord in binding.chords {
                assert!(!window.contains(&chord), "{chord:?} is the window's, not {}'s", binding.label);
            }
        }
        let modifier = |key: &str| {
            let key = page(key);
            if cfg!(target_os = "macos") { PageKey { meta: true, ..key } } else { PageKey { ctrl: true, ..key } }
        };
        let action = |key: PageKey| {
            BINDINGS.iter().filter(|b| b.place == Window).find(|b| b.chords.iter().any(|c| c.matches(&key))).map(|b| b.action)
        };
        assert_eq!(action(modifier("=")), Some(ZoomIn));
        assert_eq!(action(PageKey { shift: true, ..modifier("+") }), Some(ZoomIn));
        assert_eq!(action(modifier("-")), Some(ZoomOut));
        assert_eq!(action(modifier("0")), Some(ZoomReset));
        assert_eq!(action(page("-")), None, "a minus is typed");
        for zoom in [ZoomIn, ZoomOut, ZoomReset] {
            assert!(is_command(zoom), "{zoom:?} is in the palette");
        }
    }

    #[test]
    fn only_a_pane_has_a_leader() {
        for binding in BINDINGS.iter().filter(|b| b.place != Pane) {
            assert!(binding.chords.iter().all(|c| !c.leader), "{}", binding.label);
        }
    }

    #[test]
    fn the_leader_is_ctrl_space_with_or_without_shift() {
        assert!(is_leader(&PageKey { ctrl: true, ..page(" ") }));
        assert!(is_leader(&PageKey { ctrl: true, shift: true, ..page(" ") }));
        assert!(!is_leader(&page(" ")), "a space is typed");
    }

    #[test]
    fn a_pane_keeps_its_keys_until_the_leader() {
        assert_eq!(in_pane(&page("f"), false), None);
        assert_eq!(in_pane(&page("f"), true), Some(Fill));
        assert_eq!(in_pane(&page("2"), true), Some(Action::Feature));
        assert_eq!(in_pane(&page("x"), true), Some(Close));
        assert_eq!(in_pane(&page("x"), false), None, "an x is typed");
        assert_eq!(in_pane(&page("j"), true), None, "the Feature stage's key is not a pane's");
        assert_eq!(in_pane(&page("Tab"), true), Some(OpenReview));
        assert_eq!(in_pane(&page("Tab"), false), None, "a tab is typed");
    }

    /// The console is the fourth stage's key, and from a pane its show,
    /// hide and fullscreen go through the leader like every window key.
    #[test]
    fn the_console_opens_with_4_and_shows_hides_fills_and_splits_after_the_leader() {
        let anywhere = |key: &str| {
            BINDINGS
                .iter()
                .filter(|b| b.place == Anywhere)
                .find(|b| b.chords.iter().any(|c| c.matches(&page(key))))
                .map(|b| b.action)
        };
        assert_eq!(anywhere("4"), Some(Action::Console));
        assert_eq!(anywhere("`"), Some(ToggleConsole));
        assert_eq!(in_pane(&page("4"), true), Some(Action::Console));
        assert_eq!(in_pane(&page("`"), true), Some(ToggleConsole));
        assert_eq!(in_pane(&page("z"), true), Some(ConsoleFullscreen));
        assert_eq!(in_pane(&page("\\"), true), Some(SplitConsole));
        assert_eq!(in_pane(&page("o"), true), Some(OtherSide));
        for key in ["4", "`", "z", "\\", "o"] {
            assert_eq!(in_pane(&page(key), false), None, "{key} is typed");
        }
        for action in [Action::Console, ToggleConsole, ConsoleFullscreen, NewConsole, SplitConsole, OtherSide] {
            assert!(is_command(action), "{action:?} is in the palette");
        }
    }

    #[test]
    fn the_review_jumps_with_ctrl_p_and_opens_the_editor_with_e() {
        let review = |key: &PageKey| {
            BINDINGS
                .iter()
                .filter(|b| b.place == Review)
                .find(|b| b.chords.iter().any(|c| c.matches(key)))
                .map(|b| b.action)
        };
        assert_eq!(review(&PageKey { ctrl: true, ..page("p") }), Some(Filter));
        assert_eq!(review(&page("p")), None, "a bare p is nothing");
        assert_eq!(review(&page("E")), Some(OpenEditor));
    }

    #[test]
    fn a_named_key_asks_for_its_shift_and_a_character_does_not() {
        let shift_page_up = PageKey { shift: true, ..page("PageUp") };
        assert_eq!(in_pane(&shift_page_up, false), Some(PageUp));
        assert_eq!(in_pane(&page("PageUp"), false), None, "the program's own paging key");
        assert!(Chord::key("A").matches(&PageKey { shift: true, ..page("A") }));
        assert!(!Chord::key("A").matches(&PageKey { ctrl: true, ..page("A") }));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn the_paste_chords_are_a_terminals() {
        let ctrl_v = PageKey { ctrl: true, ..page("v") };
        let ctrl_shift_v = PageKey { ctrl: true, shift: true, ..page("V") };
        let alt_v = PageKey { alt: true, ..page("v") };
        for key in [ctrl_v, ctrl_shift_v, alt_v] {
            assert_eq!(in_pane(&key, false), Some(Paste), "{key:?}");
        }
        assert_eq!(in_pane(&PageKey { alt: true, shift: true, ..page("V") }, false), None);
    }

    #[test]
    fn a_group_crosses_to_the_page_with_its_chords_and_labels() {
        let value = serde_json::to_value(&groups()[1]).unwrap();
        assert_eq!(value["place"], "pane");
        assert_eq!(value["title"], "In a pane");
        let fill = &value["bindings"][1];
        assert_eq!(fill["action"], "fill");
        assert_eq!(fill["chords"][0]["key"], "f");
        assert_eq!(fill["chords"][0]["leader"], true);
        assert!(fill.get("place").is_none(), "the group says where");
    }
}
