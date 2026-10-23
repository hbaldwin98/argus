//! A changed line's box, in a diff that stages: ticked when the diff on screen is the index's, so
//! a tick takes a line in and an untick gives it back, and what a press on one, or on a hunk's
//! button, says to the owner.
//!
//! The box is drawn from what the session said the diff is, never from the press: a tick is a
//! request, and the box changes when the diff read after the index moved says so.

use argus_desktop_core::connection::page::DiffSide;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, Context, MouseButton, MouseDownEvent};

use super::{DiffEvent, DiffView};
use crate::kit::icons::Glyph;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The box's column, before the numbers.
pub const BOX: f32 = 24.0;
const MARK: f32 = 12.0;

/// How a diff that stages draws its boxes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ticks {
    /// The diff is the index's: every changed line in it is staged.
    pub staged: bool,
    /// A write is out, and the boxes wait on the diff that answers it.
    pub busy: bool,
}

/// Whether a side is a line a box is drawn on: one that was added or removed.
pub fn tickable(side: Option<&DiffSide>) -> bool {
    side.is_some_and(|side| side.kind == "added" || side.kind == "removed")
}

impl DiffView {
    /// Draws a box on each changed line and a button on each hunk, or none.
    pub fn set_ticks(&mut self, ticks: Option<Ticks>, cx: &mut Context<Self>) {
        if self.ticks != ticks {
            self.ticks = ticks;
            self.relist();
            cx.notify();
        }
    }

    /// The box for `side` of row `ix`, or the empty column on a line that has none, so every line's
    /// numbers stay where they are.
    pub(super) fn tick(&self, ix: usize, half: usize, side: Option<&DiffSide>, tokens: &Tokens, cx: &mut Context<Self>) -> Option<gpui_kit::AnyElement> {
        let ticks = self.ticks?;
        let cell = div().flex_shrink_0().w(rem(BOX)).h_full().flex().items_center().justify_center();
        if !tickable(side) {
            return Some(cell.into_any_element());
        }
        let number = self.rows[ix].row;
        let mark = div()
            .size(rem(MARK))
            .rounded(rem(3.0))
            .border_1()
            .flex()
            .items_center()
            .justify_center()
            .when(ticks.staged, |mark| mark.bg(tokens.accent).border_color(tokens.accent).text_color(tokens.fg_on_accent).child(Glyph::Check.at(10.0)))
            .when(!ticks.staged, |mark| mark.bg(tokens.bg_sunken).border_color(tokens.line_strong))
            .when(ticks.busy, |mark| mark.opacity(0.5));
        Some(
            cell.id(("diff-tick", ix * 2 + half))
                .test_support()
                .aria_label(if ticks.staged { "Unstage the line" } else { "Stage the line" })
                .cursor_pointer()
                .hover(|cell| cell.bg(tokens.fill))
                .child(mark)
                // Pressed, not clicked: the row under it would take the click as a cursor move, and a
                // press on the box is not one.
                .on_mouse_down(MouseButton::Left, cx.listener(move |_, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    cx.emit(DiffEvent::Tick { row: number, extend: event.modifiers.shift });
                }))
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::diff::tests::side;

    #[test]
    fn only_an_added_or_removed_line_has_a_box() {
        assert!(tickable(Some(&side(None, Some(2), "added", &[]))));
        assert!(tickable(Some(&side(Some(2), None, "removed", &[]))));
        assert!(!tickable(Some(&side(Some(1), Some(1), "context", &[]))));
        assert!(!tickable(None), "a split pair's missing half");
    }
}
