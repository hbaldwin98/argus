//! What an extension's table says once a row's status leads it: the cells that only repeat the
//! status blanked, the columns left with nothing to say dropped, and the "Status" column in front.
//!
//! The page did this in `extensions/status_echo.ts` and `views.ts` (`asBlock`); both windows draw
//! from this one answer, so "exited (137)" beside an "exited 137" cell reads the same in each.

use argus_protocol::{Chip, Column, Value};

use super::views::{ExtTable, ExtTableRow};

/// The text with case, spacing and punctuation taken out: a status and its echo differ in them
/// ("exited (137)" repeats "exited 137").
fn plain(text: &str) -> String {
    let mut out = String::new();
    let mut gap = false;
    for c in text.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            if gap && !out.is_empty() {
                out.push(' ');
            }
            gap = false;
            out.push(c);
        } else {
            gap = true;
        }
    }
    out
}

/// Whether `cell` only repeats the status chip of `row`.
pub fn echoes(row: &ExtTableRow, cell: Option<&Value>) -> bool {
    let word = row.status.as_ref().map(|status| plain(&status.label)).unwrap_or_default();
    if word.is_empty() {
        return false;
    }
    let text = match cell {
        Some(Value::Text { text }) => text,
        Some(Value::Chip { chip: Chip { label, .. } }) => label,
        _ => return false,
    };
    plain(text) == word
}

/// An extension's table as the agent table's renderer draws it.
#[derive(Debug, Clone, PartialEq)]
pub struct Shaped {
    /// A leading "Status" column holds each row's chip.
    pub status: bool,
    pub columns: Vec<Column>,
    /// Each row's cells, the status first when `status`; a blank is `Value::Empty`.
    pub rows: Vec<Vec<Value>>,
}

/// `view`'s table with a row's status leading it, when any row has one.
pub fn shape(view: &ExtTable) -> Shaped {
    let status = view.rows.iter().any(|row| row.status.is_some());
    // A column whose every cell echoes its row's status says nothing; one cell that differs keeps it.
    let kept: Vec<bool> =
        (0..view.columns.len()).map(|c| !status || view.rows.is_empty() || !view.rows.iter().all(|row| echoes(row, row.cells.get(c)))).collect();
    let lead = |row: &ExtTableRow| -> Option<Value> {
        status.then(|| row.status.clone().map_or(Value::Empty, |chip| Value::Chip { chip }))
    };
    let mut columns = Vec::new();
    if status {
        columns.push(Column { title: "Status".into(), numeric: false, optional: false });
    }
    columns.extend(view.columns.iter().zip(&kept).filter(|(_, kept)| **kept).map(|(column, _)| column.clone()));
    let rows = view
        .rows
        .iter()
        .map(|row| {
            let cells = row.cells.iter().enumerate().filter(|(c, _)| kept.get(*c).copied().unwrap_or(false));
            lead(row)
                .into_iter()
                .chain(cells.map(|(_, cell)| if echoes(row, Some(cell)) { Value::Empty } else { cell.clone() }))
                .collect()
        })
        .collect();
    Shaped { status, columns, rows }
}

#[cfg(test)]
mod tests;
