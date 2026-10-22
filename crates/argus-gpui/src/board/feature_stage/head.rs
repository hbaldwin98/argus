//! The index's head (`indexHead`): the filter field, and on the list the toggle that shows the held
//! and archived features (`.index-head`).
//!
//! Under 1200 px the field folds into the strip's search button: it is drawn only while that button
//! has opened it (`filtering`) or the filter has text, and on the board the whole head goes while
//! it is not open. The field is the stage's and outlives every snapshot, so what is being typed is
//! never rewritten by an older echo (BOARD-INDEX-9, `kit/field`).

use argus_desktop_core::board::index_words::fold_toggle;
use argus_desktop_core::connection::page::{Call, Features};
use argus_desktop_core::keys::keymap::Action;
use gpui_kit::component::input::Input;
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, SharedString, TestSupportExt as _};

use super::FeatureStage;
use crate::kit::button::{button, Look};
use crate::theme::scale::rem;
use crate::theme::Tokens;

impl FeatureStage {
    /// Whether the head is drawn at all: a narrow window hides the board's until the filter is opened.
    pub(super) fn head_shown(&self, features: &Features) -> bool {
        !(self.narrow && features.layout == "board" && !self.filtering)
    }

    pub(super) fn head(&self, features: &Features, tokens: &Tokens) -> impl IntoElement {
        let field = !self.narrow || self.filtering || !features.filter.is_empty();
        let on_board = features.layout == "board";
        let ctx = self.ctx.clone();
        h_flex()
            .id("feature-head")
            .test_support()
            .flex_shrink_0()
            .items_center()
            .gap(rem(tokens.sp_12))
            .px(rem(tokens.sp_20))
            .py(rem(tokens.sp_12))
            .when(field, |head| {
                head.child(div().id("feature-filter").test_support().w(rem(if on_board { tokens.fx_filter_board_w } else { tokens.fx_filter_w })).flex_shrink(1.0).min_w(rem(120.0)).child(
                    Input::new(self.filter.input()).id("feature-filter-field").h(rem(tokens.control_h)).aria_label("Filter features"),
                ))
            })
            .children(fold_toggle(features).map(|(words, folded)| {
                button("feature-fold", words.trim_end(), Look::Plain, false, tokens)
                    .child(SharedString::from(words))
                    .child(div().text_color(tokens.fg_faint).child(folded.to_string()))
                    .on_click(move |_, _, cx| ctx.send(Call::FeatureAction { action: Action::Folded }, cx))
            }))
    }
}
