//! The limits a view keeps, and the check that holds an agent's view to
//! them.
//!
//! A refusal names the offending part by its path (`blocks[2].rows[0]`), in
//! the grammar a patch reads, so the agent can find it and fix it. The
//! limits live here with the check, not with the types, so a number and the
//! code that enforces it change together. What the check lets through is
//! what the clients draw without guarding: finite numbers they can scale to,
//! text without terminal escapes in the ids that are echoed to one, and
//! kinds this Argus knows.

use std::collections::HashSet;

use crate::agent_view::{AgentView, Block, FormInput, RowItem, RowSelect, TreeNode, Value};
use crate::checkout_path::is_checkout_relative;
use crate::view_press::ViewAction;

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
/// Edges in one graph; the layout is quadratic in what it joins.
pub const MAX_GRAPH_EDGES: usize = 1000;
/// Lines a log block holds: a tail, not the whole file.
pub const MAX_LOG_LINES: usize = 1000;
/// Actions in one place — a view, a card, a row, a table — before it is a menu.
pub const MAX_VIEW_ACTIONS: usize = 12;
pub const MAX_FORM_FIELDS: usize = 24;
/// Values in a stat's trend: a spark line under a figure, not a chart.
pub const MAX_TREND_POINTS: usize = 500;
/// The largest magnitude of any number in a view. A finite value near the
/// top of `f64` is still finite, and a chart that scales an axis to it, or
/// steps along it, never finishes; no figure a person reads is this big.
pub const MAX_VIEW_NUMBER: f64 = 1e15;
/// The shortest `refresh_every` honoured: each one asks the agent to read again.
pub const MIN_REFRESH_SECS: u32 = 60;

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
        if self.id.chars().any(char::is_control) {
            return Err("id: no control characters".into());
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
        let mut actions = HashSet::new();
        actions_ok(&self.actions, "actions", &mut actions)?;
        let name = |i: usize| format!("blocks[{i}]");
        list_ok(&self.blocks, "blocks", &name, 1, &mut actions)?;
        Ok(AgentView { id: id.to_string(), ..self })
    }
}

/// The action ids met so far, which a press is told apart by.
type Actions = HashSet<String>;

fn list_ok(
    list: &[Block],
    list_path: &str,
    name: &dyn Fn(usize) -> String,
    depth: usize,
    actions: &mut Actions,
) -> Result<(), String> {
    if depth > MAX_BLOCK_DEPTH {
        return Err(format!("{list_path}: blocks nest at most {MAX_BLOCK_DEPTH} deep"));
    }
    for (i, block) in list.iter().enumerate() {
        block_ok(block, &name(i), depth, actions)?;
    }
    Ok(())
}

fn block_ok(block: &Block, path: &str, depth: usize, actions: &mut Actions) -> Result<(), String> {
    own_ok(block, path, actions)?;
    for (slot, list) in block.children() {
        let name = |i: usize| format!("{path}{}", slot.patch_path(i));
        list_ok(list, &format!("{path}{}", slot.patch_step()), &name, depth + 1, actions)?;
    }
    Ok(())
}

/// What a block holds itself, not the blocks inside it.
fn own_ok(block: &Block, path: &str, actions: &mut Actions) -> Result<(), String> {
    match block {
        Block::Unknown => Err(format!("{path}: no block kind by that name")),
        Block::Tabs { tabs } if tabs.is_empty() => Err(format!("{path}: tabs need at least one tab")),
        Block::Fields { fields } => {
            for (i, field) in fields.iter().enumerate() {
                value_ok(&field.value, &format!("{path}.fields[{i}].value"))?;
            }
            Ok(())
        }
        Block::Stats { stats } => {
            for (i, stat) in stats.iter().enumerate() {
                if stat.trend.len() > MAX_TREND_POINTS {
                    return Err(format!("{path}.stats[{i}].trend: a trend has at most {MAX_TREND_POINTS} points"));
                }
                for (t, point) in stat.trend.iter().enumerate() {
                    bounded(*point, &format!("{path}.stats[{i}].trend[{t}]"))?;
                }
            }
            Ok(())
        }
        Block::Table { columns, rows, sorted_by, row_keys, select, actions: own, details, .. } => {
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
            for (k, key) in row_keys.iter().enumerate() {
                no_control(key, &format!("{path}.row_keys[{k}]"))?;
            }
            if (*select != RowSelect::None || !own.is_empty()) && row_keys.is_empty() {
                return Err(format!("{path}: a table that selects rows or acts on them needs row_keys"));
            }
            actions_ok(own, &format!("{path}.actions"), actions)?;
            if !details.is_empty() && details.len() != rows.len() {
                return Err(format!("{path}.details: one per row, the table has {} rows", rows.len()));
            }
            Ok(())
        }
        Block::Card { actions: own, .. } => actions_ok(own, &format!("{path}.actions"), actions),
        Block::Row { items, .. } => {
            let count = items.iter().filter(|i| matches!(i, RowItem::Action { .. })).count();
            if count > MAX_VIEW_ACTIONS {
                return Err(format!("{path}: at most {MAX_VIEW_ACTIONS} actions in one place"));
            }
            for (i, item) in items.iter().enumerate() {
                match item {
                    RowItem::Value { value } => value_ok(value, &format!("{path}.items[{i}].value"))?,
                    RowItem::Action { action } => action_ok(action, &format!("{path}.items[{i}].action"), actions)?,
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
                for (v, value) in one.values.iter().enumerate() {
                    if let Some(value) = value {
                        bounded(*value, &format!("{path}.series[{s}].values[{v}]"))?;
                    }
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
            if edges.len() > MAX_GRAPH_EDGES {
                return Err(format!("{path}.edges: a graph has at most {MAX_GRAPH_EDGES} edges"));
            }
            let mut ids = HashSet::new();
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
                // Also refuses NaN and anything past the view bound, which
                // are outside 0 to 1 as well.
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
            no_control(id, &format!("{path}.id"))?;
            if fields.is_empty() || fields.len() > MAX_FORM_FIELDS {
                return Err(format!("{path}.fields: a form has 1 to {MAX_FORM_FIELDS} fields"));
            }
            let mut names = HashSet::new();
            for (f, field) in fields.iter().enumerate() {
                let at = format!("{path}.fields[{f}]");
                no_control(&field.name, &format!("{at}.name"))?;
                if !names.insert(field.name.as_str()) {
                    return Err(format!("{at}: the name {:?} is used twice", field.name));
                }
                match &field.input {
                    FormInput::Choice { options, default } => {
                        if options.is_empty() {
                            return Err(format!("{at}: a choice needs options"));
                        }
                        if default.as_ref().is_some_and(|d| !options.contains(d)) {
                            return Err(format!("{at}: the default is not one of the options"));
                        }
                    }
                    FormInput::Number { default, min, max } => {
                        for (what, n) in [("default", default), ("min", min), ("max", max)] {
                            if let Some(n) = n {
                                bounded(*n, &format!("{at}.{what}"))?;
                            }
                        }
                    }
                    _ => {}
                }
            }
            action_ok(submit, &format!("{path}.submit"), actions)
        }
        // Containers are walked through their children; the rest have no
        // limit of their own to keep.
        Block::Section { .. }
        | Block::Columns { .. }
        | Block::Tabs { .. }
        | Block::Details { .. }
        | Block::Grid { .. }
        | Block::Markdown { .. }
        | Block::List { .. }
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

fn actions_ok(list: &[ViewAction], path: &str, ids: &mut Actions) -> Result<(), String> {
    if list.len() > MAX_VIEW_ACTIONS {
        return Err(format!("{path}: at most {MAX_VIEW_ACTIONS} actions in one place"));
    }
    for (i, action) in list.iter().enumerate() {
        action_ok(action, &format!("{path}[{i}]"), ids)?;
    }
    Ok(())
}

fn action_ok(action: &ViewAction, path: &str, ids: &mut Actions) -> Result<(), String> {
    if action.id.trim().is_empty() || action.id.len() > MAX_VIEW_ID_BYTES {
        return Err(format!("{path}: an action's id is 1 to {MAX_VIEW_ID_BYTES} bytes"));
    }
    no_control(&action.id, &format!("{path}.id"))?;
    if action.label.trim().is_empty() {
        return Err(format!("{path}: an action needs a label"));
    }
    no_control(&action.label, &format!("{path}.label"))?;
    // A press names its action by id alone, so two with one id could not be
    // told apart, and the agent would be told of the wrong one.
    if !ids.insert(action.id.clone()) {
        return Err(format!("{path}: the action id {:?} is used twice in the view", action.id));
    }
    Ok(())
}

/// Ids, names and labels are echoed to the agent's terminal.
fn no_control(text: &str, path: &str) -> Result<(), String> {
    if text.chars().any(char::is_control) {
        return Err(format!("{path}: no control characters"));
    }
    Ok(())
}

fn bounded(n: f64, path: &str) -> Result<(), String> {
    if n.is_nan() || n.abs() > MAX_VIEW_NUMBER {
        return Err(format!("{path}: a number is finite and at most {MAX_VIEW_NUMBER:e} in magnitude"));
    }
    Ok(())
}

fn progress_ok(value: f64, max: f64, path: &str) -> Result<(), String> {
    bounded(value, &format!("{path}.value"))?;
    bounded(max, &format!("{path}.max"))?;
    if max <= 0.0 {
        return Err(format!("{path}: progress needs a max above 0"));
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
    match value {
        Value::Unknown => Err(format!("{path}: no value kind by that name")),
        Value::Number { number, .. } => bounded(*number, path),
        Value::Progress { value, max, .. } => progress_ok(*value, *max, path),
        Value::Path { path: file, .. } if !is_checkout_relative(file) => {
            Err(format!("{path}: a path is inside the checkout: relative, with no `..`"))
        }
        Value::Link { url, .. } => web_url(url).map_err(|e| format!("{path}: {e}")),
        Value::Ticket { url: Some(url), .. } => web_url(url).map_err(|e| format!("{path}: {e}")),
        _ => Ok(()),
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
mod tests;
