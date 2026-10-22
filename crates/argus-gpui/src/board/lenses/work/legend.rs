//! The outline's legend (BOARD-WORK-8): a `?` beside the label asks for what each mark means, which
//! hangs under the header over the rows, so asking moves nothing; Escape puts it away before a form or
//! the feature. It is asked for, not drawn: the marks carry a tooltip each, and a row of words above
//! every outline was the loudest thing on it.

use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, AnyElement, Entity, TestSupportExt as _};

use super::marks::{state_mark, MARK_WORDS};
use crate::board::feature_view::OpenFeature;
use crate::kit::chips::cdot;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// `.legend-ask`: a round `?` button, 20px, on a hairline.
pub fn ask(open: bool, feature: &Entity<OpenFeature>, tokens: &Tokens) -> AnyElement {
    let toggling = feature.clone();
    div()
        .id("legend-ask")
        .test_support()
        .role(gpui_kit::Role::Button)
        .aria_label("What the marks mean")
        .aria_expanded(open)
        .flex()
        .items_center()
        .justify_center()
        .size(rem(tokens.sp_20))
        .rounded_full()
        .border_1()
        .border_color(tokens.line)
        .typed(tokens.type_small)
        .text_color(tokens.fg_faint)
        .cursor_pointer()
        .hover(|button| button.text_color(tokens.fg))
        .child("?")
        .on_click(move |_, _, cx| toggling.update(cx, |feature, cx| feature.toggle_legend(cx)))
        .into_any_element()
}

/// `.legend-pop`: under the header, over the rows, the marks and their words; with what changed since
/// the reconcile when a dot says it.
pub fn pop(changed: bool, tokens: &Tokens) -> AnyElement {
    let item = |id: gpui_kit::ElementId, mark: AnyElement, words: &'static str| h_flex().id(id).test_support().aria_label(words).gap(rem(tokens.sp_4)).items_center().child(mark).child(words);
    h_flex()
        .id("legend-pop")
        .test_support()
        .aria_label("What the marks mean, said")
        .absolute()
        .top(rem(tokens.legend_top))
        .left(rem(tokens.sp_8))
        .flex_wrap()
        .gap_x(rem(tokens.sp_12))
        .gap_y(rem(tokens.sp_4))
        .px(rem(tokens.sp_12))
        .py(rem(tokens.sp_8))
        .bg(tokens.bg_raised)
        .border_1()
        .border_color(tokens.line)
        .rounded(rem(tokens.r_md))
        .shadow(tokens.shadow_pop())
        .typed(tokens.type_small)
        .text_color(tokens.fg_muted)
        .children(MARK_WORDS.iter().enumerate().map(|(ix, (state, words))| {
            let (glyph, tone) = state_mark(state);
            item(("legend-item", ix).into(), div().text_color(tone.colour(tokens)).child(glyph).into_any_element(), words)
        }))
        .when(changed, |legend| legend.child(item("legend-changed".into(), cdot(tokens).ml_0().into_any_element(), "changed since reconcile")))
        .into_any_element()
}

