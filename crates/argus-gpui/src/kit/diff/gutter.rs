//! The columns before a line's code: its numbers and marker, and the unified line's pinned to the
//! pane's left edge while the code scrolls sideways under them.

use argus_desktop_core::connection::page::{DiffRow, DiffSide};
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, Div, Hsla, Pixels, SharedString};

use super::shape::{self, Shape};
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The width of a line-number column, and of the marker's.
pub(super) const NUMBER: f32 = 44.0;
pub(super) const MARKER: f32 = 16.0;
/// What a comment box's left edge clears when a diff shows both number columns and the marker
/// (`.comment-card`'s 104px).
pub const GUTTER: f32 = NUMBER * 2.0 + MARKER;

/// Which of a unified line's two number columns a diff shows. A column no line has a number in
/// (the old one of a new file, the new one of a deleted file) is left out rather than drawn as a
/// wide empty lane beside every line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shown {
    pub old: bool,
    pub new: bool,
}

impl Shown {
    pub const BOTH: Shown = Shown { old: true, new: true };

    /// How many number columns are drawn.
    pub fn count(self) -> usize {
        usize::from(self.old) + usize::from(self.new)
    }

    /// What a unified line puts before its code: the columns shown and the marker, which a
    /// comment box clears.
    pub fn width(self) -> f32 {
        self.count() as f32 * NUMBER + MARKER
    }
}

/// The columns `rows` need: each that some unified line has a number in. Rows with no unified line
/// (a split diff's pairs, or none at all) keep both, so nothing moves while a diff loads.
pub fn shown(rows: &[DiffRow]) -> Shown {
    let mut shown = Shown { old: false, new: false };
    for side in rows.iter().filter(|row| shape::shape(row) == Shape::Line).filter_map(|row| row.left.as_ref()) {
        shown.old |= side.old.is_some();
        shown.new |= side.new.is_some();
    }
    if shown.count() == 0 {
        Shown::BOTH
    } else {
        shown
    }
}

/// What a file read whole puts before its code: the one number column and the marker, which a
/// comment card and a removal marker clear (`.cgone`'s and `.comment-card`'s 68px).
pub fn reading_gutter(tokens: &Tokens) -> f32 {
    tokens.code_no_w + MARKER
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

/// Which number columns a line carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Columns {
    /// The old number and the new of a unified line, as many of them as the diff shows.
    Both(Shown),
    /// The one a half of a split pair shows.
    One,
    /// A file read whole: the line's own number, in the wider column `.cline` gives it.
    Reading,
}

/// A unified line's gutter held at the pane's left edge: how far right it
/// moves to undo the sideways scroll.
#[derive(Clone, Copy)]
pub(super) struct Pin {
    pub(super) shift: Pixels,
}

/// A line's number or numbers and its marker, drawn once and placed by whoever holds the row, on
/// the gutter's own wash (`tone::gutter_fill`).
pub(super) struct Gutter {
    numbers: Vec<Div>,
    marker: Div,
    width: gpui_kit::Rems,
    fill: Option<Hsla>,
}

pub(super) fn gutter(side: Option<&DiffSide>, columns: Columns, fill: Option<Hsla>, tokens: &Tokens) -> Gutter {
    // `.dno`: `--fg-faint` on every line, changed or not.
    let number_colour = tokens.fg_faint;
    let marker_colour = match side.map(|side| side.kind) {
        Some("added") => tokens.ok,
        Some("removed") => tokens.danger,
        _ => tokens.fg_faint,
    };
    let figures = match columns {
        Columns::Both(shown) => {
            let [old, new] = [side.and_then(|s| s.old), side.and_then(|s| s.new)];
            [(shown.old, old), (shown.new, new)].into_iter().filter(|(on, _)| *on).map(|(_, n)| n).collect()
        }
        Columns::One => numbers(side, false),
        Columns::Reading => vec![side.and_then(|side| side.new)],
    };
    let each = if columns == Columns::Reading { tokens.code_no_w } else { NUMBER };
    let width = rem(figures.len() as f32 * each + MARKER);
    let numbers = figures
        .into_iter()
        .map(move |n| {
            div()
                .flex_shrink_0()
                .w(rem(each))
                .pr_2()
                .text_right()
                .text_color(number_colour)
                .child(SharedString::from(n.map_or_else(String::new, |n| n.to_string())))
        })
        .collect();
    let marker = div()
        .flex_shrink_0()
        .w(rem(MARKER))
        .text_center()
        .text_color(marker_colour)
        .child(SharedString::from(side.map_or("", |side| side.marker)));
    Gutter { numbers, marker, width, fill }
}

impl Gutter {
    /// The gutter as the row's first child, moving with the code.
    pub(super) fn flowing(self, row: Div) -> Div {
        row.child(
            h_flex()
                .flex_shrink_0()
                .h_full()
                .items_center()
                .when_some(self.fill, |gutter, fill| gutter.bg(fill))
                .children(self.numbers)
                .child(self.marker),
        )
    }

    /// What keeps the code's place while the gutter itself is pinned.
    pub(super) fn spacer(&self) -> Div {
        div().flex_shrink_0().w(self.width)
    }

    /// The gutter laid absolutely at the pane's edge, painted after the code so it lies over it.
    pub(super) fn pinned(self, pin: Pin, tokens: &Tokens) -> Div {
        div()
            .absolute()
            .top_0()
            .bottom_0()
            .left(pin.shift)
            .w(self.width)
            .bg(tokens.bg_sunken)
            .child(h_flex().size_full().items_center().when_some(self.fill, |gutter, fill| gutter.bg(fill)).children(self.numbers).child(self.marker))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::diff::tests::{row, side};

    #[test]
    fn a_pair_half_shows_the_new_number_for_an_added_line_else_the_old() {
        assert_eq!(numbers(Some(&side(None, Some(7), "added", &[])), false), [Some(7)]);
        assert_eq!(numbers(Some(&side(Some(3), None, "removed", &[])), false), [Some(3)]);
        assert_eq!(numbers(Some(&side(Some(3), Some(4), "context", &[])), false), [Some(3)]);
        assert_eq!(numbers(None, false), [None], "a missing half is blank");
        assert_eq!(numbers(Some(&side(Some(3), Some(4), "context", &[])), true), [Some(3), Some(4)]);
        assert_eq!(numbers(Some(&side(None, Some(7), "added", &[])), true), [None, Some(7)]);
    }

    fn unified(old: Option<u32>, new: Option<u32>, kind: &'static str) -> DiffRow {
        DiffRow { left: Some(side(old, new, kind, &[])), ..row("line") }
    }

    /// A new file's lines have no old numbers and a deleted file's no new ones: the empty column
    /// is left out, and a diff with both kept whole.
    #[test]
    fn a_number_column_no_line_uses_is_left_out() {
        let new_file = [row("hunk"), unified(None, Some(1), "added"), unified(None, Some(2), "added")];
        assert_eq!(shown(&new_file), Shown { old: false, new: true });
        assert_eq!(shown(&new_file).width(), NUMBER + MARKER);
        let deleted = [unified(Some(1), None, "removed")];
        assert_eq!(shown(&deleted), Shown { old: true, new: false });
        let edited = [unified(Some(1), None, "removed"), unified(None, Some(1), "added")];
        assert_eq!(shown(&edited), Shown::BOTH);
        assert_eq!(shown(&[]), Shown::BOTH, "nothing to judge by keeps both");
        assert_eq!(shown(&[row("pair")]), Shown::BOTH, "a split diff's halves have one column each already");
        assert_eq!(Shown::BOTH.width(), GUTTER);
    }
}
