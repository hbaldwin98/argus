//! The ask a reconcile starts with (BOARD-RECONCILE-3, -4): what to reconcile in the person's words, the
//! agent to ask as a radio group (or the word that none runs here), and Send the ask.

use argus_desktop_core::board::tracker::reconcile_dialog::{ReconcileGesture, ReconcilePage};
use gpui_kit::component::input::Textarea;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, Context, Role, SharedString, TestSupportExt as _};

use super::ReconcileView;
use crate::kit::button::{button, off, Look};
use crate::kit::dialog::meta;
use crate::kit::choice::radio_row;
use crate::kit::keycap::keycap;
use crate::style::Typed as _;
use crate::theme::scale::{rem, Type};
use crate::theme::Tokens;

impl ReconcileView {
    pub(super) fn ask(&self, page: &ReconcilePage, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let small = |words: &'static str| div().typed(tokens.type_small).text_color(tokens.fg_muted).child(words);
        let agents = if page.agents.is_empty() {
            meta("No agent runs in this repository. Start one, then ask it here.", tokens).text_color(tokens.attention).into_any_element()
        } else {
            v_flex()
                .id("reconcile-agents")
                .role(Role::RadioGroup)
                .test_support()
                .aria_label("The agent to ask")
                .gap(rem(tokens.sp_6))
                .children(page.agents.iter().map(|agent| {
                    let pane = agent.pane;
                    h_flex()
                        .gap(rem(tokens.sp_8))
                        .items_center()
                        .child(radio_row(("reconcile-agent", pane as usize), SharedString::from(agent.title.clone()), agent.chosen, false, tokens, { let me = cx.entity(); move |_, cx| me.update(cx, |this, _| this.send(ReconcileGesture::Agent { pane })) }))
                        .children(agent.last.then(|| small("did the last one")))
                }))
                .into_any_element()
        };
        let enter = keycap("Ctrl+Enter", SharedString::from(tokens.font_mono), tokens).text_color(tokens.fg_on_accent).border_color(tokens.fg_on_accent.opacity(0.5));
        let send = if page.agents.is_empty() {
            off("reconcile-send", "Send the ask", true, tokens).child("Send the ask").child(keycap("Ctrl+Enter", SharedString::from(tokens.font_mono), tokens)).into_any_element()
        } else {
            button("reconcile-send", "Send the ask", Look::Primary, true, tokens)
                .child("Send the ask")
                .child(enter)
                .on_click(cx.listener(|this, _, _, cx| this.send(ReconcileGesture::Ask { ask: this.ask.text(cx).to_string() })))
                .into_any_element()
        };
        v_flex()
            .id("reconcile-ask")
            .gap(rem(tokens.sp_12))
            .child(div().typed(Type::new(tokens.weight_medium, tokens.fs_md, 1.4)).text_color(tokens.fg).child("You say what; it proposes; you tick"))
            .child(div().id("reconcile-ask-text").child(Textarea::new(self.ask.state()).aria_label("What to reconcile").h(rem(tokens.sp_24 * 4.0))))
            .child(agents)
            .child(h_flex().gap(rem(tokens.sp_8)).items_center().child(div().flex_1().min_w_0().child(small("It reads only, then waits for you: nothing is written until you tick it."))).child(send))
            .into_any_element()
    }
}
