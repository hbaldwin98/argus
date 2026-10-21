//! What one diff row shows, as plain data: how tall it is, which number each
//! half carries, the text of a line with its tabs stopped and its highlight
//! kinds cut into spans, and where a comment box sits among the rows.
//!
//! The rows are `argus-desktop-core`'s (`DiffRow`): which line is picked, which
//! one holds the cursor, what a fold hides and what the highlighter cut are all
//! Rust's. This is only what the page's `diffRow` computed on the way to the
//! DOM, so `view` draws and never decides.

use std::ops::Range;

use argus_desktop_core::connection::page::{DiffRow, DiffSide};
use gpui_kit::Hsla;
use unicode_width::UnicodeWidthChar as _;

use crate::theme::Ledger;

/// Where a tab stops. The page leaves the browser's 8; 4 keeps an indented
/// line inside a row a half of a split view holds.
pub const TAB: usize = 4;
/// A line, and the mono text in it.
pub const LINE: f32 = 22.0;
/// A hunk header or a note: a little more air, on the chrome grey.
pub const HUNK: f32 = 26.0;
/// A fold: its words and two buttons between dashed rules.
pub const FOLD: f32 = 30.0;

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

/// The numbers a side carries: both columns of a unified line, or the one a
/// half of a split pair shows (the new for an added line, else the old, else
/// the new).
pub fn numbers(side: Option<&DiffSide>, both: bool) -> Vec<Option<u32>> {
    if both {
        vec![side.and_then(|s| s.old), side.and_then(|s| s.new)]
    } else {
        vec![side.and_then(|s| if s.kind == "added" { s.new } else { s.old.or(s.new) })]
    }
}

/// A line's text as it is drawn, and the highlight kind of each stretch of it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Code {
    pub text: String,
    /// Byte ranges of `text`, in order and apart, each with its kind: `k`,
    /// `s`, `c`, `n`, `t`, `f`, `p` or `o`.
    pub spans: Vec<(Range<usize>, &'static str)>,
}

/// The side's parts as one string. A tab becomes the spaces up to the next
/// stop, counted in cells so a wide glyph (two cells) before it moves the
/// stop as it would in a terminal; a carriage return is dropped, since a
/// CRLF file would otherwise draw a box at every line's end; any other
/// control character is shown as U+FFFD rather than handed to the shaper.
pub fn code(side: &DiffSide) -> Code {
    let mut out = Code::default();
    let mut column = 0usize;
    for (text, kind) in &side.parts {
        let start = out.text.len();
        for c in text.chars() {
            match c {
                '\t' => {
                    let n = TAB - column % TAB;
                    out.text.extend(std::iter::repeat_n(' ', n));
                    column += n;
                }
                '\r' => {}
                c if c.is_control() => {
                    out.text.push('\u{fffd}');
                    column += 1;
                }
                c => {
                    out.text.push(c);
                    column += c.width().unwrap_or(0);
                }
            }
        }
        if !kind.is_empty() && out.text.len() > start {
            out.spans.push((start..out.text.len(), kind));
        }
    }
    out
}

/// How a highlight kind is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tone {
    pub colour: Hsla,
    pub italic: bool,
}

/// The colour of a highlight kind, from the Ledger's roles (the page's
/// `--mauve`, `--green`, `--peach`, `--yellow`, `--blue` and `--teal`).
pub fn tone(kind: &str, ledger: &Ledger) -> Option<Tone> {
    let (colour, italic) = match kind {
        "k" => (ledger.accent, false),
        "s" => (ledger.success, false),
        "c" => (ledger.faint, true),
        "n" => (ledger.changed, false),
        "t" => (ledger.warning, false),
        "f" => (ledger.link, false),
        "p" => (ledger.info, false),
        "o" => (ledger.muted, false),
        _ => return None,
    };
    Some(Tone { colour, italic })
}

/// The wash behind a side by what it is: added green, removed red, a
/// context line none.
pub fn wash(kind: &str, ledger: &Ledger) -> Option<Hsla> {
    match kind {
        "added" => Some(ledger.success.opacity(0.12)),
        "removed" => Some(ledger.danger.opacity(0.12)),
        _ => None,
    }
}

/// What a side is filled with: its wash, with the pick laid over it rather
/// than in its place, so a picked line's side stays readable, or the cursor's
/// faint lift. Composed over the window's background by hand: `Hsla::blend`
/// keeps its receiver's alpha, so a see-through wash blended with a
/// see-through pick would paint nothing.
pub fn fill(kind: &str, picked: bool, cursor: bool, ledger: &Ledger) -> Option<Hsla> {
    let wash = wash(kind, ledger);
    let over = if picked {
        Some(ledger.accent.opacity(0.24))
    } else if cursor {
        Some(ledger.foreground.opacity(0.07))
    } else {
        None
    };
    if wash.is_none() && over.is_none() {
        return None;
    }
    let mut fill = ledger.background;
    for layer in wash.into_iter().chain(over) {
        fill = fill.blend(layer);
    }
    Some(fill)
}

/// What the list holds at an index: a row, or the comment box among them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    Row(usize),
    Comment,
}

/// Where the comment box goes: under the last picked row, or the cursor's when
/// none is picked.
pub fn comment_under(rows: &[DiffRow]) -> Option<usize> {
    let mut under = None;
    for (i, row) in rows.iter().enumerate() {
        if row.picked || (row.cursor && under.is_none()) {
            under = Some(i);
        }
    }
    under
}

/// The list's items: every row, with the comment box after `under` when it is open.
pub fn items(rows: &[DiffRow], comment: bool) -> Vec<Item> {
    let under = comment.then(|| comment_under(rows)).flatten();
    let mut items = Vec::with_capacity(rows.len() + 1);
    for i in 0..rows.len() {
        items.push(Item::Row(i));
        if under == Some(i) {
            items.push(Item::Comment);
        }
    }
    // A box with no row to sit under (the lines it was on are gone) still shows
    // at the end, so its words are not hidden from the person typing them.
    if comment && under.is_none() {
        items.push(Item::Comment);
    }
    items
}

/// The index in `items` of the cursor's row, to bring into view.
pub fn cursor_item(rows: &[DiffRow], items: &[Item]) -> Option<usize> {
    let cursor = rows.iter().position(|row| row.cursor)?;
    items.iter().position(|item| *item == Item::Row(cursor))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn side(old: Option<u32>, new: Option<u32>, kind: &'static str, parts: &[(&str, &'static str)]) -> DiffSide {
        DiffSide {
            old,
            new,
            marker: "",
            kind,
            parts: parts.iter().map(|(text, kind)| ((*text).to_owned(), *kind)).collect(),
        }
    }

    pub(crate) fn row(kind: &'static str) -> DiffRow {
        DiffRow { row: 0, kind, text: String::new(), hidden: None, step: None, left: None, right: None, cursor: false, picked: false }
    }

    #[test]
    fn a_pair_half_shows_the_new_number_for_an_added_line_else_the_old() {
        assert_eq!(numbers(Some(&side(None, Some(7), "added", &[])), false), [Some(7)]);
        assert_eq!(numbers(Some(&side(Some(3), None, "removed", &[])), false), [Some(3)]);
        assert_eq!(numbers(Some(&side(Some(3), Some(4), "context", &[])), false), [Some(3)]);
        assert_eq!(numbers(None, false), [None], "a missing half is blank");
        assert_eq!(numbers(Some(&side(Some(3), Some(4), "context", &[])), true), [Some(3), Some(4)]);
        assert_eq!(numbers(Some(&side(None, Some(7), "added", &[])), true), [None, Some(7)]);
    }

    #[test]
    fn a_tab_stops_at_the_next_column_a_multiple_of_the_tab_width() {
        let drawn = code(&side(None, None, "context", &[("\tfn", ""), ("\ta", "k")]));
        assert_eq!(drawn.text, "    fn  a");
        assert_eq!(drawn.spans, [(6..9, "k")], "the tab belongs to the part it was typed in");
    }

    #[test]
    fn a_wide_glyph_counts_two_cells_before_the_tab_that_follows_it() {
        // Two cells, then a tab: it stops at 4, two spaces, where a narrow one would take three.
        let wide = code(&side(None, None, "context", &[("\u{4e2d}\tx", "")]));
        assert_eq!(wide.text, "\u{4e2d}  x");
        let narrow = code(&side(None, None, "context", &[("a\tx", "")]));
        assert_eq!(narrow.text, "a   x");
    }

    #[test]
    fn spans_follow_the_text_after_a_tab_widens_it_and_a_carriage_return_is_dropped() {
        let drawn = code(&side(None, None, "added", &[("a\t", ""), ("b", "s"), ("\r", "c")]));
        assert_eq!(drawn.text, "a   b");
        assert_eq!(drawn.spans, [(4..5, "s")], "an emptied part makes no span");
        for (range, _) in &drawn.spans {
            assert!(drawn.text.is_char_boundary(range.start) && drawn.text.is_char_boundary(range.end));
        }
    }

    #[test]
    fn a_control_character_is_shown_not_shaped() {
        assert_eq!(code(&side(None, None, "context", &[("a\u{1b}b", "")])).text, "a\u{fffd}b");
    }

    #[test]
    fn the_comment_sits_under_the_last_picked_row_else_the_cursors() {
        let mut rows = vec![row("line"), row("line"), row("line"), row("line")];
        assert_eq!(comment_under(&rows), None);
        rows[1].cursor = true;
        assert_eq!(comment_under(&rows), Some(1));
        rows[2].picked = true;
        rows[3].picked = true;
        assert_eq!(comment_under(&rows), Some(3), "the last picked row, whatever the cursor");
    }

    #[test]
    fn an_open_comment_is_one_item_after_its_row_and_a_closed_one_is_none() {
        let mut rows = vec![row("line"), row("line"), row("line")];
        rows[1].cursor = true;
        assert_eq!(items(&rows, false), [Item::Row(0), Item::Row(1), Item::Row(2)]);
        assert_eq!(items(&rows, true), [Item::Row(0), Item::Row(1), Item::Comment, Item::Row(2)]);
        let none: Vec<DiffRow> = vec![row("line")];
        assert_eq!(items(&none, true), [Item::Row(0), Item::Comment], "a box with no row to sit under still shows");
    }

    #[test]
    fn the_cursors_item_counts_the_comment_above_it() {
        let mut rows = vec![row("line"), row("line"), row("line")];
        rows[0].picked = true;
        rows[2].cursor = true;
        let list = items(&rows, true);
        assert_eq!(cursor_item(&rows, &list), Some(3), "the box is under the picked row, above the cursor's");
    }

    #[test]
    fn a_row_is_as_tall_as_what_it_is() {
        assert_eq!(height(&row("hunk")), HUNK);
        assert_eq!(height(&row("note")), HUNK);
        assert_eq!(height(&row("fold")), FOLD);
        assert_eq!(height(&row("line")), LINE);
        assert_eq!(height(&row("pair")), LINE);
        assert_eq!(shape(&row("something new")), Shape::Note);
    }

    #[test]
    fn a_pick_is_laid_over_a_sides_wash_and_each_side_stays_distinct() {
        let ledger = Ledger::new();
        assert_eq!(fill("context", false, false, &ledger), None);
        let added = fill("added", false, false, &ledger).unwrap();
        let removed = fill("removed", false, false, &ledger).unwrap();
        let picked_added = fill("added", true, false, &ledger).unwrap();
        let picked_removed = fill("removed", true, false, &ledger).unwrap();
        assert_ne!(picked_added, added, "picking brightens");
        assert_ne!(picked_added, picked_removed, "a picked line still says which side it is");
        assert_ne!(added, removed);
        assert_eq!(fill("context", true, false, &ledger).unwrap().a, 1.0, "opaque, so it paints");
        assert_ne!(fill("context", false, true, &ledger), None, "the cursor lifts a context line");
        assert_eq!(fill("added", true, true, &ledger), fill("added", true, false, &ledger), "a picked line is drawn picked");
    }

    #[test]
    fn each_highlight_kind_has_a_tone_and_an_unknown_one_none() {
        let ledger = Ledger::new();
        for kind in ["k", "s", "c", "n", "t", "f", "p", "o"] {
            assert!(tone(kind, &ledger).is_some(), "{kind}");
        }
        assert!(tone("zz", &ledger).is_none());
        assert!(tone("c", &ledger).unwrap().italic, "a comment is italic");
        assert!(wash("added", &ledger).is_some() && wash("removed", &ledger).is_some() && wash("context", &ledger).is_none());
    }
}
