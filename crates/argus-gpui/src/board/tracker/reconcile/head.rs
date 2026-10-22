//! The reconcile dialog's head and foot (BOARD-RECONCILE-2, -11): what was reconciled with what, what was
//! asked, who proposed it and when, what the agent has and has not done; and the count ticked with the
//! ways out.

use argus_desktop_core::board::tracker::reconcile_dialog::{ReconcileGesture, ReconcilePage};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, Context, SharedString, TestSupportExt as _};

use super::ReconcileView;
use crate::kit::button::{button, off, Look};
use crate::kit::dialog::{esc, foot, meta};
use crate::kit::icon_button::icon_button;
use crate::kit::icons::Glyph;
use crate::kit::keycap::keycap;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::{rem, Type};
use crate::theme::Tokens;

/// What a proposal says of itself under its title.
const ONLY_READ: &str = "The agent has only read. Every line starts unticked: it writes nothing, in Argus or in a tracker, that you have not ticked.";
/// What the count in the foot says its keys are.
const KEYS: &str = "J and K move, Space ticks, E edits the text, G goes to the agent.";


impl ReconcileView {
    pub(super) fn head(&self, page: &ReconcilePage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let title = SharedString::from(if page.tickets.is_empty() { format!("Reconcile #{}", page.number) } else { format!("Reconcile #{} with {}", page.number, page.tickets) });
        let said = page.by.as_ref().map_or(String::new(), |by| {
            format!("{} {by} \u{b7} {} ago \u{b7} ", if page.stage == "asked" { "asked of" } else { "proposed by" }, page.ago.clone().unwrap_or_default())
        }) + page.reconciled.as_deref().unwrap_or("never reconciled before");
        let label = Type::new(tokens.weight_medium, tokens.fs_md, 1.4);
        h_flex()
            .flex_shrink_0()
            .items_start()
            .gap(rem(tokens.sp_12))
            .px(rem(tokens.sp_24))
            .pt(rem(tokens.sp_24))
            .pb(rem(tokens.sp_16))
            .border_b_1()
            .border_color(tokens.line)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(rem(tokens.sp_6))
                    .child(div().id("reconcile-title").test_support().aria_label(title.clone()).typed(tokens.type_title).text_color(tokens.fg).child(title))
                    .children((!page.asked.is_empty()).then(|| {
                        v_flex().mt(rem(tokens.sp_12 - tokens.sp_6)).child(div().typed(label).text_color(tokens.fg).child("You asked ")).child(
                            div().id("reconcile-asked").test_support().aria_label(SharedString::from(page.asked.clone())).text_color(tokens.fg).child(SharedString::from(format!("\u{201c}{}\u{201d}", page.asked))),
                        )
                    }))
                    .child(meta(said, tokens))
                    .children((page.stage == "proposed").then(|| meta(ONLY_READ, tokens)))
                    .children((page.stage == "carrying").then(|| {
                        let by = page.by.clone().unwrap_or_default();
                        meta(format!("{by} is carrying out the {} you ticked; nothing more is ticked until it reports.", page.ticked), tokens).text_color(tokens.attention)
                    })),
            )
            .child(
                icon_button("reconcile-close", "Close", false, tokens)
                    .on_click(cx.listener(|this, _, _, _| this.send(ReconcileGesture::Close)))
                    .child(Glyph::Close.at(tokens.icon_sm)),
            )
    }

    pub(super) fn foot(&self, page: &ReconcilePage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let mono = || SharedString::from(tokens.font_mono);
        let proposed = page.stage == "proposed";
        let discard = if page.stage == "asked" { "Take the ask back" } else { "Discard proposal" };
        let carry = format!("Carry out the {} ticked", page.ticked);
        let enter = keycap("Enter", mono(), tokens).text_color(tokens.fg_on_accent).border_color(tokens.fg_on_accent.opacity(0.5));
        foot(tokens)
            .child(
                div()
                    .id("reconcile-why")
                    .flex_1()
                    .min_w_0()
                    .typed(tokens.type_small)
                    .text_color(tokens.fg_muted)
                    .tooltip(tip(KEYS))
                    .children(proposed.then(|| SharedString::from(format!("{} of {} ticked", page.ticked, page.total)))),
            )
            .children((page.stage != "none").then(|| {
                button("reconcile-discard", discard, Look::Plain, false, tokens)
                    .child(discard)
                    .child(keycap("D", mono(), tokens))
                    .on_click(cx.listener(|this, _, _, _| this.send(ReconcileGesture::Discard)))
            }))
            .child(button("reconcile-later", "Later", Look::Plain, false, tokens).child("Later").child(esc(tokens)).on_click(cx.listener(|this, _, _, _| this.send(ReconcileGesture::Close))))
            .children(proposed.then(|| {
                if page.ticked > 0 {
                    button("reconcile-carry", carry.clone(), Look::Primary, false, tokens)
                        .child(SharedString::from(carry))
                        .child(enter)
                        .on_click(cx.listener(|this, _, _, _| this.send(ReconcileGesture::CarryOut)))
                        .into_any_element()
                } else {
                    off("reconcile-carry", carry.clone(), false, tokens).child(SharedString::from(carry)).child(keycap("Enter", mono(), tokens)).into_any_element()
                }
            }))
    }
}
