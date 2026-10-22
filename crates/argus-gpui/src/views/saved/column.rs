//! One column of a saved panel: its header over its body, and the stand-in for a view the window
//! does not hold (CHECKOUTS-CODE-VIEWS-SAVED-1, -2).
//!
//! Columns share the panel's width and each is at least `--saved-col-min` wide (or the panel's width
//! when that is less), so the panel scrolls sideways only when they no longer fit. The body scrolls
//! on its own, down, and its offset is the panel's (`panel`), kept across redraws.

use gpui_kit::component::v_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, Role, ScrollHandle, SharedString};

use super::header::{close_button, header, Head};
use crate::kit::icon_button::Node;
use crate::kit::wheel::Wheel;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::views::blocks::pen::Pen;
use crate::views::ctx::ViewCtx;

/// The box every column is: grows to share the width, never shrinks below its least, and never
/// exceeds the panel; the hairline between columns is on all but the last.
fn frame(id: String, last: bool, title: String, pen: &Pen) -> Node {
    let tokens = pen.draw.tokens;
    v_flex()
        .id(SharedString::from(id))
        .role(Role::Region)
        .test_support()
        .aria_label(SharedString::from(title))
        .flex_grow(1.0)
        .flex_shrink_0()
        .flex_basis(rem(tokens.saved_col_min))
        .max_w_full()
        .min_w_0()
        .h_full()
        .min_h_0()
        .when(!last, |column| column.border_r_1().border_color(tokens.line))
}

/// A column showing `head.view`: the header, then the blocks in `body`, scrolling down under it.
pub fn column(head: &Head, ctx: &ViewCtx, body: AnyElement, scroll: &ScrollHandle, last: bool, pen: &mut Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let id = format!("saved-col-{}-{}", head.panel, head.index);
    let top = header(head, ctx, pen);
    let wheel = Wheel::new(scroll);
    frame(id.clone(), last, head.view.view.title.clone(), pen)
        .child(top)
        .child(
            div()
                .id(SharedString::from(format!("{id}-body")))
                .test_support()
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(scroll)
                .on_scroll_wheel(wheel.keep())
                .child(wheel.probe())
                .p(rem(tokens.sp_16))
                .child(body),
        )
        .into_any_element()
}

/// A column whose view the window no longer holds, or does not yet: it says so with the id, and
/// closes, since the view may be saved later.
pub fn missing(panel: &argus_desktop_core::layout::docks::Panel, index: usize, id: &str, last: bool, held: bool, pen: &Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let key = format!("saved-col-{panel}-{index}");
    frame(key, last, "View not found".to_owned(), pen)
        .child(
            v_flex()
                .flex_shrink_0()
                .gap(rem(tokens.sp_6))
                .px(rem(tokens.sp_16))
                .py(rem(tokens.sp_12))
                .border_b_1()
                .border_color(tokens.line)
                .child(
                    gpui_kit::component::h_flex()
                        .gap(rem(tokens.sp_8))
                        .items_center()
                        .child(div().flex_1().min_w_0().truncate().typed(tokens.type_heading).child("View not found"))
                        .child(close_button(panel, index, pen, held)),
                )
                .child(div().text_size(rem(tokens.fs_xs)).text_color(tokens.fg_faint).child(SharedString::from(id.to_owned()))),
        )
        .into_any_element()
}
