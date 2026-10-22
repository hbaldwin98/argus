//! Which rows of a table a filter lets through, and the words a table says about how many it shows.
//!
//! A row matches when its cells' text, lowercased and joined by newlines, holds what was typed,
//! trimmed and lowercased (TABLE-3).

use argus_protocol::Value;

use crate::views::values::cell_text;

/// Past this many rows a table offers a filter.
pub const FILTERABLE: usize = 8;

/// What a filter matches a row by.
pub fn row_text(cells: &[Value]) -> String {
    cells.iter().map(cell_text).collect::<Vec<_>>().join("\n").to_lowercase()
}

/// Whether the row whose text is `text` is let through by `typed`; no filter lets every row through.
pub fn matches(text: &str, typed: &str) -> bool {
    let needle = typed.trim().to_lowercase();
    needle.is_empty() || text.contains(&needle)
}

/// `3 of 40 rows` while a filter is typed, and nothing otherwise.
pub fn count_line(shown: usize, rows: usize, typed: &str) -> String {
    if typed.trim().is_empty() {
        String::new()
    } else {
        format!("{shown} of {rows} rows")
    }
}

/// `Showing 40 of 212 rows`, when the agent cut the rows it sent.
pub fn showing(rows: usize, total: Option<u64>) -> Option<String> {
    total.filter(|total| *total > rows as u64).map(|total| format!("Showing {rows} of {total} rows"))
}

#[cfg(test)]
mod tests;
