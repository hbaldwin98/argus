//! The line under the cursor in full, at the right of a proposal (BOARD-RECONCILE-9, -10, -20): its
//! heading and whether it is ticked, what it does and its key, how it failed last time, its text word
//! for word (editable only while proposed), the call the agent will make, and what happens afterwards.

use argus_desktop_core::board::tracker::reconcile_dialog::{ReconcilePage, SelectedLine};
use gpui_kit::component::input::Textarea;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, Role, SharedString, TestSupportExt as _};

use super::ReconcileView;
use crate::kit::dialog::{label as named, meta};
use crate::kit::keycap::keycap;
use crate::style::Typed as _;
use crate::theme::scale::{rem, Type};
use crate::theme::Tokens;

/// What a ticked line is followed by.
const AFTERWARDS: &str = "The agent reports each ticked line as done or failed. A failed line comes back unticked, with the tracker\u{2019}s error.";

/// `.label` in a dialog: sentence case, medium.
fn label(words: &str, tokens: &Tokens) -> gpui_kit::Div {
    div().typed(Type::new(tokens.weight_medium, tokens.fs_md, 1.4)).text_color(tokens.fg).child(SharedString::from(words.to_owned()))
}


impl ReconcileView {
    fn words(&self, page: &ReconcilePage, line: &SelectedLine, tokens: &Tokens) -> impl IntoElement {
        let editable = page.stage == "proposed";
        v_flex()
            .gap(rem(tokens.sp_6))
            .child(
                h_flex()
                    .gap(rem(tokens.sp_8))
                    .items_center()
                    .child(label("The text, word for word", tokens))
                    .children(line.edited.then(|| {
                        div()
                            .id("reconcile-edited")
                            .test_support()
                            .aria_label("edited")
                            .px(rem(tokens.sp_8))
                            .py(rem(tokens.hairline))
                            .rounded(rem(tokens.r_pill))
                            .bg(tokens.fill)
                            .text_size(rem(tokens.fs_xs))
                            .weighted(tokens.weight_semibold)
                            .text_color(tokens.fg)
                            .child("edited")
                    })),
            )
            .child(
                div().id("reconcile-text").child(
                    Textarea::new(self.text.state())
                        .aria_label("The text, word for word")
                        .readonly(!editable)
                        .h(rem(tokens.sp_24 * 4.0))
                ),
            )
            .children(editable.then(|| {
                h_flex()
                    .gap(rem(tokens.sp_4))
                    .items_center()
                    .typed(tokens.type_small)
                    .text_color(tokens.fg_muted)
                    .child(keycap("E", SharedString::from(tokens.font_mono), tokens))
                    .child("edits it here before you tick it.")
            }))
    }

    /// The right column: the line open, or what to say when the agent proposed none.
    pub(super) fn detail(&self, page: &ReconcilePage, tokens: &Tokens) -> AnyElement {
        let Some(line) = &page.selected else {
            return meta("The agent noticed things, and proposes nothing.", tokens).flex_1().min_w_0().into_any_element();
        };
        let editable = page.stage == "proposed";
        v_flex()
            .id("reconcile-detail")
            .role(Role::Group)
            .test_support()
            .aria_label("The line open")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .gap(rem(tokens.sp_12))
            .child(
                v_flex()
                    .gap(rem(tokens.sp_6))
                    .child(h_flex().items_center().child(label(&line.heading, tokens)).child(div().flex_1()).child(label(if line.ticked { "ticked" } else { "unticked" }, tokens)))
                    .child(div().typed(tokens.type_title).text_color(tokens.fg).child(SharedString::from(format!("{} \u{b7} {}", line.does, line.key))))
                    .children(line.failed.clone().map(|why| meta(format!("Failed last time: {why}"), tokens).text_color(tokens.danger))),
            )
            .children((!line.text.is_empty() || editable).then(|| self.words(page, line, tokens)))
            .children((!line.call.is_empty()).then(|| {
                v_flex().gap(rem(tokens.sp_6)).child(named("What the agent will do", tokens)).child(
                    div()
                        .id("reconcile-call")
                        .test_support()
                        .aria_label(SharedString::from(line.call.clone()))
                        .px(rem(tokens.sp_12))
                        .py(rem(tokens.sp_8))
                        .rounded(rem(tokens.r_sm))
                        .bg(tokens.bg_sunken)
                        .font_family(SharedString::from(tokens.font_mono))
                        .text_size(rem(tokens.fs_sm))
                        .text_color(tokens.fg)
                        .child(SharedString::from(line.call.clone())),
                )
            }))
            .child(v_flex().gap(rem(tokens.sp_6)).child(named("Afterwards", tokens)).child(meta(AFTERWARDS, tokens)))
            .into_any_element()
    }
}
