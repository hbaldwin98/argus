//! What the page and the window's Rust say to each other.
//!
//! Everything here crosses into `assets/app.js` as JSON, so a field's name
//! is a contract with the script that reads it: the tests pin each shape.
//! Rust sends events down one channel; the page's calls are in `commands`.
//! `cargo test` writes each type as TypeScript into `page/src/bindings`, so
//! the page is compiled against these definitions rather than a copy.

use serde::{Deserialize, Serialize};

use crate::keymap::Group;

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
    /// The review dialog, whole, whenever it changes; `open` false once
    /// it closes.
    Review(Box<ReviewPage>),
    /// The history dialog, whole, whenever it changes; `open` false once
    /// it closes.
    History(Box<HistoryPage>),
    /// Text a pane's child put on the clipboard (OSC 52).
    Clipboard { text: String },
    /// Something the daemon said that passes: a refused request's reason.
    /// The page shows it a while and lets it go.
    Notice { text: String },
    /// An agent came to need the person somewhere they are not looking.
    Chime,
    /// Every binding, grouped as the shortcuts sheet lists them, sent once
    /// a page loads: the page dispatches from it outside a pane.
    Keymap { groups: Vec<Group> },
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
    /// `workspace` or `feature`.
    #[cfg_attr(test, ts(type = "\"workspace\" | \"feature\" | \"checkouts\""))]
    pub stage: &'static str,
    /// The pane fills the window: the rail and the stage's chrome are put
    /// away until the leader and `f` again.
    pub filled: bool,
    /// The agents a checkout's + offers beside a shell: the daemon's
    /// templates, the first its default.
    pub templates: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ProjectChoice {
    pub id: u64,
    pub name: String,
    pub needs_you: usize,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Link {
    pub tracker: String,
    pub key: String,
    pub url: String,
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

/// The rows of a pane's screen that changed since the last frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    pub pane: u64,
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
    pub loading: bool,
    /// Why there are no rows: no commits yet, or a failed read.
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn as_json(event: Event) -> serde_json::Value {
        serde_json::to_value(event).unwrap()
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
                "type": "frame", "pane": 4, "rows": 2, "cols": 10, "full": true,
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
            projects: vec![ProjectChoice { id: 1, name: "src".into(), needs_you: 2 }],
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
                        panes: vec![Pane {
                            id: 4,
                            agent: true,
                            title: "claude".into(),
                            status: "working",
                            note: None,
                            on: true,
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
            }),
            loaded: true,
            repository: Some(2),
            stage: "workspace",
            filled: false,
            templates: vec!["claude".into()],
        };
        let value = as_json(Event::Rail(Box::new(rail)));
        assert_eq!(value["type"], "rail");
        assert_eq!(value["needsYou"], 2);
        assert_eq!(value["projects"][0]["needsYou"], 2);
        let repo = &value["project"]["repositories"][0];
        assert_eq!(repo["checkouts"][0]["panes"][0]["status"], "working");
        assert_eq!(repo["checkouts"][0]["git"], "clean");
        assert_eq!(value["shown"]["pane"], 4);
        assert_eq!(value["loaded"], true);
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
}
