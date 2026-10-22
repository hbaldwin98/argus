//! What a pane's or a console's status looks like when it has no word: the page's `.sdot`.

use gpui_kit::{
    div, prelude::*, px, Animation, AnimationExt as _, SharedString, Transformation, percentage,
};

use crate::icons::Glyph;
use crate::theme::Ledger;
use crate::tokens::{motion, size};

/// `.sdot`: working is a ring turning at 1s; waiting, review and failed are
/// filled; ended is a dash; idle is a ring. `key` names the turn, so two dots
/// on screen do not share one animation.
pub fn status_dot(key: impl Into<SharedString>, status: &str, ledger: &Ledger) -> gpui_kit::AnyElement {
    let at = |element: gpui_kit::AnyElement| div().flex().items_center().size(px(size::STATUS_BOX)).flex_shrink_0().child(element).into_any_element();
    match status {
        "working" => at(Glyph::Arc
            .at(size::STATUS_BOX)
            .text_color(ledger.accent)
            .with_animation(
                SharedString::from(format!("{}-spin", key.into())),
                Animation::new(motion::SPIN_SLOW).repeat(),
                |icon, delta| icon.transform(Transformation::rotate(percentage(delta))),
            )
            .into_any_element()),
        "waiting" | "needs-review" => div().size(px(size::STATUS_BOX)).flex_shrink_0().rounded_full().bg(ledger.warning).into_any_element(),
        "failed" => div().size(px(size::STATUS_BOX)).flex_shrink_0().rounded_full().bg(ledger.danger).into_any_element(),
        "exited" | "done" => div()
            .w(px(size::STATUS_BOX))
            .h(px(size::STATUS_DASH_H))
            .flex_shrink_0()
            .rounded(px(1.0))
            .bg(if status == "done" { ledger.success } else { ledger.faint })
            .into_any_element(),
        _ => at(Glyph::Ring9.at(size::STATUS_BOX).text_color(ledger.faint).into_any_element()),
    }
}
