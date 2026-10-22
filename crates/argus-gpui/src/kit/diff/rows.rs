//! What each kind of row draws: a hunk's heading, a unified line, a split pair of halves, and the
//! border and click every row but a heading shares.
//!
//! It draws them and reports what was pressed: a click on a line, with Shift held or not. It sends
//! no call and changes no row, so a refused click leaves the diff as the session says it is.

use argus_desktop_core::connection::page::{DiffRow, DiffSide};
use gpui_kit::component::h_flex;
use gpui_kit::{prelude::*, AnyElement, App, ClickEvent, Context, ElementId, MouseButton, MouseDownEvent, SharedString, TestSupportExt as _, Window};

use super::gutter::{gutter, Pin};
use super::paint::{code, Ink};
use super::shape::{self, Shape};
use super::text::Code;
use super::{tone, wrap, DiffEvent, DiffView};
use crate::theme::scale::rem;
use crate::theme::Tokens;

impl DiffView {
    pub(super) fn row_id(ix: usize) -> ElementId {
        ("diff-row", ix).into()
    }

    /// One row, `ix` of the rows.
    pub(super) fn row(&self, ix: usize, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let row = &self.rows[ix];
        let base = match shape::shape(row) {
            Shape::Hunk | Shape::Note => return self.heading(ix, row, tokens),
            Shape::Fold => return self.fold(ix, row, tokens, cx),
            Shape::Line => self.line(ix, row, tokens, cx),
            Shape::Pair => self.pair(ix, row, tokens, cx),
        };
        let (number, interactive) = (row.row, self.interactive);
        self.wide(base)
            .id(Self::row_id(ix))
            .test_support()
            .flex_shrink_0()
            .h(rem(self.heights[ix]))
            .border_1()
            .border_color(if row.cursor || row.picked { tokens.line_strong } else { gpui_kit::transparent_black() })
            .when(interactive, |row| {
                row.cursor_text().on_click(cx.listener(move |_, event: &ClickEvent, _, cx| {
                    cx.emit(DiffEvent::Row { row: number, extend: event.modifiers().shift });
                }))
            })
            .into_any_element()
    }

    /// A hunk's `@@` line or a note: said, never picked.
    fn heading(&self, ix: usize, row: &DiffRow, tokens: &Tokens) -> AnyElement {
        self.wide(h_flex())
            .id(Self::row_id(ix))
            .test_support()
            .flex_shrink_0()
            .h(rem(shape::HUNK))
            .px(rem(tokens.sp_12))
            .items_center()
            .bg(tokens.bg_chrome)
            .text_color(tokens.fg_faint)
            .overflow_hidden()
            .whitespace_nowrap()
            .child(SharedString::from(row.text.clone()))
            .into_any_element()
    }

    /// A unified line: two numbers, a marker, then code.
    fn line(&self, ix: usize, row: &DiffRow, tokens: &Tokens, cx: &mut Context<Self>) -> gpui_kit::Div {
        let side = row.left.as_ref();
        let kind = side.map_or("context", |side| side.kind);
        let fill = tone::fill(kind, row.picked, row.cursor, tokens);
        // The gutter stays at the pane's left edge while the code scrolls
        // sideways under it: the row has moved by the offset, the gutter moves back.
        let pin = Pin { shift: -self.scroll.offset().x, fill };
        let down = cx.listener(move |this, event: &MouseDownEvent, _, _| this.press(ix, 0, event.position));
        h_flex()
            .items_center()
            .when_some(fill, |row, fill| row.bg(fill))
            .child(half(side, &self.drawn[ix][0], true, tokens, self.ink(ix, 0, &self.drawn[ix][0].text, cx), down, Some(pin)))
            .overflow_hidden()
    }

    /// A split pair: its two halves, each with its own number, marker and
    /// code, cut at its own half. A missing half is blank and recessed.
    fn pair(&self, ix: usize, row: &DiffRow, tokens: &Tokens, cx: &mut Context<Self>) -> gpui_kit::Div {
        let side = |side: Option<&DiffSide>, second: bool| {
            let fill = match side {
                Some(side) => tone::fill(side.kind, row.picked, row.cursor, tokens),
                None => Some(tokens.bg_chrome),
            };
            h_flex()
                .flex_1()
                .min_w_0()
                .h_full()
                .items_center()
                .overflow_hidden()
                .when(second, |half| half.border_l_1().border_color(tokens.line))
                .when_some(fill, |half, fill| half.bg(fill))
                .child({
                    let wrapped = wrap::wrap(&self.drawn[ix][usize::from(second)], self.per_line);
                    let ink = self.ink(ix, usize::from(second), &wrapped.text, cx);
                    half(side, &wrapped, false, tokens, ink, cx.listener(move |this, event: &MouseDownEvent, _, _| this.press(ix, usize::from(second), event.position)), None)
                })
        };
        h_flex()
            .items_center()
            .when(self.heights[ix] > shape::LINE, |pair| pair.line_height(rem(shape::LINE)))
            .child(side(row.left.as_ref(), false))
            .child(side(row.right.as_ref(), true))
    }
}

/// A line's number or numbers, its marker and its code.
fn half(
    side: Option<&DiffSide>,
    drawn: &Code,
    both: bool,
    tokens: &Tokens,
    ink: Ink,
    down: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    pin: Option<Pin>,
) -> impl IntoElement {
    let gutter = gutter(side, both, tokens);
    let code = gpui_kit::div()
        .flex_1()
        .min_w_0()
        .whitespace_nowrap()
        .on_mouse_down(MouseButton::Left, down)
        .children(side.map(|_| code(drawn, tokens, ink)));
    let Some(pin) = pin else {
        return gutter.flowing(h_flex().flex_1().min_w_0().items_center()).child(code);
    };
    // Painted after the code so it lies over it: a spacer keeps the code's
    // place, and the gutter itself sits absolutely at the pane's edge.
    let spacer = gutter.spacer();
    h_flex()
        .relative()
        .flex_1()
        .min_w_0()
        .h_full()
        .items_center()
        .child(spacer)
        .child(code)
        .child(gutter.pinned(pin, tokens))
}
