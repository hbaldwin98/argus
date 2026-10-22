//! The question an action asks before it is sent (EXTVIEW-17 to -19): `<action label>?`, the
//! extension's question, `from <extension>` in the foot, Cancel with the `Esc` cap and the keys, and
//! a danger button named by the action.
//!
//! The session holds the question (`ExtensionsPage::confirm`) and the dialog is up while it does:
//! drawn once per question and taken away when the page says there is none, which is also what
//! happens when the extension publishes the view again without the row or the action. Yes sends
//! `AnswerExtension { yes: true }`; Cancel, the close button, Escape and a press on the scrim send
//! `yes: false`. A question is answered once: the dialog closes on the answer, and a page heard
//! again before the session has cleared it does not put it back up.

use std::rc::Rc;

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::extensions::views::ExtConfirm;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::v_flex;
use gpui_kit::{div, prelude::*, App, FocusHandle, Global, SharedString, TestSupportExt as _, Window};

use crate::connection::caller::Caller;
use crate::kit::button::{button, Look};
use crate::kit::dialog::{esc, foot, head, why};
use crate::kit::overlay::host::{Dismiss, Overlays, Spec};
use crate::kit::overlay::kind::Overlay;
use crate::style::Typed as _;
use crate::theme::scale::{pixels, rem};
use crate::theme::Tokens;

/// What the window knows of the question beyond what the page says.
#[derive(Default)]
struct Asked {
    /// The question up is this one's, so a page with none does not close another dialog.
    up: bool,
    /// It has been answered, and the session has not yet said so: the page heard in between must
    /// not put it up again.
    answered: bool,
}

impl Global for Asked {}

fn answered(cx: &App) -> bool {
    cx.try_global::<Asked>().is_some_and(|asked| asked.answered)
}

/// Answers the question, once, and takes the dialog away.
fn answer(yes: bool, caller: &Caller, window: &mut Window, cx: &mut App) {
    if answered(cx) {
        return;
    }
    cx.default_global::<Asked>().answered = true;
    caller.call(Call::AnswerExtension { yes });
    Overlays::get(cx).close(Overlay::ExtensionConfirm, window, cx);
    cx.default_global::<Asked>().up = false;
}

/// Puts the question up or takes it away to match what the page holds.
pub fn follow(confirm: Option<&ExtConfirm>, caller: &Caller, window: &mut Window, cx: &mut App) {
    let host = Overlays::get(cx);
    let Some(confirm) = confirm else {
        let was = std::mem::take(cx.default_global::<Asked>());
        if was.up && host.is_open(Overlay::ExtensionConfirm) {
            host.close(Overlay::ExtensionConfirm, window, cx);
        }
        return;
    };
    if host.is_open(Overlay::ExtensionConfirm) || answered(cx) {
        return;
    }
    cx.default_global::<Asked>().up = true;
    let confirm = confirm.clone();
    let keep: FocusHandle = cx.focus_handle();
    let no = caller.clone();
    let (asking, answering) = (caller.clone(), caller.clone());
    host.open(
        Spec::new(Overlay::ExtensionConfirm).dismissed(Dismiss::Answers(Rc::new(move |window, cx| answer(false, &no, window, cx)))),
        move |dialog: Dialog, window, cx, first| {
            // Cancel has the keys, without a ring, so a stray Enter answers no.
            if first {
                window.focus(&keep, cx);
            }
            let tokens = *Tokens::global(cx);
            let (no, yes, close, ok) = (asking.clone(), answering.clone(), asking.clone(), asking.clone());
            dialog
                .w(pixels(tokens.dialog_w_sm, window.rem_size()))
                .p_0()
                .margin_top((window.viewport_size().height - pixels(height(&tokens), window.rem_size())) / 2.0)
                // Enter on Cancel is a press on it.
                .on_ok(move |_, window, cx| {
                    answer(false, &ok, window, cx);
                    true
                })
                .child(body(
                    &confirm,
                    &keep,
                    move |window, cx| answer(false, &no, window, cx),
                    move |window, cx| answer(true, &yes, window, cx),
                    move |window, cx| answer(false, &close, window, cx),
                    &tokens,
                ))
        },
        window,
        cx,
    );
}

/// How tall the dialog is in the page's pixels, for centring it as the page does: a one-line
/// question under its head over its foot.
fn height(tokens: &Tokens) -> f32 {
    let line = tokens.type_body.size * 1.4;
    let head = tokens.sp_24 + tokens.sp_32.max(line) + tokens.sp_16;
    let foot = 2.0 * tokens.sp_16 + tokens.control_h + tokens.hairline;
    head + line + tokens.sp_24 + foot
}

fn body(
    confirm: &ExtConfirm,
    keep: &FocusHandle,
    cancel: impl Fn(&mut Window, &mut App) + 'static,
    yes: impl Fn(&mut Window, &mut App) + 'static,
    close: impl Fn(&mut Window, &mut App) + 'static,
    tokens: &Tokens,
) -> impl IntoElement {
    let title = SharedString::from(format!("{}?", confirm.label));
    let question = SharedString::from(confirm.question.clone());
    v_flex()
        .id("extension-confirm")
        .test_support()
        .child(head("ext-confirm", div().id("extension-confirm-title").test_support().aria_label(title.clone()).child(title), close, None, tokens))
        .child(
            div()
                .id("extension-confirm-body")
                .test_support()
                .aria_label(question.clone())
                .px(rem(tokens.sp_24))
                .pb(rem(tokens.sp_24))
                .typed(tokens.type_body)
                .text_color(tokens.fg_muted)
                .child(question),
        )
        .child(
            foot(tokens)
                .child(why(format!("from {}", confirm.extension), tokens))
                .child(
                    button("ext-confirm-keep", "Cancel", Look::Plain, false, tokens)
                        .track_focus(keep)
                        .focus_visible(|style| style)
                        .child("Cancel")
                        .child(esc(tokens))
                        .on_click(move |_, window, cx| cancel(window, cx)),
                )
                .child(button("ext-confirm-yes", confirm.label.clone(), Look::Danger, false, tokens).child(SharedString::from(confirm.label.clone())).on_click(move |_, window, cx| yes(window, cx))),
        )
}

#[cfg(test)]
mod tests;
