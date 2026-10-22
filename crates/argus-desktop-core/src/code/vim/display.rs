//! Display lines. When the window wraps a long line, `gj` and `gk` move by
//! the rows it is wrapped into, and `g0`, `g^` and `g$` go to a row's ends.
//! The window says how many characters a row holds (`Vim::set_wrap`); with
//! no width given a line is one row, and these are `j`, `k`, the smart
//! Home and the line's last character.

use super::text::Text;

/// The row a column is in, and the column within it.
fn split(col: usize, width: Option<usize>) -> (usize, usize) {
    match width {
        Some(w) => (col / w, col % w),
        None => (0, col),
    }
}

/// `gj` and `gk`: where a row down or up from `from` is, keeping the column
/// within the row `want` says; none at the text's ends.
pub fn vertical(t: Text, from: usize, forward: bool, width: Option<usize>, want: usize) -> Option<usize> {
    let line = t.line_of(from);
    let len = t.line_len(line);
    let (row, _) = split(t.col(from), width);
    let in_row = split(want, width).1;
    let row_len = width.unwrap_or(usize::MAX);
    let place = |ln: usize, row: usize| {
        let len = t.line_len(ln);
        let col = row.saturating_mul(row_len).saturating_add(in_row);
        t.at(ln, col.min(len.saturating_sub(1)))
    };
    if forward {
        if width.is_some_and(|w| (row + 1) * w < len) {
            return Some(place(line, row + 1));
        }
        if line >= t.last_line() {
            return None;
        }
        Some(place(line + 1, 0))
    } else {
        if row > 0 {
            return Some(place(line, row - 1));
        }
        if line == 0 {
            return None;
        }
        let prev = t.line_len(line - 1);
        let last_row = width.map_or(0, |w| prev.saturating_sub(1) / w);
        Some(place(line - 1, last_row))
    }
}

/// `g0` and `g^`: the start of the row. On a line's first row this is the
/// editor's smart Home: the first character that is not blank, or if the
/// cursor is there already, the line's start.
pub fn row_start(t: Text, from: usize, width: Option<usize>) -> usize {
    let line = t.line_of(from);
    let (row, _) = split(t.col(from), width);
    if row > 0 {
        return t.at(line, row * width.unwrap_or(usize::MAX));
    }
    let start = t.line_start(line);
    let indent = t.first_nonblank(line);
    if t.line_start(line) != t.line_end(line) && indent > start && from != indent {
        indent
    } else {
        start
    }
}

/// `g$`: the last character on the row.
pub fn row_end(t: Text, from: usize, width: Option<usize>) -> usize {
    let line = t.line_of(from);
    let len = t.line_len(line);
    let (row, _) = split(t.col(from), width);
    let end = width.map_or(len, |w| ((row + 1) * w).min(len));
    t.at(line, end.saturating_sub(1))
}

#[cfg(test)]
mod tests;
