//! What a view an agent shows in the desktop is made of: blocks of what it
//! read with its own tools — a ticket, a run, a query — and the limits a
//! view keeps.
//!
//! The agent translates whatever it read into these blocks and the desktop
//! draws them; neither ever calls a tracker. The agent picks what a thing
//! is and a [`Tone`] for it, never a colour, size or font, and no markup
//! crosses: every string is drawn as text, Markdown included. A view is
//! held in the daemon's memory only, never in the store, so nothing read
//! from a ticket outlives the agent's next look at it.

use serde::{Deserialize, Serialize};

use crate::ids::PaneId;
use crate::tracker_links::TrackerLink;

/// A view's JSON, as the agent sends it. Room for a large ticket with its
/// comments; past it the agent is dumping, not choosing.
pub const MAX_VIEW_BYTES: usize = 256 * 1024;
pub const MAX_VIEW_ID_BYTES: usize = 64;
pub const MAX_VIEW_TITLE_BYTES: usize = 200;
/// A table is something a person scans; past this many rows it is an export.
pub const MAX_TABLE_ROWS: usize = 200;
/// A section in a tab in a column is already as deep as a reader follows.
pub const MAX_BLOCK_DEPTH: usize = 4;
/// Showing one more drops the pane's oldest.
pub const MAX_VIEWS_PER_PANE: usize = 8;
/// Points along a chart's x axis; past it the agent should bucket.
pub const MAX_CHART_POINTS: usize = 500;
/// Series in one chart that can still be told apart by tone and legend.
pub const MAX_CHART_SERIES: usize = 8;
/// Nodes in one tree, counted at every level.
pub const MAX_TREE_NODES: usize = 1000;
pub const MAX_TREE_DEPTH: usize = 12;
/// Nodes in one graph the page lays out itself.
pub const MAX_GRAPH_NODES: usize = 200;
/// Lines a log block holds: a tail, not the whole file.
pub const MAX_LOG_LINES: usize = 1000;
/// Actions in one place — a view, a card, a row, a table — before it is a menu.
pub const MAX_VIEW_ACTIONS: usize = 12;
pub const MAX_FORM_FIELDS: usize = 24;
/// The shortest `refresh_every` honoured: each one asks the agent to read again.
pub const MIN_REFRESH_SECS: u32 = 60;

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
    /// it is held; at least [`MIN_REFRESH_SECS`].
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

/// A button the agent offers. Argus runs nothing for it: pressing it sends
/// the agent a [`ViewPress`], as a refresh is sent, and the view stays as
/// it is until the agent shows it again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ViewAction {
    /// The agent's name for it, sent back on a press.
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub tone: Tone,
    /// Asked of the person, as a yes or no, before the press is sent.
    #[serde(default)]
    pub confirm: Option<String>,
}

/// A press of an action in a view, as the person's client sends it and the
/// agent is told it: which action, on which selected rows, with which
/// answers when it submits a form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ViewPress {
    /// The view's id.
    pub view: String,
    pub action: String,
    /// The `row_keys` of the rows selected in the table the action is on.
    #[serde(default)]
    pub rows: Vec<String>,
    /// The form the action submits, by its id.
    #[serde(default)]
    pub form: Option<String>,
    #[serde(default)]
    pub answers: Vec<Answer>,
}

/// One form field's answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Answer {
    pub name: String,
    pub value: AnswerValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum AnswerValue {
    Text(String),
    Number(f64),
    Bool(bool),
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
    Section { heading: String, blocks: Vec<Block> },
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
}

impl Block {
    /// The block lists directly inside this one, for walking a view.
    pub fn nested(&self) -> Vec<&[Block]> {
        match self {
            Block::Section { blocks, .. } | Block::Card { blocks, .. } | Block::Details { blocks, .. } => {
                vec![blocks]
            }
            Block::Columns { columns } => columns.iter().map(Vec::as_slice).collect(),
            Block::Tabs { tabs } => tabs.iter().map(|t| t.blocks.as_slice()).collect(),
            Block::Grid { items, .. } => vec![items],
            Block::Table { details, .. } => details.iter().flatten().map(Vec::as_slice).collect(),
            _ => Vec::new(),
        }
    }
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
    /// Events sharing a lane are drawn on one line of their own.
    #[serde(default)]
    pub lane: Option<String>,
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub tone: Tone,
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
    Empty,
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
    /// The view if it keeps every limit, or what is wrong with it named by
    /// its path (`blocks[2].rows[0]`), so the agent can find and fix it.
    pub fn checked(self) -> Result<AgentView, String> {
        let id = self.id.trim();
        if id.is_empty() {
            return Err("a view needs an id".into());
        }
        if id.len() > MAX_VIEW_ID_BYTES {
            return Err(format!("an id is at most {MAX_VIEW_ID_BYTES} bytes"));
        }
        if self.title.trim().is_empty() {
            return Err("a view needs a title".into());
        }
        if self.title.len() > MAX_VIEW_TITLE_BYTES {
            return Err(format!("a title is at most {MAX_VIEW_TITLE_BYTES} bytes"));
        }
        if let Some(subject) = &self.subject {
            web_url(subject).map_err(|e| format!("subject: {e}"))?;
        }
        if self.refresh_every.is_some_and(|secs| secs < MIN_REFRESH_SECS) {
            return Err(format!("refresh_every: at least {MIN_REFRESH_SECS} seconds"));
        }
        actions_ok(&self.actions, "actions")?;
        blocks(&self.blocks, "blocks", 1)?;
        Ok(AgentView { id: id.to_string(), ..self })
    }

    /// The subject read as a tracker link, its tracker and key out of the
    /// URL as a linked ticket's are.
    pub fn subject_link(&self) -> Option<TrackerLink> {
        let url = self.subject.clone()?;
        crate::tracker_links::LinkWrite { url, ..Default::default() }.checked().ok()
    }
}

fn blocks(blocks: &[Block], path: &str, depth: usize) -> Result<(), String> {
    if depth > MAX_BLOCK_DEPTH {
        return Err(format!("{path}: blocks nest at most {MAX_BLOCK_DEPTH} deep"));
    }
    for (i, block) in blocks.iter().enumerate() {
        block_ok(block, &format!("{path}[{i}]"), depth)?;
    }
    Ok(())
}

fn block_ok(block: &Block, path: &str, depth: usize) -> Result<(), String> {
    match block {
        Block::Section { blocks: inner, .. } => blocks(inner, &format!("{path}.blocks"), depth + 1),
        Block::Columns { columns } => {
            for (i, column) in columns.iter().enumerate() {
                blocks(column, &format!("{path}.columns[{i}]"), depth + 1)?;
            }
            Ok(())
        }
        Block::Tabs { tabs } => {
            if tabs.is_empty() {
                return Err(format!("{path}: tabs need at least one tab"));
            }
            for (i, tab) in tabs.iter().enumerate() {
                blocks(&tab.blocks, &format!("{path}.tabs[{i}].blocks"), depth + 1)?;
            }
            Ok(())
        }
        Block::Fields { fields } => {
            for (i, field) in fields.iter().enumerate() {
                value_ok(&field.value, &format!("{path}.fields[{i}].value"))?;
            }
            Ok(())
        }
        Block::Table { columns, rows, sorted_by, row_keys, select, actions, details, .. } => {
            if columns.is_empty() {
                return Err(format!("{path}: a table needs a column"));
            }
            if rows.len() > MAX_TABLE_ROWS {
                return Err(format!(
                    "{path}: a table has at most {MAX_TABLE_ROWS} rows; send the first ones and their total"
                ));
            }
            if let Some(sorted) = sorted_by {
                if *sorted >= columns.len() {
                    return Err(format!("{path}.sorted_by: the table has {} columns", columns.len()));
                }
            }
            for (r, row) in rows.iter().enumerate() {
                if row.len() != columns.len() {
                    return Err(format!(
                        "{path}.rows[{r}] has {} cells, the table has {} columns",
                        row.len(),
                        columns.len()
                    ));
                }
                for (c, cell) in row.iter().enumerate() {
                    value_ok(cell, &format!("{path}.rows[{r}][{c}]"))?;
                }
            }
            if !row_keys.is_empty() && row_keys.len() != rows.len() {
                return Err(format!("{path}.row_keys: one per row, the table has {} rows", rows.len()));
            }
            if (*select != RowSelect::None || !actions.is_empty()) && row_keys.is_empty() {
                return Err(format!("{path}: a table that selects rows or acts on them needs row_keys"));
            }
            actions_ok(actions, &format!("{path}.actions"))?;
            if !details.is_empty() && details.len() != rows.len() {
                return Err(format!("{path}.details: one per row, the table has {} rows", rows.len()));
            }
            for (r, detail) in details.iter().enumerate() {
                if let Some(inner) = detail {
                    blocks(inner, &format!("{path}.details[{r}]"), depth + 1)?;
                }
            }
            Ok(())
        }
        Block::Card { blocks: inner, actions, .. } => {
            actions_ok(actions, &format!("{path}.actions"))?;
            blocks(inner, &format!("{path}.blocks"), depth + 1)
        }
        Block::Details { blocks: inner, .. } => blocks(inner, &format!("{path}.blocks"), depth + 1),
        Block::Grid { items, .. } => blocks(items, &format!("{path}.items"), depth + 1),
        Block::Row { items, .. } => {
            let actions = items.iter().filter(|i| matches!(i, RowItem::Action { .. })).count();
            if actions > MAX_VIEW_ACTIONS {
                return Err(format!("{path}: at most {MAX_VIEW_ACTIONS} actions in one place"));
            }
            for (i, item) in items.iter().enumerate() {
                match item {
                    RowItem::Value { value } => value_ok(value, &format!("{path}.items[{i}].value"))?,
                    RowItem::Action { action } => action_ok(action, &format!("{path}.items[{i}].action"))?,
                }
            }
            Ok(())
        }
        Block::Chart { x, series, .. } => {
            if x.len() > MAX_CHART_POINTS {
                return Err(format!("{path}.x: a chart has at most {MAX_CHART_POINTS} points"));
            }
            if series.is_empty() || series.len() > MAX_CHART_SERIES {
                return Err(format!("{path}.series: a chart has 1 to {MAX_CHART_SERIES} series"));
            }
            for (s, one) in series.iter().enumerate() {
                if one.values.len() != x.len() {
                    return Err(format!(
                        "{path}.series[{s}] has {} values, the chart has {} points",
                        one.values.len(),
                        x.len()
                    ));
                }
                if one.values.iter().flatten().any(|v| !v.is_finite()) {
                    return Err(format!("{path}.series[{s}]: values are finite numbers"));
                }
            }
            Ok(())
        }
        Block::Progress { value, max, .. } => progress_ok(*value, *max, path),
        Block::Tree { nodes } => {
            let mut count = 0;
            tree_ok(nodes, &format!("{path}.nodes"), 1, &mut count)
        }
        Block::Graph { nodes, edges, .. } => {
            if nodes.len() > MAX_GRAPH_NODES {
                return Err(format!("{path}.nodes: a graph has at most {MAX_GRAPH_NODES} nodes"));
            }
            let mut ids = std::collections::HashSet::new();
            for (n, node) in nodes.iter().enumerate() {
                if !ids.insert(node.id.as_str()) {
                    return Err(format!("{path}.nodes[{n}]: the id {:?} is used twice", node.id));
                }
            }
            for (e, edge) in edges.iter().enumerate() {
                for end in [&edge.from, &edge.to] {
                    if !ids.contains(end.as_str()) {
                        return Err(format!("{path}.edges[{e}]: no node has the id {end:?}"));
                    }
                }
            }
            Ok(())
        }
        Block::Matrix { rows, columns, cells } => {
            if cells.len() != rows.len() {
                return Err(format!("{path}.cells: one row of cells per row, the matrix has {}", rows.len()));
            }
            for (r, row) in cells.iter().enumerate() {
                if row.len() != columns.len() {
                    return Err(format!(
                        "{path}.cells[{r}] has {} cells, the matrix has {} columns",
                        row.len(),
                        columns.len()
                    ));
                }
                if row.iter().any(|c| !(0.0..=1.0).contains(&c.weight)) {
                    return Err(format!("{path}.cells[{r}]: a weight is from 0 to 1"));
                }
            }
            Ok(())
        }
        Block::Log { lines } if lines.len() > MAX_LOG_LINES => {
            Err(format!("{path}.lines: a log has at most {MAX_LOG_LINES} lines; send the tail"))
        }
        Block::Board { columns } => {
            for (c, column) in columns.iter().enumerate() {
                for (k, card) in column.cards.iter().enumerate() {
                    if let Some(value) = &card.value {
                        value_ok(value, &format!("{path}.columns[{c}].cards[{k}].value"))?;
                    }
                }
            }
            Ok(())
        }
        Block::Form { id, fields, submit } => {
            if id.trim().is_empty() {
                return Err(format!("{path}: a form needs an id"));
            }
            if fields.is_empty() || fields.len() > MAX_FORM_FIELDS {
                return Err(format!("{path}.fields: a form has 1 to {MAX_FORM_FIELDS} fields"));
            }
            let mut names = std::collections::HashSet::new();
            for (f, field) in fields.iter().enumerate() {
                if !names.insert(field.name.as_str()) {
                    return Err(format!("{path}.fields[{f}]: the name {:?} is used twice", field.name));
                }
                if let FormInput::Choice { options, default } = &field.input {
                    if options.is_empty() {
                        return Err(format!("{path}.fields[{f}]: a choice needs options"));
                    }
                    if default.as_ref().is_some_and(|d| !options.contains(d)) {
                        return Err(format!("{path}.fields[{f}]: the default is not one of the options"));
                    }
                }
            }
            action_ok(submit, &format!("{path}.submit"))
        }
        Block::Markdown { .. }
        | Block::List { .. }
        | Block::Stats { .. }
        | Block::Code { .. }
        | Block::Diagram { .. }
        | Block::Callout { .. }
        | Block::Divider
        | Block::Timeline { .. }
        | Block::Log { .. }
        | Block::Checklist { .. }
        | Block::Diff { .. } => Ok(()),
    }
}

fn actions_ok(actions: &[ViewAction], path: &str) -> Result<(), String> {
    if actions.len() > MAX_VIEW_ACTIONS {
        return Err(format!("{path}: at most {MAX_VIEW_ACTIONS} actions in one place"));
    }
    for (i, action) in actions.iter().enumerate() {
        action_ok(action, &format!("{path}[{i}]"))?;
    }
    Ok(())
}

fn action_ok(action: &ViewAction, path: &str) -> Result<(), String> {
    if action.id.trim().is_empty() || action.id.len() > MAX_VIEW_ID_BYTES {
        return Err(format!("{path}: an action's id is 1 to {MAX_VIEW_ID_BYTES} bytes"));
    }
    if action.label.trim().is_empty() {
        return Err(format!("{path}: an action needs a label"));
    }
    Ok(())
}

fn progress_ok(value: f64, max: f64, path: &str) -> Result<(), String> {
    if !(value.is_finite() && max.is_finite() && max > 0.0) {
        return Err(format!("{path}: progress needs a finite value and a max above 0"));
    }
    Ok(())
}

fn tree_ok(nodes: &[TreeNode], path: &str, depth: usize, count: &mut usize) -> Result<(), String> {
    if depth > MAX_TREE_DEPTH {
        return Err(format!("{path}: a tree nests at most {MAX_TREE_DEPTH} deep"));
    }
    for (i, node) in nodes.iter().enumerate() {
        *count += 1;
        if *count > MAX_TREE_NODES {
            return Err(format!("{path}[{i}]: a tree has at most {MAX_TREE_NODES} nodes"));
        }
        if let Some(value) = &node.value {
            value_ok(value, &format!("{path}[{i}].value"))?;
        }
        tree_ok(&node.children, &format!("{path}[{i}].children"), depth + 1, count)?;
    }
    Ok(())
}

pub(crate) fn value_ok(value: &Value, path: &str) -> Result<(), String> {
    if let Value::Progress { value, max, .. } = value {
        return progress_ok(*value, *max, path);
    }
    let url = match value {
        Value::Link { url, .. } => Some(url),
        Value::Ticket { url, .. } => url.as_ref(),
        _ => None,
    };
    match url {
        Some(url) => web_url(url).map_err(|e| format!("{path}: {e}")),
        None => Ok(()),
    }
}

/// Whether a client may hand `url` to the system's opener, which would as
/// happily run a `file:` URL. The daemon refuses any other link in a view,
/// and the client asks again before it opens one.
pub fn is_web_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://"))
        && !url.trim().chars().any(|c| c.is_whitespace() || c.is_control())
}

fn web_url(url: &str) -> Result<(), &'static str> {
    if is_web_url(url) { Ok(()) } else { Err("a link is an http or https URL") }
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

    fn table(rows: Vec<Vec<Value>>) -> Block {
        Block::Table {
            columns: vec![
                Column { title: "Run".into(), numeric: false, optional: false },
                Column { title: "Took".into(), numeric: true, optional: false },
            ],
            rows,
            sorted_by: None,
            total: None,
            row_keys: Vec::new(),
            select: RowSelect::None,
            actions: Vec::new(),
            details: Vec::new(),
        }
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
        let shown = ShownView {
            pane: PaneId(3),
            at: 1_760_000_000,
            subject: None,
            view: view(vec![table(vec![vec![text("4812"), Value::Number { number: 9.5, unit: Some("m".into()) }]])]),
        };
        let bytes = rmp_serde::to_vec_named(&shown).unwrap();
        assert_eq!(rmp_serde::from_slice::<ShownView>(&bytes).unwrap(), shown);
    }

    #[test]
    fn a_row_with_the_wrong_number_of_cells_is_named_by_its_path() {
        let bad = view(vec![Block::Markdown { text: "x".into() }, table(vec![vec![text("4812")]])]);
        assert_eq!(bad.checked().unwrap_err(), "blocks[1].rows[0] has 1 cells, the table has 2 columns");
    }

    #[test]
    fn a_table_past_its_rows_is_refused() {
        let row = || vec![text("a"), text("b")];
        assert!(view(vec![table((0..MAX_TABLE_ROWS).map(|_| row()).collect())]).checked().is_ok());
        let err = view(vec![table((0..=MAX_TABLE_ROWS).map(|_| row()).collect())]).checked().unwrap_err();
        assert!(err.starts_with("blocks[0]: a table has at most 200 rows"), "{err}");
    }

    #[test]
    fn blocks_nest_only_so_deep() {
        let section = |blocks| Block::Section { heading: "s".into(), blocks };
        let deep = |n: usize| (0..n).fold(vec![Block::Markdown { text: "x".into() }], |inner, _| vec![section(inner)]);
        assert!(view(deep(MAX_BLOCK_DEPTH - 1)).checked().is_ok());
        let err = view(deep(MAX_BLOCK_DEPTH)).checked().unwrap_err();
        assert!(err.contains("nest at most 4 deep"), "{err}");
    }

    #[test]
    fn only_a_web_link_is_a_link() {
        let link = |url: &str| view(vec![Block::Fields {
            fields: vec![Field { label: "Doc".into(), value: Value::Link { label: "d".into(), url: url.into() } }],
        }]);
        assert!(link("https://example.com/doc").checked().is_ok());
        let err = link("file:///etc/passwd").checked().unwrap_err();
        assert_eq!(err, "blocks[0].fields[0].value: a link is an http or https URL");
        let mut subject = view(vec![]);
        subject.subject = Some("javascript:alert(1)".into());
        assert!(subject.checked().unwrap_err().starts_with("subject:"));
    }

    #[test]
    fn a_view_needs_an_id_and_a_title() {
        let mut v = view(vec![]);
        v.id = "  ".into();
        assert_eq!(v.checked().unwrap_err(), "a view needs an id");
        let mut v = view(vec![]);
        v.title = String::new();
        assert_eq!(v.checked().unwrap_err(), "a view needs a title");
    }

    #[test]
    fn a_change_reads_as_the_hook_sends_it() {
        let close: ViewChange = serde_json::from_str(r#"{"action":"close","id":"ci"}"#).unwrap();
        assert_eq!(close, ViewChange::Close { id: "ci".into() });
    }
}
