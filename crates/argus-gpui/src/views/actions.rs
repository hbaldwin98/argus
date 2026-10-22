//! The buttons an agent put in a view, and what a press of one sends: the agent is told through the
//! daemon, as a refresh is asked of it, and the view changes only when the agent shows it again
//! (the page's `views/actions.ts`).
//!
//! A button waits, disabled with a spinner, while its press is unanswered; one the daemon cannot take
//! is disabled with the reason as its tooltip, each keeping its tone at 70%; one with a `confirm` first asks, as a yes and a no,
//! and the question is kept in `views/memory` so a redraw does not take it from under the person.

use std::rc::Rc;

use argus_desktop_core::connection::page::Call;
use argus_protocol::{Answer, Tone, ViewAction, ViewPress};
use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, Role, SharedString};

use super::blocks::pen::Pen;
use super::ctx::{Kind, ViewCtx};
use crate::kit::button::{button, inert, outlined, outlined_inert, Look};
use crate::kit::icon_button::Node;
use crate::kit::spinner::spinner;
use crate::kit::tip::tip;
use crate::theme::scale::rem;
use crate::theme::Tokens;
use crate::views::chip::tone_colour;

/// What a press carries besides the action: the selected rows' keys, or a form's id and answers. It
/// is read at the moment of the press, never when the button was drawn.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PressWith {
    pub rows: Vec<String>,
    pub form: Option<String>,
    pub answers: Vec<Answer>,
}

/// What a press of `action` in the view `ctx` is in asks Rust for. A saved view has no pane: the
/// daemon relays the press to whichever agent last saved it.
pub fn press_call(ctx: &ViewCtx, action: &ViewAction, with: PressWith) -> Call {
    let press = ViewPress { view: ctx.id.clone(), action: action.id.clone(), rows: with.rows, form: with.form, answers: with.answers };
    match ctx.kind {
        Kind::Saved => Call::PressSavedView { id: ctx.id.clone(), press },
        Kind::Agent => Call::PressView { pane: ctx.pane, press },
    }
}

/// Why a button cannot be pressed when the daemon cannot take presses.
pub const CANNOT_PRESS: &str = "This Argus daemon cannot take button presses";

/// `.btn.sm.vtone-*`: a good or an active action is the primary button; a bad, a warn and an info
/// one keep the plain button's fill and take the tone for the edge and the words, which stay there
/// under the pointer. `live` is whether it can be pressed: the tone's rule comes after
/// `.btn[disabled]` in the page's stylesheet, so a button that cannot be pressed keeps its look.
fn toned(id: SharedString, words: &str, tone: Tone, live: bool, tokens: &Tokens) -> Node {
    let words = words.to_owned();
    match (tone, live) {
        (Tone::Good | Tone::Active, true) => button(id, words, Look::Primary, true, tokens),
        (Tone::Good | Tone::Active, false) => inert(id, words, Look::Primary, true, tokens),
        (Tone::Bad | Tone::Warn | Tone::Info, true) => outlined(id, words, tone_colour(tone, tokens), true, tokens),
        (Tone::Bad | Tone::Warn | Tone::Info, false) => outlined_inert(id, words, tone_colour(tone, tokens), true, tokens),
        (Tone::Neutral, true) => button(id, words, Look::Plain, true, tokens),
        (Tone::Neutral, false) => inert(id, words, Look::Plain, true, tokens),
    }
}

/// A button for `action` in the view `ctx` is in. `with` is read at the press. `off` is a button that
/// cannot be pressed for want of what it acts on (a table's actions with no row ticked): it waits
/// as an unanswered one does, without the spinner.
pub fn action_button(action: &ViewAction, ctx: &ViewCtx, with: Rc<dyn Fn() -> PressWith>, off: bool, pen: &Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let key = format!("{}|{}", ctx.path, action.id);
    let id = SharedString::from(key.clone());
    let waiting = ctx.pressed.contains(&action.id);
    let (repaint, memory) = (pen.repaint, pen.memory.clone());
    let ask = {
        let (memory, key) = (memory.clone(), key.clone());
        move |open: bool, cx: &mut gpui_kit::App| {
            memory.borrow_mut().set_asking(&key, open);
            cx.notify(repaint);
        }
    };
    let send = {
        let (caller, ctx, action) = (pen.draw.caller.clone(), ctx.clone(), action.clone());
        move || caller.call(press_call(&ctx, &action, with()))
    };

    // As the page's `actions.ts` draws it: whatever is pending or the daemon can take.
    if memory.borrow().asking(&key) {
        let (yes, no, send) = (ask.clone(), ask, send);
        let question = action.confirm.clone().unwrap_or_default();
        return h_flex()
            .id(SharedString::from(format!("{key}-confirm")))
            .role(Role::Group)
            .test_support()
            .aria_label(question.clone())
            .gap(rem(tokens.sp_6))
            .flex_wrap()
            .child(div().text_color(tokens.fg_muted).text_size(rem(tokens.fs_sm)).child(SharedString::from(question)))
            .child(toned(SharedString::from(format!("{key}-yes")), "Yes", action.tone, true, tokens).child("Yes").on_click(move |_, _, cx| {
                yes(false, cx);
                send();
            }))
            .child(button(SharedString::from(format!("{key}-no")), "No", Look::Plain, true, tokens).child("No").on_click(move |_, _, cx| no(false, cx)))
            .into_any_element();
    }
    if waiting || !ctx.can_press || off {
        return toned(id, &action.label, action.tone, false, tokens)
            .opacity(tokens.opacity_wait)
            .when(!ctx.can_press, |button| button.tooltip(tip(CANNOT_PRESS)))
            .when(waiting, |button| button.child(spinner(&key, tokens)))
            .child(SharedString::from(action.label.clone()))
            .into_any_element();
    }
    let confirm = action.confirm.is_some();
    toned(id, &action.label, action.tone, true, tokens)
        .child(SharedString::from(action.label.clone()))
        .on_click(move |_, _, cx| {
            if confirm {
                ask(true, cx);
            } else {
                send();
            }
        })
        .into_any_element()
}

#[cfg(test)]
mod tests;
