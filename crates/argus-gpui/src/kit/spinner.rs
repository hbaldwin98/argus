//! The ring that turns while something is asked and not yet answered: the page's `.vspin`.
//!
//! A button waiting on a press and a view's tab waiting on a refresh both say it with this, so the
//! look is here once.

use gpui_kit::{div, prelude::*, percentage, Animation, AnimationExt as _, AnyElement, SharedString, Transformation};

use crate::kit::icons::Glyph;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// `.vspin`: a ring turning, 10px, the accent on the quiet edge. `key` names the turn, so two
/// spinners on screen do not share one.
pub fn spinner(key: &str, tokens: &Tokens) -> AnyElement {
    div()
        .relative()
        .size(rem(tokens.spin_sm))
        .flex_shrink_0()
        .child(Glyph::Ring9.at(tokens.spin_sm).text_color(tokens.line_strong))
        .child(div().absolute().top_0().left_0().child(Glyph::Arc.at(tokens.spin_sm).text_color(tokens.accent).with_animation(
            SharedString::from(format!("{key}-spin")),
            Animation::new(tokens.motion_spin_slow).repeat(),
            |icon, delta| icon.transform(Transformation::rotate(percentage(delta))),
        )))
        .into_any_element()
}
