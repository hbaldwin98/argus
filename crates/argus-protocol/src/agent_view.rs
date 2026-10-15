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

/// One view, as the agent writes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    pub blocks: Vec<Block>,
}

/// What the agent sends to the pane API's `view` endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ViewChange {
    Show { view: AgentView },
    Close { id: String },
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
        blocks(&self.blocks, "blocks", 1)?;
        Ok(AgentView { id: id.to_string(), ..self })
    }

    /// The subject read as a tracker link, its tracker and key out of the
    /// URL as a linked ticket's are.
    pub fn subject_link(&self) -> Option<TrackerLink> {
        let url = self.subject.clone()?;
        crate::tracker_links::LinkWrite { url, tracker: None, key: None }.checked().ok()
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
        Block::Table { columns, rows, sorted_by, .. } => {
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
            Ok(())
        }
        Block::Markdown { .. }
        | Block::List { .. }
        | Block::Stats { .. }
        | Block::Code { .. }
        | Block::Diagram { .. }
        | Block::Callout { .. } => Ok(()),
    }
}

fn value_ok(value: &Value, path: &str) -> Result<(), String> {
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
        AgentView { id: "ci".into(), title: "CI on main".into(), status: None, subject: None, blocks }
    }

    fn text(t: &str) -> Value {
        Value::Text { text: t.into() }
    }

    fn table(rows: Vec<Vec<Value>>) -> Block {
        Block::Table {
            columns: vec![
                Column { title: "Run".into(), numeric: false },
                Column { title: "Took".into(), numeric: true },
            ],
            rows,
            sorted_by: None,
            total: None,
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
