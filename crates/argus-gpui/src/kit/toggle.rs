//! The page's `.toggle`: a switch, 40 by 24, a pill that is the fill while off and the accent while
//! on, with a round knob that sits left and goes right.
//!
//! Controlled, as every control here is (CROSS-27): it draws the state it is given and asks for the
//! opposite when pressed; the surface sends that to Rust and draws what Rust says next. The kit's
//! `Switch` is its own size and look, and a test could not read its state off it by name.

use gpui_kit::component::{h_flex, Colorize as _};
use gpui_kit::{div, prelude::*, ElementId, Role, SharedString, TestSupportExt as _, Toggled};

use crate::kit::icon_button::Node;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// `off` is a switch that cannot be pressed now: dimmed, with no pointer. The caller adds `on_click`
/// (not while `off`).
pub fn toggle(id: impl Into<ElementId>, label: impl Into<SharedString>, on: bool, off: bool, tokens: &Tokens) -> Node {
    let (rest, lit) = if on { (tokens.accent, tokens.accent.lighten(tokens.step_lift)) } else { (tokens.fill, tokens.fill.lighten(tokens.step_lift)) };
    // The knob has 4px above and beside it; on, it is as far from the right edge.
    let inset = tokens.sp_4;
    let left = if on { tokens.toggle_w - tokens.check_box - inset } else { inset };
    h_flex()
        .id(id)
        .role(Role::Switch)
        .test_support()
        .aria_label(label)
        .aria_toggled(if on { Toggled::True } else { Toggled::False })
        .relative()
        .flex_shrink_0()
        .w(rem(tokens.toggle_w))
        .h(rem(tokens.sp_24))
        .rounded(rem(tokens.r_pill))
        .bg(rest)
        .when(off, |toggle| toggle.opacity(tokens.opacity_dim))
        .when(!off, move |toggle| toggle.cursor_pointer().hover(move |style| style.bg(lit)))
        .child(
            div()
                .absolute()
                .top(rem(inset))
                .left(rem(left))
                .size(rem(tokens.check_box))
                .rounded_full()
                .bg(if on { tokens.fg_on_accent } else { tokens.fg_muted }),
        )
}
