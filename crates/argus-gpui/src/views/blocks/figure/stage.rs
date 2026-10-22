//! The larger view's stage, where the drawing is moved, and the pane that shows its source
//! (FIGURE-2 to FIGURE-5).
//!
//! The stage is measured each frame, which is how a drawing not yet placed opens (`Larger::measured`);
//! the wheel zooms it about the pointer and a left press that goes 5 px drags it. The drawing is
//! drawn afresh at the scale it is at, so what is pressable in it still is.

use gpui_kit::{canvas, div, prelude::*, AnyElement, Context, MouseButton, MouseDownEvent, ScrollWheelEvent, SharedString, Window};
use gpui_kit::TestSupportExt as _;

use super::larger::Larger;
use crate::kit::drag;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// `.vfig-stage`: the sunken ground the drawing sits on, clipped to itself.
pub fn stage(larger: &Larger, window: &mut Window, tokens: &Tokens, cx: &mut Context<Larger>) -> AnyElement {
    let (area, me) = (larger.stage.clone(), cx.entity());
    let measure = canvas(
        move |bounds, window, cx| {
            let changed = area.get().size != bounds.size;
            area.set(bounds);
            if changed {
                let me = me.clone();
                window.defer(cx, move |window, cx| me.update(cx, |larger, cx| larger.measured(window, cx)));
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full();
    let drawing = larger.zoom().map(|zoom| {
        div()
            .id("figure-canvas")
            .test_support()
            .absolute()
            .left(rem(zoom.x))
            .top(rem(zoom.y))
            .child((larger.figure.draw)(zoom.scale))
    });
    let _ = window;
    div()
        .id("figure-stage")
        .test_support()
        .relative()
        .flex_1()
        .min_w_0()
        .h_full()
        .overflow_hidden()
        .bg(tokens.bg_sunken)
        .when(larger.dragging, |stage| stage.cursor_grabbing())
        .when(!larger.dragging, |stage| stage.cursor_grab())
        .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, window, cx| {
            cx.stop_propagation();
            this.wheel(event.delta, event.position, window, cx);
        }))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, event: &MouseDownEvent, _, _| {
                this.press(event.position);
            }),
        )
        .child(measure)
        .children(drawing)
        .child(drag::watch(cx.weak_entity()))
        .into_any_element()
}

/// `.vfig-source`: what the figure was drawn from as it was written, beside the stage, `min(40%,
/// 520px)` wide, under a rule; none until Source is on.
pub fn source(larger: &Larger, tokens: &Tokens) -> Option<AnyElement> {
    let text = larger.figure.source.as_ref().filter(|_| larger.is_source_shown())?;
    Some(
        div()
            .id("figure-source-pane")
            .test_support()
            .flex_shrink_0()
            .w(gpui_kit::relative(tokens.fig_source_share))
            .max_w(rem(tokens.fig_source_max_w))
            .h_full()
            .p(rem(tokens.sp_12))
            .overflow_scroll()
            .track_scroll(&larger.source_scroll)
            .border_l_1()
            .border_color(tokens.line)
            .text_color(tokens.fg_muted)
            .font_family(SharedString::from(tokens.font_mono))
            .text_size(rem(tokens.fs_sm))
            .whitespace_nowrap()
            .child(SharedString::from(text.clone()))
            .into_any_element(),
    )
}
