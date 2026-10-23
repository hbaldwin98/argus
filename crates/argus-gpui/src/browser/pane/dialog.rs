//! A script's dialog over a cell's page: headless Chrome draws none, and the page is stopped until
//! it is answered, so the window asks the person. An `alert` has OK, a `confirm` Cancel and OK, a
//! `prompt` a field beside them that starts at the text the page offered, and a leave-this-page
//! question Stay and Leave.
//!
//! The card is drawn over the picture, not the window: it is the page's question, and the rest of
//! the window goes on. An answer is sent and the card stays until the daemon says the dialog
//! closed, so an agent's tool answering first takes it away as well. The page is stopped under it,
//! so the pointer goes no further than the card.

use argus_desktop_core::connection::page::Call;
use argus_protocol::{BrowserDialog, BrowserDialogKind, BrowserInput};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::component::input::Input;
use gpui_kit::{div, prelude::*, SharedString, TestSupportExt as _, Window};

use super::BrowserPane;
use crate::kit::button::{button, Look};
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The words on the buttons that refuse and that accept, the refusing one absent for an alert.
fn words(kind: BrowserDialogKind) -> (Option<&'static str>, &'static str) {
    match kind {
        BrowserDialogKind::Alert => (None, "OK"),
        BrowserDialogKind::Confirm | BrowserDialogKind::Prompt => (Some("Cancel"), "OK"),
        BrowserDialogKind::BeforeUnload => (Some("Stay"), "Leave"),
    }
}

/// What the page says over its words: whose question it is.
pub const ASKS: &str = "The page asks";

impl BrowserPane {
    /// Puts the prompt's text back to what a new dialog offers.
    pub(super) fn follow_dialog(&mut self, dialog: Option<&BrowserDialog>, window: &mut Window, cx: &mut gpui_kit::App) {
        if dialog == self.asked.as_ref() {
            return;
        }
        self.asked = dialog.cloned();
        if let Some(dialog) = dialog {
            self.prompt.put(&dialog.default_prompt, window, cx);
        }
    }

    /// The card, centred over the picture.
    pub(super) fn dialog(&self, dialog: &BrowserDialog, tokens: &Tokens) -> impl IntoElement {
        let pane = self.pane;
        let (refuse, accept) = words(dialog.kind);
        let prompt = dialog.kind == BrowserDialogKind::Prompt;
        let answer = |accept: bool| {
            let (caller, field) = (self.caller.clone(), self.prompt.clone());
            move |_: &gpui_kit::ClickEvent, _: &mut Window, cx: &mut gpui_kit::App| {
                let text = (prompt && accept).then(|| field.text(cx).to_string());
                caller.call(Call::BrowserInput { pane, input: BrowserInput::Answer { accept, text } });
            }
        };
        let buttons = h_flex()
            .justify_end()
            .gap(rem(tokens.sp_8))
            .when_some(refuse, |row, refuse| {
                row.child(button(SharedString::from(format!("browser-dialog-refuse-{pane}")), refuse, Look::Plain, false, tokens).child(refuse).on_click(answer(false)))
            })
            .child(button(SharedString::from(format!("browser-dialog-accept-{pane}")), accept, Look::Primary, false, tokens).child(accept).on_click(answer(true)));
        div()
            .absolute()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(tokens.scrim)
            // The page under it is stopped: a press, a move or the wheel here is the card's, never
            // the screen's to send on.
            .occlude()
            .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(
                v_flex()
                    .id(SharedString::from(format!("browser-dialog-{pane}")))
                    .test_support()
                    .aria_label(SharedString::from(dialog.message.clone()))
                    .w(rem(tokens.dialog_w_sm))
                    .max_w_full()
                    .gap(rem(tokens.sp_12))
                    .p(rem(tokens.sp_16))
                    .rounded(rem(tokens.r_lg))
                    .border_1()
                    .border_color(tokens.line)
                    .bg(tokens.bg_surface)
                    .shadow(tokens.shadow_dialog())
                    .child(div().typed(tokens.type_label).text_color(tokens.fg_muted).child(ASKS))
                    .child(div().text_color(tokens.fg).child(SharedString::from(dialog.message.clone())))
                    .when(prompt, |card| {
                        card.child(Input::new(self.prompt.input()).id(SharedString::from(format!("browser-dialog-prompt-{pane}"))).h(rem(tokens.control_h)))
                    })
                    .child(buttons),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_of_dialog_offers_the_answers_a_browser_does() {
        assert_eq!(words(BrowserDialogKind::Alert), (None, "OK"));
        assert_eq!(words(BrowserDialogKind::Confirm), (Some("Cancel"), "OK"));
        assert_eq!(words(BrowserDialogKind::BeforeUnload), (Some("Stay"), "Leave"));
    }
}
