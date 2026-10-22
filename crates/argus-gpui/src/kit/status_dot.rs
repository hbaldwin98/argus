//! What a pane's or a console's status looks like when it has no word: the page's `.sdot`.

use gpui_kit::{
    div, prelude::*, px, Animation, AnimationExt as _, SharedString, Transformation, percentage,
};

use crate::kit::icons::Glyph;
use crate::theme::Tokens;

/// `.sdot`: working is a ring turning at 1s; waiting, review and failed are
/// filled; ended is a dash; idle is a ring. `key` names the turn, so two dots
/// on screen do not share one animation.
pub fn status_dot(key: impl Into<SharedString>, status: &str, tokens: &Tokens) -> gpui_kit::AnyElement {
    let at = |element: gpui_kit::AnyElement| div().flex().items_center().size(px(tokens.status_box)).flex_shrink_0().child(element).into_any_element();
    match status {
        "working" => at(Glyph::Arc
            .at(tokens.status_box)
            .text_color(tokens.accent)
            .with_animation(
                SharedString::from(format!("{}-spin", key.into())),
                Animation::new(tokens.motion_spin_slow).repeat(),
                |icon, delta| icon.transform(Transformation::rotate(percentage(delta))),
            )
            .into_any_element()),
        "waiting" | "needs-review" => div().size(px(tokens.status_box)).flex_shrink_0().rounded_full().bg(tokens.attention).into_any_element(),
        "failed" => div().size(px(tokens.status_box)).flex_shrink_0().rounded_full().bg(tokens.danger).into_any_element(),
        "exited" | "done" => div()
            .w(px(tokens.status_box))
            .h(px(tokens.status_dash_h))
            .flex_shrink_0()
            .rounded(px(1.0))
            .bg(if status == "done" { tokens.ok } else { tokens.fg_faint })
            .into_any_element(),
        _ => at(Glyph::Ring9.at(tokens.status_box).text_color(tokens.fg_faint).into_any_element()),
    }
}
