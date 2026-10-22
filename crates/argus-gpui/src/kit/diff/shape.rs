//! What a row is and how tall it is, from the kind the session gave it.
//!
//! The rows are `argus-desktop-core`'s (`DiffRow`): which line is picked, which one holds the
//! cursor, what a fold hides and what the highlighter cut are all Rust's. This is only what the
//! page's `diffRow` computed on the way to the DOM, so the view draws and never decides.

use argus_desktop_core::connection::page::DiffRow;

/// A line, and the mono text in it: `.dbody`'s `line-height: 20px`.
pub const LINE: f32 = 20.0;
/// A hunk header or a note: a line on the chrome grey.
pub const HUNK: f32 = 20.0;
/// The marker where a file read whole lost lines: a line and the rule above it (`.cgone`).
pub const GONE: f32 = 21.0;
/// A fold: its words and two buttons between dashed rules, `--row-h-sm`.
pub const FOLD: f32 = 26.0;

/// What a row is, as `DiffRow::kind` says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Hunk,
    Note,
    Fold,
    /// One line, with an old and a new number.
    Line,
    /// Two halves, each with its own one number.
    Pair,
}

pub fn shape(row: &DiffRow) -> Shape {
    match row.kind {
        "hunk" => Shape::Hunk,
        "fold" => Shape::Fold,
        "line" => Shape::Line,
        "pair" => Shape::Pair,
        // A kind this window does not know is said as text rather than dropped.
        _ => Shape::Note,
    }
}

pub fn height(row: &DiffRow) -> f32 {
    match shape(row) {
        Shape::Hunk | Shape::Note => HUNK,
        Shape::Fold => FOLD,
        Shape::Line | Shape::Pair => LINE,
    }
}

/// A pair of `lines` lines tall: a single line is a row; more take their
/// lines whole and the row's two border pixels besides.
pub fn pair_height(lines: usize) -> f32 {
    if lines <= 1 {
        LINE
    } else {
        lines as f32 * LINE + 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::diff::tests::row;

    #[test]
    fn a_row_is_as_tall_as_what_it_is() {
        assert_eq!(height(&row("hunk")), HUNK);
        assert_eq!(height(&row("note")), HUNK);
        assert_eq!(height(&row("fold")), FOLD);
        assert_eq!(height(&row("line")), LINE);
        assert_eq!(height(&row("pair")), LINE);
        assert_eq!(shape(&row("something new")), Shape::Note);
    }
}
