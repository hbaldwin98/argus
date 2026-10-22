//! The question before a project is removed (SHELL-SWITCH-6): an alert over the window while the
//! session holds one, `Remove <name>?`, with what leaves the sidebar and, when panes run in it, the
//! warning that argusd waits for them.
//!
//! The session holds the question (`Rail::removing`) and the dialog is up exactly while it does: it
//! comes with the rail that carries it and goes with the next that does not. `Keep it` has the keys,
//! and it, the close button, Escape and a press on the scrim all answer no; only `Remove` answers
//! yes, and the project leaves when the daemon's tree says so. The question is an overlay of the
//! host (`kit/overlay`), so the key walk's menu layer asks it to say no on Escape (SHELL-KEYS-19).

use std::rc::Rc;

use argus_desktop_core::chrome::projects::{question, Question};
use argus_desktop_core::connection::page::{Call, Removing};
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, App, FocusHandle, Global, SharedString, TestSupportExt as _, Window};

use crate::connection::caller::Caller;
use crate::kit::alert;
use crate::kit::dialog::head;
use crate::kit::overlay::host::{Dismiss, Overlays, Spec};
use crate::kit::overlay::kind::Overlay;
use crate::style::Typed as _;
use crate::theme::scale::{pixels, rem};
use crate::theme::Tokens;

/// Whether no has been said to the question the rail still carries. The session answers on its own
/// time, so a rail heard in between must not put the dialog up again (SHELL-SWITCH-6); it is
/// cleared by the first rail without a question, as the add dialog's `dismissed` is.
struct Declined(bool);

impl Global for Declined {}

/// Says no to the question: the answer of Escape, the scrim, the close button and `Keep it`.
fn say_no(caller: &Caller, cx: &mut App) {
    cx.set_global(Declined(true));
    caller.call(Call::AnswerRemoveProject { yes: false });
}

/// Puts the question up or takes it away to match what the rail holds.
pub fn follow(removing: Option<&Removing>, caller: &Caller, window: &mut Window, cx: &mut App) {
    let host = Overlays::get(cx);
    let Some(removing) = removing else {
        cx.set_global(Declined(false));
        host.close(Overlay::Question, window, cx);
        return;
    };
    if host.is_open(Overlay::Question) || cx.try_global::<Declined>().is_some_and(|declined| declined.0) {
        return;
    }
    let (words, caller_no) = (question(removing), caller.clone());
    let keep: FocusHandle = cx.focus_handle();
    let (asking, answering) = (caller.clone(), caller.clone());
    host.open(
        Spec::new(Overlay::Question).dismissed(Dismiss::Answers(Rc::new(move |_, cx| say_no(&caller_no, cx)))),
        move |dialog: Dialog, window, cx, first| {
            // `Keep it` has the keys, as the page focuses it, and without a ring.
            if first {
                window.focus(&keep, cx);
            }
            let tokens = *Tokens::global(cx);
            let (no, yes, close, cancel) = (asking.clone(), answering.clone(), asking.clone(), asking.clone());
            dialog
                .w(pixels(tokens.dialog_w_sm, window.rem_size()))
                .p_0()
                // Enter on `Keep it` is a press on it.
                .on_ok(move |_, _, cx| {
                    say_no(&cancel, cx);
                    true
                })
                .child(body(&words, &keep, move |_, cx| say_no(&no, cx), move |_, _| yes.call(Call::AnswerRemoveProject { yes: true }), move |_, cx| say_no(&close, cx), &tokens))
        },
        window,
        cx,
    );
}

/// The alert: the bin tile and the question, what leaves, the warning, and the two answers.
fn body(
    words: &Question,
    keep: &FocusHandle,
    keep_it: impl Fn(&mut Window, &mut App) + 'static,
    remove: impl Fn(&mut Window, &mut App) + 'static,
    close: impl Fn(&mut Window, &mut App) + 'static,
    tokens: &Tokens,
) -> impl IntoElement {
    let title = h_flex()
        .gap(rem(tokens.sp_12))
        .items_center()
        .child(alert::tile(tokens))
        .child(div().id("remove-title").test_support().aria_label(SharedString::from(words.title.clone())).min_w_0().child(SharedString::from(words.title.clone())));
    let callout = words.warning.as_ref().map(|warning| alert::callout("remove-warning", warning.clone(), tokens));
    v_flex()
        .id("project-remove")
        .child(head("remove", title, close, None, tokens))
        .child(
            v_flex()
                .gap(rem(tokens.sp_12))
                .px(rem(tokens.sp_24))
                .pb(rem(tokens.sp_24))
                .typed(tokens.type_body)
                .text_color(tokens.fg_muted)
                .child(div().id("remove-body").test_support().aria_label(SharedString::from(words.body.clone())).child(SharedString::from(words.body.clone())))
                .children(callout),
        )
        .child(alert::answers("remove", "Keep it", "Remove", keep, keep_it, remove, tokens))
}

#[cfg(test)]
mod tests;
