//! What a view an agent shows is made of: blocks of what it read with its
//! own tools — a ticket, a run, a query — and the values inside them.
//!
//! The agent translates whatever it read into these blocks and a client
//! draws them, the desktop window or the terminal alike; neither ever calls
//! a tracker. The agent picks what a thing is and a [`Tone`] for it, never a
//! colour, size or font, and no markup crosses: every string is drawn as
//! text, Markdown included. A view is held in the daemon's memory only,
//! never in the store, so nothing read from a ticket outlives the agent's
//! next look at it. The limits a view keeps are in `view_check`, what a
//! press of its buttons carries in `view_press`, and what the agent is typed
//! of a press or refresh in `view_requests`.

use serde::{Deserialize, Serialize};

use crate::ids::PaneId;
use crate::tracker_links::TrackerLink;
use crate::view_press::ViewAction;

/// One view, as the agent writes it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AgentView {
    /// The agent's name for it. Showing a view with an id the pane already
    /// shows replaces that view whole.
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub status: Option<Chip>,
    /// The ticket this view is of, as its URL. A view with one hangs off
    /// that ticket's link on its feature; one without docks by its pane.
    #[serde(default)]
    pub subject: Option<String>,
    /// The feature, by number, this view belongs with: it is offered from
    /// that feature's header as well as from its pane. Nothing is stored;
    /// the view still goes when its agent or the daemon does.
    #[serde(default)]
    pub feature: Option<i64>,
    /// Ask the agent to read the view again this often, in seconds, while
    /// it is held; at least [`MIN_REFRESH_SECS`](crate::view_check::MIN_REFRESH_SECS).
    #[serde(default)]
    pub refresh_every: Option<u32>,
    /// Buttons in the view's header, for what the whole view offers.
    #[serde(default)]
    pub actions: Vec<ViewAction>,
    pub blocks: Vec<Block>,
}

/// What the agent sends to the pane API's `view` endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ViewChange {
    Show { view: AgentView },
    Close { id: String },
    /// Replaces the one block at `path` (`blocks[2]`,
    /// `blocks[1].columns[0][3]`, the grammar a refusal names blocks by) in
    /// a view the pane holds, so a progress bar or a log tail moves without
    /// the whole view being sent again.
    Patch { id: String, path: String, block: Block },
}

/// A view as the daemon passes it to clients: what the agent wrote, and
/// what only the daemon can say about it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShownView {
    /// The pane whose agent sent it, which a refresh is asked of.
    pub pane: PaneId,
    /// When the daemon received it, in Unix seconds.
    pub at: i64,
    /// The view's `subject`, read as a tracker link.
    pub subject: Option<TrackerLink>,
    pub view: AgentView,
}

/// One part of a view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Block {
    Section {
        heading: String,
        blocks: Vec<Block>,
        /// A saved view this opens, by its id; see [`Value::View`].
        #[serde(default, skip_serializing_if = "Option::is_none")]
        opens: Option<String>,
    },
    /// Side by side; the page stacks them when it is narrow.
    Columns { columns: Vec<Vec<Block>> },
    Tabs { tabs: Vec<Tab> },
    /// Headings, lists, emphasis, inline code, code blocks, links and
    /// quotes. No HTML and no images: the page draws it as text.
    Markdown { text: String },
    Fields { fields: Vec<Field> },
    Table {
        columns: Vec<Column>,
        rows: Vec<Vec<Value>>,
        /// The column the agent sorted the rows by.
        #[serde(default)]
        sorted_by: Option<usize>,
        /// How many rows there were before the agent cut them.
        #[serde(default)]
        total: Option<u64>,
        /// A key per row, sent back with a press so the agent knows which
        /// rows were selected. Needed for `select`; one per row when given.
        #[serde(default)]
        row_keys: Vec<String>,
        #[serde(default)]
        select: RowSelect,
        /// Offered on the selected rows.
        #[serde(default)]
        actions: Vec<ViewAction>,
        /// Blocks a row opens to in place, one entry per row when given.
        #[serde(default)]
        details: Vec<Option<Vec<Block>>>,
    },
    List { items: Vec<Item> },
    Stats { stats: Vec<Stat> },
    Code {
        #[serde(default)]
        lang: Option<String>,
        text: String,
    },
    /// Mermaid source.
    Diagram { source: String },
    Callout { tone: Tone, text: String },
    /// Blocks in a bordered box, with a heading and buttons of their own.
    Card {
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        chip: Option<Chip>,
        #[serde(default)]
        tone: Tone,
        blocks: Vec<Block>,
        #[serde(default)]
        actions: Vec<ViewAction>,
        /// A saved view this opens, by its id; see [`Value::View`].
        #[serde(default, skip_serializing_if = "Option::is_none")]
        opens: Option<String>,
    },
    /// As many blocks as fit side by side at the named minimum width,
    /// wrapping onto rows: a gallery of cards.
    Grid {
        #[serde(default)]
        min: GridMin,
        items: Vec<Block>,
    },
    /// A summary line that opens to its blocks. Which are open is the
    /// page's, and outlives a refresh as a chosen tab does.
    Details {
        summary: String,
        #[serde(default)]
        chip: Option<Chip>,
        #[serde(default)]
        open: bool,
        blocks: Vec<Block>,
        /// A saved view this opens, by its id; see [`Value::View`].
        #[serde(default, skip_serializing_if = "Option::is_none")]
        opens: Option<String>,
    },
    /// Values and buttons on one line, wrapping when narrow.
    Row {
        #[serde(default)]
        align: RowAlign,
        items: Vec<RowItem>,
    },
    Divider,
    /// Series over a shared x axis. The page owns the axes and the scale.
    Chart {
        #[serde(default)]
        shape: ChartShape,
        x: Vec<String>,
        series: Vec<Series>,
        #[serde(default)]
        unit: Option<String>,
    },
    Progress {
        label: String,
        value: f64,
        max: f64,
        #[serde(default)]
        tone: Tone,
        /// Already formatted, beside the label: `412 / 1,000`.
        #[serde(default)]
        note: Option<String>,
    },
    Timeline { events: Vec<TimelineEvent> },
    Tree { nodes: Vec<TreeNode> },
    /// Boxes and arrows the page lays out.
    Graph {
        nodes: Vec<GraphNode>,
        edges: Vec<GraphEdge>,
        #[serde(default)]
        direction: GraphDirection,
    },
    /// A unified diff, drawn as the review viewer draws one.
    Diff {
        #[serde(default)]
        path: Option<String>,
        patch: String,
    },
    /// A grid of tinted cells: a heatmap, a test-by-platform grid.
    Matrix {
        rows: Vec<String>,
        columns: Vec<String>,
        cells: Vec<Vec<MatrixCell>>,
    },
    /// What is done and what is not; the person does not tick it.
    Checklist { items: Vec<CheckItem> },
    Log { lines: Vec<LogLine> },
    Board { columns: Vec<BoardColumn> },
    /// Fields the person fills in, sent back to the agent with `submit`.
    Form {
        id: String,
        fields: Vec<FormField>,
        submit: ViewAction,
    },
    /// Another saved view's blocks, drawn in place. A leaf: the embedded
    /// body is not copied into this view's JSON, so a client resolves `view`
    /// from the saved views it holds, guards a cycle, caps the depth it
    /// follows at [`MAX_EMBED_DEPTH`](crate::saved_view::MAX_EMBED_DEPTH), and
    /// draws a line saying so where the view is missing or too deep. The
    /// saved view may not exist yet; it is drawn as missing until it does.
    Embed { view: String },
    /// A kind this Argus does not know: what a newer one sent. An agent's
    /// typo is refused when the view is checked, but a client decoding what
    /// the daemon sent keeps it, to say it needs a newer Argus.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum RowSelect {
    #[default]
    None,
    One,
    Many,
}

/// A grid item's narrowest width, named rather than measured so the agent
/// never picks pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum GridMin {
    Sm,
    #[default]
    Md,
    Lg,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum RowAlign {
    #[default]
    Start,
    Spread,
    End,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RowItem {
    Value { value: Value },
    Action { action: ViewAction },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum ChartShape {
    #[default]
    Line,
    Bar,
    Area,
    /// A small line with no axes, to sit inline.
    Spark,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Series {
    pub name: String,
    /// One per x; `null` is a gap.
    pub values: Vec<Option<f64>>,
    #[serde(default)]
    pub tone: Tone,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TimelineEvent {
    /// RFC 3339, or the source's own words for when.
    #[serde(default)]
    pub at: Option<String>,
    pub title: String,
    #[serde(default)]
    pub tone: Tone,
    /// Markdown.
    #[serde(default)]
    pub body: Option<String>,
    /// Events sharing a lane are drawn in a column of their own.
    #[serde(default)]
    pub lane: Option<String>,
    /// A saved view this opens, by its id; see [`Value::View`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opens: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TreeNode {
    pub label: String,
    #[serde(default)]
    pub value: Option<Value>,
    #[serde(default)]
    pub chip: Option<Chip>,
    #[serde(default)]
    pub open: bool,
    #[serde(default)]
    pub children: Vec<TreeNode>,
    /// A saved view this opens, by its id; see [`Value::View`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opens: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub tone: Tone,
    /// A saved view this opens, by its id; see [`Value::View`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opens: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum GraphDirection {
    #[default]
    Down,
    Right,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct MatrixCell {
    #[serde(default)]
    pub tone: Tone,
    /// How strongly it is tinted, 0 to 1.
    #[serde(default)]
    pub weight: f64,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CheckItem {
    pub label: String,
    pub done: bool,
    #[serde(default)]
    pub note: Option<String>,
    /// A saved view this opens, by its id; see [`Value::View`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opens: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LogLine {
    #[serde(default)]
    pub at: Option<String>,
    #[serde(default)]
    pub level: LogLevel,
    pub text: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BoardColumn {
    pub title: String,
    #[serde(default)]
    pub chip: Option<Chip>,
    pub cards: Vec<BoardCard>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BoardCard {
    pub title: String,
    /// Markdown.
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub chip: Option<Chip>,
    #[serde(default)]
    pub value: Option<Value>,
    /// A saved view this opens, by its id; see [`Value::View`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opens: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FormField {
    /// The answer's name.
    pub name: String,
    pub label: String,
    pub input: FormInput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FormInput {
    Text {
        #[serde(default)]
        default: Option<String>,
        #[serde(default)]
        placeholder: Option<String>,
        #[serde(default)]
        multiline: bool,
    },
    Number {
        #[serde(default)]
        default: Option<f64>,
        #[serde(default)]
        min: Option<f64>,
        #[serde(default)]
        max: Option<f64>,
    },
    Choice {
        options: Vec<String>,
        #[serde(default)]
        default: Option<String>,
    },
    Toggle {
        #[serde(default)]
        default: bool,
    },
    /// `YYYY-MM-DD`.
    Date {
        #[serde(default)]
        default: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Tab {
    pub title: String,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Field {
    pub label: String,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Column {
    pub title: String,
    /// Aligned right, in figures.
    #[serde(default)]
    pub numeric: bool,
    /// Dropped where the table is too narrow for every column, so the ones
    /// that matter fit without scrolling sideways.
    #[serde(default)]
    pub optional: bool,
}

/// A comment, an event, a commit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Item {
    #[serde(default)]
    pub who: Option<String>,
    /// RFC 3339, as the source wrote it.
    #[serde(default)]
    pub at: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    /// Markdown.
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub chip: Option<Chip>,
    /// A saved view this opens, by its id; see [`Value::View`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opens: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Stat {
    pub label: String,
    /// Already formatted: `4m 12s`, `70%`.
    pub value: String,
    #[serde(default)]
    pub delta: Option<String>,
    #[serde(default)]
    pub tone: Tone,
    /// Recent values, oldest first, drawn as a spark line under the value.
    #[serde(default)]
    pub trend: Vec<f64>,
    /// A saved view this opens, by its id; see [`Value::View`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opens: Option<String>,
}

/// What a field, a table cell or a list item holds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Value {
    Text { text: String },
    Number {
        number: f64,
        #[serde(default)]
        unit: Option<String>,
    },
    Bool { value: bool },
    /// `YYYY-MM-DD`.
    Date { date: String },
    /// RFC 3339.
    DateTime { at: String },
    Person { name: String },
    People { names: Vec<String> },
    Chip { chip: Chip },
    Chips { chips: Vec<Chip> },
    Link { label: String, url: String },
    Ticket {
        key: String,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        url: Option<String>,
        #[serde(default)]
        status: Option<Chip>,
    },
    /// A meter in a cell.
    Progress {
        value: f64,
        max: f64,
        #[serde(default)]
        tone: Tone,
    },
    Duration { ms: u64 },
    /// Inline code.
    Code { text: String },
    // What follows points into Argus rather than out of it: a click opens
    // it in the window, never in a browser. Each is read against the pane
    // that showed the view.
    /// A file in the pane's checkout, relative to its root; opened in the
    /// person's editor.
    Path {
        path: String,
        #[serde(default)]
        line: Option<u32>,
    },
    /// A commit in the pane's checkout; opened in History.
    Commit {
        sha: String,
        #[serde(default)]
        title: Option<String>,
    },
    /// A feature of the pane's repository, by number; opened in the Feature stage.
    Feature { number: i64 },
    /// A pane, by id; focused.
    Pane {
        pane: u64,
        #[serde(default)]
        label: Option<String>,
    },
    /// A saved view, by its id; opened beside the one that links it. The
    /// view may not be saved yet, and is then drawn as missing.
    View {
        id: String,
        #[serde(default)]
        label: Option<String>,
    },
    Empty,
    /// A kind this Argus does not know, as [`Block::Unknown`] is.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Chip {
    pub label: String,
    #[serde(default)]
    pub tone: Tone,
}

/// The only colour an agent chooses, and it chooses a meaning, not a hue.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    #[default]
    Neutral,
    Good,
    Warn,
    Bad,
    Active,
    Info,
}


impl AgentView {
    /// The subject read as a tracker link, its tracker and key out of the
    /// URL as a linked ticket's are.
    pub fn subject_link(&self) -> Option<TrackerLink> {
        let url = self.subject.clone()?;
        crate::tracker_links::LinkWrite { url, ..Default::default() }.checked().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(blocks: Vec<Block>) -> AgentView {
        AgentView { id: "ci".into(), title: "CI on main".into(), blocks, ..Default::default() }
    }

    fn text(t: &str) -> Value {
        Value::Text { text: t.into() }
    }

    #[test]
    fn a_view_reads_from_the_json_an_agent_writes() {
        let json = r#"{
            "id": "arg-142", "title": "Rank recipes", "subject": "https://acme.atlassian.net/browse/ARG-142",
            "status": {"label": "In progress", "tone": "active"},
            "blocks": [
                {"kind": "fields", "fields": [
                    {"label": "Points", "value": {"kind": "number", "number": 5}},
                    {"label": "Severity", "value": {"kind": "empty"}},
                    {"label": "Labels", "value": {"kind": "chips", "chips": [{"label": "search"}]}}
                ]},
                {"kind": "markdown", "text": "**Done when** it ranks"},
                {"kind": "callout", "tone": "warn", "text": "Blocked"}
            ]
        }"#;
        let view: AgentView = serde_json::from_str(json).unwrap();
        let view = view.checked().unwrap();
        assert_eq!(view.status.as_ref().unwrap().tone, Tone::Active);
        let Block::Fields { fields } = &view.blocks[0] else { panic!() };
        assert_eq!(fields[2].value, Value::Chips { chips: vec![Chip { label: "search".into(), tone: Tone::Neutral }] });
        let link = view.subject_link().unwrap();
        assert_eq!((link.tracker.as_str(), link.key.as_str()), ("Jira", "ARG-142"));
    }

    #[test]
    fn a_shown_view_survives_the_wire() {
        let table = Block::Table {
            columns: vec![
                Column { title: "Run".into(), numeric: false, optional: false },
                Column { title: "Took".into(), numeric: true, optional: false },
            ],
            rows: vec![vec![text("4812"), Value::Number { number: 9.5, unit: Some("m".into()) }]],
            sorted_by: None,
            total: None,
            row_keys: Vec::new(),
            select: RowSelect::None,
            actions: Vec::new(),
            details: Vec::new(),
        };
        let shown = ShownView { pane: PaneId(3), at: 1_760_000_000, subject: None, view: view(vec![table]) };
        let bytes = rmp_serde::to_vec_named(&shown).unwrap();
        assert_eq!(rmp_serde::from_slice::<ShownView>(&bytes).unwrap(), shown);
    }

    #[test]
    fn a_kind_from_a_newer_argus_decodes_as_unknown() {
        let json = r#"{"id": "v", "title": "t", "blocks": [
            {"kind": "sparkle", "glitter": 3},
            {"kind": "fields", "fields": [{"label": "x", "value": {"kind": "sparkle", "n": 1}}]}
        ]}"#;
        let view: AgentView = serde_json::from_str(json).unwrap();
        assert_eq!(view.blocks[0], Block::Unknown);
        let Block::Fields { fields } = &view.blocks[1] else { panic!() };
        assert_eq!(fields[0].value, Value::Unknown);
        // A client keeps it across the daemon's wire, to say it needs a newer Argus.
        let shown = ShownView { pane: PaneId(1), at: 0, subject: None, view };
        let bytes = rmp_serde::to_vec_named(&shown).unwrap();
        let back = rmp_serde::from_slice::<ShownView>(&bytes).unwrap();
        assert_eq!(back.view.blocks[0], Block::Unknown);
    }

    #[test]
    fn a_change_reads_as_the_hook_sends_it() {
        let close: ViewChange = serde_json::from_str(r#"{"action":"close","id":"ci"}"#).unwrap();
        assert_eq!(close, ViewChange::Close { id: "ci".into() });
    }
}
