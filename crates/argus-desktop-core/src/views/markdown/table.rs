//! A pipe table: a header line, a separator line, and rows while the lines hold a pipe.
//!
//! Cells split on a pipe that is not escaped (`\|` is a pipe in the text); the pipe at either end
//! of a line is dropped; every row is padded or cut to the header's width. A cell is read for its
//! marks, one level in.

use super::inline::parse_nested;
use super::line;
use super::model::Table;

/// The cells of one line.
pub fn cells(row: &str) -> Vec<String> {
    let row: Vec<char> = row.trim().chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut cell = String::new();
    let mut i = 0;
    while i < row.len() {
        if row[i] == '\\' && row.get(i + 1) == Some(&'|') {
            cell.push('|');
            i += 1;
        } else if row[i] == '|' {
            out.push(cell.trim().to_owned());
            cell.clear();
        } else {
            cell.push(row[i]);
        }
        i += 1;
    }
    out.push(cell.trim().to_owned());
    if row.first() == Some(&'|') {
        out.remove(0);
    }
    let escaped_end = row.len() >= 2 && row[row.len() - 2] == '\\';
    if row.last() == Some(&'|') && !escaped_end {
        out.pop();
    }
    out
}

/// Whether the line at `from` begins a table: a header with a pipe over a separator with one.
pub fn begins(lines: &[String], from: usize) -> bool {
    lines[from].contains('|') && lines.get(from + 1).is_some_and(|next| line::is_table_separator(next) && next.contains('|'))
}

/// The table that begins at line `from`, and the line after it.
pub fn read(lines: &[String], from: usize) -> (Table, usize) {
    let head = cells(&lines[from]);
    let width = head.len();
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut i = from + 2;
    while i < lines.len() && !line::is_blank(&lines[i]) && lines[i].contains('|') {
        rows.push(cells(&lines[i]));
        i += 1;
    }
    let shape = |values: &[String]| (0..width).map(|k| parse_nested(values.get(k).map_or("", String::as_str))).collect();
    (Table { head: shape(&head), rows: rows.iter().map(|row| shape(row)).collect() }, i)
}
