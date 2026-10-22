//! How a block that changed since the person saw it is marked: one wash in the changed colour that
//! fades, so a block that moved is found without being read for, and the dot a tab wears while its
//! view changed out of sight (AGENTVIEWS-13, BLOCKS-2).
//!
//! Which blocks changed, and whether the person was looking when they did, is core's
//! (`views::changes`) and arrives with the view as paths (`ViewCtx::changed`); a view changed out of
//! sight draws the same wash when it is opened. The wash runs once per change: it is named by the
//! change it belongs to (`ViewCtx::flash`, which the blocks element counts), so a redraw that changed
//! nothing does not start it again and a later change does. With reduced motion it is not drawn.

use gpui_kit::{div, point, prelude::*, Animation, AnimationExt as _, AnyElement, App, BoxShadow, Role, SharedString, TestSupportExt as _, Window};

use crate::theme::scale::{pixels, rem};
use crate::theme::Tokens;
use crate::views::ctx::ViewCtx;

/// `block`, washed if its path is among those that changed. `ctx` is the block's own.
pub fn marked(block: AnyElement, ctx: &ViewCtx, tokens: &Tokens, window: &Window, cx: &App) -> AnyElement {
    if !ctx.is_changed() || cx.reduce_motion() {
        return block;
    }
    let (wash, spread) = (tokens.changed.opacity(tokens.share_changed), pixels(tokens.sp_4, window.rem_size()));
    let id = SharedString::from(format!("{}-changed", ctx.path));
    div()
        .id(id.clone())
        .test_support()
        .rounded(rem(tokens.r_sm))
        .child(block)
        .with_animation(
            SharedString::from(format!("{id}{}", ctx.flash)),
            // `ease-out`, which the page's wash is written with.
            Animation::new(tokens.motion_flash).with_easing(gpui_kit::base::animation::cubic_bezier(0.0, 0.0, 0.58, 1.0)),
            move |mark, delta| {
                let left = wash.opacity(1.0 - delta);
                mark.bg(left).shadow(vec![BoxShadow { color: left, offset: point(gpui_kit::px(0.0), gpui_kit::px(0.0)), blur_radius: gpui_kit::px(0.0), spread_radius: spread, inset: false }])
            },
        )
        .into_any_element()
}

/// `.vnew`: the dot a tab of a view that changed since it was in front of the person wears, in the
/// changed colour, `--sp-6` across.
pub fn unseen(tokens: &Tokens) -> AnyElement {
    div()
        .id("view-unseen")
        .role(Role::Image)
        .test_support()
        .aria_label("changed")
        .flex_shrink_0()
        .size(rem(tokens.sp_6))
        .rounded(rem(tokens.r_pill))
        .bg(tokens.changed)
        .into_any_element()
}
