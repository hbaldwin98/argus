//! What a list or a diff looks like while it loads: the page's `.ghost` bars.
//!
//! Each bar is a share of the width, a `--fill` strip 12px tall that fades to
//! 45% and back on the page's 1.4s shimmer, so a pane that has nothing yet is
//! seen to be waiting rather than empty.

use gpui_kit::component::v_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, px, relative, Animation, AnimationExt as _, SharedString};

use crate::theme::Tokens;

/// What a bar keeps of its colour at the middle of the shimmer.
const DIM: f32 = 0.45;

/// The column of bars named `id`: `ghost` has each 8px under the last, and
/// 16px in from the sides.
pub fn ghosts(id: &'static str, bars: &'static [f32], tokens: &Tokens) -> impl IntoElement {
    let fill = tokens.fill;
    v_flex().id(id).test_support().size_full().px(px(tokens.sp_16)).children(bars.iter().enumerate().map(move |(at, width)| {
        div()
            .id((id, at))
            .mt(px(tokens.sp_8))
            .flex_shrink_0()
            .h(px(12.0))
            .w(relative(*width))
            .rounded(px(tokens.r_xs))
            .bg(fill)
            .with_animation(
                SharedString::from(format!("{id}-{at}-shimmer")),
                Animation::new(tokens.motion_shimmer).repeat(),
                // 1 at either end and `DIM` in the middle, as `@keyframes ghost`.
                |bar, delta| bar.opacity(1.0 - (1.0 - DIM) * (1.0 - (2.0 * delta - 1.0).abs())),
            )
    }))
}
