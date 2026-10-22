//! The columns before a line's code: its numbers and marker, and the unified line's pinned to the
//! pane's left edge while the code scrolls sideways under them.

use argus_desktop_core::connection::page::DiffSide;
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, px, Div, Hsla, Pixels, SharedString};

use crate::theme::Tokens;

/// The width of a line-number column, and of the marker's.
pub(super) const NUMBER: f32 = 44.0;
pub(super) const MARKER: f32 = 16.0;
/// What a comment box's left edge clears: both number columns and the marker (`.comment-card`'s 104px).
pub const GUTTER: f32 = NUMBER * 2.0 + MARKER;

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

/// A unified line's gutter held at the pane's left edge: how far right it
/// moves to undo the sideways scroll, and the row's wash it wears over the
/// diff's background so the code passing under it is hidden.
#[derive(Clone, Copy)]
pub(super) struct Pin {
    pub(super) shift: Pixels,
    pub(super) fill: Option<Hsla>,
}

/// A line's number or numbers and its marker, drawn once and placed by whoever holds the row.
pub(super) struct Gutter {
    numbers: Vec<Div>,
    marker: Div,
    width: Pixels,
}

pub(super) fn gutter(side: Option<&DiffSide>, both: bool, tokens: &Tokens) -> Gutter {
    // `.dno`: `--fg-faint` on every line, changed or not.
    let number_colour = tokens.fg_faint;
    let marker_colour = match side.map(|side| side.kind) {
        Some("added") => tokens.ok,
        Some("removed") => tokens.danger,
        _ => tokens.fg_faint,
    };
    let figures = numbers(side, both);
    let width = px(figures.len() as f32 * NUMBER + MARKER);
    let numbers = figures
        .into_iter()
        .map(move |n| {
            div()
                .flex_shrink_0()
                .w(px(NUMBER))
                .pr_2()
                .text_right()
                .text_color(number_colour)
                .child(SharedString::from(n.map_or_else(String::new, |n| n.to_string())))
        })
        .collect();
    let marker = div()
        .flex_shrink_0()
        .w(px(MARKER))
        .text_center()
        .text_color(marker_colour)
        .child(SharedString::from(side.map_or("", |side| side.marker)));
    Gutter { numbers, marker, width }
}

impl Gutter {
    /// The gutter as the row's first children, moving with the code.
    pub(super) fn flowing(self, row: Div) -> Div {
        row.children(self.numbers).child(self.marker)
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
            .child(h_flex().size_full().items_center().when_some(pin.fill, |gutter, fill| gutter.bg(fill)).children(self.numbers).child(self.marker))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::diff::tests::side;

    #[test]
    fn a_pair_half_shows_the_new_number_for_an_added_line_else_the_old() {
        assert_eq!(numbers(Some(&side(None, Some(7), "added", &[])), false), [Some(7)]);
        assert_eq!(numbers(Some(&side(Some(3), None, "removed", &[])), false), [Some(3)]);
        assert_eq!(numbers(Some(&side(Some(3), Some(4), "context", &[])), false), [Some(3)]);
        assert_eq!(numbers(None, false), [None], "a missing half is blank");
        assert_eq!(numbers(Some(&side(Some(3), Some(4), "context", &[])), true), [Some(3), Some(4)]);
        assert_eq!(numbers(Some(&side(None, Some(7), "added", &[])), true), [None, Some(7)]);
    }
}
