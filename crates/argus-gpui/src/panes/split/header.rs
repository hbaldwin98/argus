//! A cell's header: the pane's title, its status, and the button that takes it out of the grid.
//!
//! The header is the handle a pane is carried by (SHELL-GRID-11): a press on it is followed by the
//! work area's drag, which lifts it after the shared threshold, lights the zone it would land on,
//! and lets Escape put it back. The close does not start one. The cell that has the keys wears the
//! selected colour and an accent edge under it (`.gcell.keyed > .thead`); the Workspace's own cell
//! keeps the close button's room empty, so the status dots line up across the headers.

use argus_desktop_core::connection::page::{Call, Shown, Side};
use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, Context, MouseButton, MouseDownEvent, SharedString};

use super::SplitView;
use crate::kit::status_dot::status_dot;
use crate::kit::strip;
use crate::kit::tip::tip;
use crate::layout::carry::{Carried, Carrier};
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The words on a header's close.
pub const TAKE_OUT: &str = "Take out of the grid";

impl SplitView {
    pub(super) fn head(&self, n: u8, pane: &Shown, keyed: bool, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *tokens;
        let (id, title) = (pane.pane, SharedString::from(pane.title.clone()));
        let (fill, foreground) = (tokens.fill, tokens.fg);
        let carried = Carried::Cell { slot: n, pane: id, title: pane.title.clone() };
        let held = self.heard.read(cx).held();
        h_flex()
            .id(SharedString::from(format!("cell-head-{n}")))
            .test_support()
            .aria_selected(keyed)
            .relative()
            .flex_shrink_0()
            .h(rem(tokens.pane_head_h))
            .items_center()
            .gap(rem(tokens.sp_6))
            .pl(rem(tokens.sp_12))
            .bg(if keyed { tokens.bg_selected } else { tokens.bg_chrome })
            .border_b_1()
            .border_color(tokens.line)
            .typed(tokens.type_small)
            .weighted(tokens.weight_medium)
            .text_color(if keyed { tokens.fg } else { tokens.fg_muted })
            .cursor_grab()
            .when(!keyed, |head| head.hover(move |style| style.bg(fill).text_color(foreground)))
            .tooltip(tip("Drag onto another pane to move this one"))
            // A press that is never a drag leaves the header as it was: it has no click of its own.
            .on_mouse_down(MouseButton::Left, {
                let carrier = Carrier::of(cx);
                cx.listener(move |_, event: &MouseDownEvent, _, cx| {
                    if !held {
                        carrier.press(event.position, carried.clone(), cx);
                    }
                })
            })
            .child(strip::title(title.clone()).flex_1())
            .child(status_dot(format!("cell-{n}"), pane.status, &tokens))
            .child(if n == 0 {
                div().w(rem(tokens.control_h_sm)).flex_shrink_0().into_any_element()
            } else {
                strip::close_x(SharedString::from(format!("cell-close-{n}")), format!("Take {title} out of the grid"), &tokens)
                    .tooltip(tip(TAKE_OUT))
                    // A press on the close must not start a move.
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if !this.heard.read(cx).held() {
                            this.caller.call(Call::CloseBeside { side: Side::Beside(n - 1) });
                        }
                    }))
                    .into_any_element()
            })
            .when(keyed, |head| head.child(div().absolute().left_0().right_0().bottom_0().h(rem(tokens.rule)).bg(tokens.accent)))
    }
}
