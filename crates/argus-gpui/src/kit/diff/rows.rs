//! What each kind of row draws: a hunk's heading, a unified line, a split pair of halves, and the
//! border and click every row but a heading shares.
//!
//! It draws them and reports what was pressed: a click on a line, with Shift held or not. It sends
//! no call and changes no row, so a refused click leaves the diff as the session says it is.

use argus_desktop_core::connection::page::{DiffRow, DiffSide};
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, AnyElement, App, ClickEvent, Context, ElementId, Hsla, MouseButton, MouseDownEvent, SharedString, TestSupportExt as _, Window};

use super::gutter::{gutter, Columns, Pin};
use super::paint::{code, Ink};
use super::shape::{self, Shape};
use super::text::Code;
use super::{text, tone, wrap, DiffEvent, DiffView};
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
            Shape::Hunk | Shape::Note => return self.heading(ix, row, tokens, cx),
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

    /// A hunk's `@@` line or a note: said, never picked. A hunk is a band on the chrome with a
    /// hairline above, as tall as a fold, so where each hunk starts reads at a glance down a long
    /// diff without a colour that could be taken for an added or removed side. Its ranges are said
    /// in the reference colour, as the fold's buttons that open more lines are, and the code it is
    /// in, which is what is read, in the muted text.
    fn heading(&self, ix: usize, row: &DiffRow, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        if self.reading && shape::shape(row) == Shape::Note {
            return self.gone(ix, row, tokens);
        }
        let (ranges, context) = text::hunk(&row.text);
        self.wide(h_flex())
            .id(Self::row_id(ix))
            .test_support()
            .flex_shrink_0()
            .h(rem(shape::HUNK))
            .px(rem(tokens.sp_12))
            .gap(rem(tokens.sp_12))
            .items_center()
            .bg(tokens.bg_chrome)
            .border_t_1()
            .border_color(tokens.line)
            .text_color(tokens.fg_muted)
            .overflow_hidden()
            .whitespace_nowrap()
            .when(!ranges.is_empty(), |band| band.child(div().flex_shrink_0().text_color(tokens.ref_).child(SharedString::from(ranges.to_owned()))))
            .when(!context.is_empty(), |band| band.child(div().min_w_0().flex_shrink(1.0).truncate().child(SharedString::from(context.to_owned()))))
            .when_some(self.ticks.filter(|_| shape::shape(row) == Shape::Hunk), |band, ticks| {
                let number = row.row;
                let words = if ticks.staged { "Unstage hunk" } else { "Stage hunk" };
                band.child(div().flex_1()).child(
                    div()
                        .id(("diff-hunk-stage", ix))
                        .test_support()
                        .aria_label(SharedString::from(words))
                        .flex_shrink_0()
                        .px(rem(tokens.sp_8))
                        .rounded(rem(tokens.r_sm))
                        .border_1()
                        .border_color(tokens.line_strong)
                        .font_family(SharedString::from(tokens.font_ui))
                        .text_size(rem(tokens.fs_xs))
                        .text_color(tokens.fg)
                        .cursor_pointer()
                        .hover(|button| button.bg(tokens.fill))
                        .child(words)
                        .on_mouse_down(MouseButton::Left, cx.listener(move |_, _: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            cx.emit(DiffEvent::Hunk { row: number });
                        })),
                )
            })
            .into_any_element()
    }

    /// Where a file read whole lost lines, said under the gutter in the danger colour, on a wash of it
    /// with a rule above (`.cgone`).
    fn gone(&self, ix: usize, row: &DiffRow, tokens: &Tokens) -> AnyElement {
        self.wide(h_flex())
            .id(Self::row_id(ix))
            .test_support()
            .flex_shrink_0()
            .h(rem(shape::GONE))
            .pl(rem(super::gutter::reading_gutter(tokens)))
            .items_center()
            .font_family(SharedString::from(tokens.font_ui))
            .text_size(rem(tokens.fs_xs))
            .text_color(tokens.danger)
            .bg(tokens.bg_sunken.blend(tokens.danger.opacity(tokens.share_gone)))
            .border_t_1()
            .border_color(tokens.danger.opacity(tokens.share_gone_edge))
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
        // A file read whole keeps one wash across its line, as the Code stage's `.cline` has it.
        let numbers = if self.reading { fill } else { tone::gutter_fill(kind, row.picked, row.cursor, tokens) };
        // The gutter stays at the pane's left edge while the code scrolls
        // sideways under it: the row has moved by the offset, the gutter moves back.
        let pin = Pin { shift: -self.scroll.offset().x };
        let down = cx.listener(move |this, event: &MouseDownEvent, _, _| this.press(ix, 0, event.position));
        let columns = if self.reading { Columns::Reading } else { Columns::Both(self.shown) };
        let tick = self.tick(ix, 0, side, tokens, cx);
        h_flex()
            .items_center()
            .when_some(fill, |row, fill| row.bg(fill))
            .child(half(side, &self.drawn[ix][0], (columns, numbers, tick), tokens, self.ink(ix, 0, &self.drawn[ix][0].text, cx), down, Some(pin)))
            .overflow_hidden()
    }

    /// A split pair: its two halves, each with its own number, marker and
    /// code, cut at its own half. A missing half is blank and recessed.
    fn pair(&self, ix: usize, row: &DiffRow, tokens: &Tokens, cx: &mut Context<Self>) -> gpui_kit::Div {
        let mut side = |side: Option<&DiffSide>, second: bool| {
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
                    let numbers = side.and_then(|side| tone::gutter_fill(side.kind, row.picked, row.cursor, tokens));
                    let tick = self.tick(ix, usize::from(second), side, tokens, cx);
                    half(side, &wrapped, (Columns::One, numbers, tick), tokens, ink, cx.listener(move |this, event: &MouseDownEvent, _, _| this.press(ix, usize::from(second), event.position)), None)
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
    (columns, numbers, tick): (Columns, Option<Hsla>, Option<AnyElement>),
    tokens: &Tokens,
    ink: Ink,
    down: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    pin: Option<Pin>,
) -> impl IntoElement {
    let gutter = gutter(side, columns, numbers, tokens).with_tick(tick);
    let code = gpui_kit::div()
        .flex_1()
        .min_w_0()
        .whitespace_nowrap()
        .on_mouse_down(MouseButton::Left, down)
        .children(side.map(|_| code(drawn, tokens, ink)));
    let Some(pin) = pin else {
        return gutter.flowing(h_flex().flex_1().min_w_0().h_full().items_center()).child(code);
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
