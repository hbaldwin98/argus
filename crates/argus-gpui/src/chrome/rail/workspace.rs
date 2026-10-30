//! The dialog that makes a workspace: the switcher's `New workspace...`, one field and the daemon's
//! answer.
//!
//! The name is sent as typed (`Call::CreateWorkspace`) and the session says what became of it
//! (`Rail::new_workspace`): asked, or refused with the daemon's words, which this shows under the
//! field in the danger colour while the field keeps what was typed. The dialog goes when that
//! answer is gone after having been there, which is the daemon having listed the workspace open,
//! and it predicts nothing. It is an overlay of the host (`kit/overlay`), so it takes the keys,
//! closes on Escape and gives them back.

use std::rc::Rc;

use argus_desktop_core::chrome::projects::{NEW_WORKSPACE_HINT, NEW_WORKSPACE_PLACEHOLDER, NEW_WORKSPACE_TITLE};
use argus_desktop_core::connection::page::Call;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::Input;
use gpui_kit::component::v_flex;
use gpui_kit::{div, prelude::*, App, Context, Entity, Role, SharedString, Subscription, TestSupportExt as _, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::button::{button, off, Look};
use crate::kit::dialog::{esc, field_label, foot, head, why};
use crate::kit::field::Field;
use crate::kit::overlay::host::{Dismiss, Overlays, Spec};
use crate::kit::overlay::kind::Overlay;
use crate::theme::scale::{pixels, rem};
use crate::theme::Tokens;

struct NewWorkspaceView {
    heard: Entity<Heard>,
    caller: Caller,
    name: Field,
    /// Create was pressed: the session's answer is what this waits on.
    asked: bool,
    /// The session has held the ask since, so its being gone is the answer and not the rail before it.
    seen: bool,
    _heard: Subscription,
}

impl NewWorkspaceView {
    fn new(heard: Entity<Heard>, caller: Caller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name = Field::new(NEW_WORKSPACE_PLACEHOLDER, window, cx);
        let subscription = cx.observe_in(&heard, window, |this, _, window, cx| this.follow(window, cx));
        Self { heard, caller, name, asked: false, seen: false, _heard: subscription }
    }

    /// Takes the dialog away when the session has let go of the ask.
    fn follow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.heard.read(cx).rail.new_workspace.is_some() {
            self.seen = true;
        } else if self.asked && self.seen {
            Overlays::get(cx).close(Overlay::NewWorkspace, window, cx);
            return;
        }
        cx.notify();
    }

    fn create(&mut self, cx: &mut Context<Self>) {
        if self.heard.read(cx).held() || self.heard.read(cx).rail.new_workspace.as_ref().is_some_and(|n| n.creating) {
            return;
        }
        self.asked = true;
        self.seen = false;
        self.caller.call(Call::CreateWorkspace { name: self.name.text(cx).to_string() });
    }

    /// Escape, Cancel and the close button: the session forgets the ask, and the dialog goes.
    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.caller.call(Call::DismissNewWorkspace);
        Overlays::get(cx).close(Overlay::NewWorkspace, window, cx);
    }
}

impl Render for NewWorkspaceView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let state = self.heard.read(cx).rail.new_workspace.clone();
        let creating = state.as_ref().is_some_and(|n| n.creating);
        let refusal = state.and_then(|n| n.refusal);
        let (shut, cancelling) = (cx.entity(), cx.entity());
        let create = if creating {
            off("nw-create", "Creating\u{2026}", false, &tokens).child("Creating\u{2026}").into_any_element()
        } else {
            button("nw-create", "Create", Look::Primary, false, &tokens).child("Create").on_click(cx.listener(|this, _, _, cx| this.create(cx))).into_any_element()
        };
        v_flex()
            .id("new-workspace")
            .role(Role::Dialog)
            .test_support()
            .aria_label(NEW_WORKSPACE_TITLE)
            .child(head("nw", NEW_WORKSPACE_TITLE, move |window, cx| shut.update(cx, |this, cx| this.cancel(window, cx)), None, &tokens))
            .child(
                v_flex()
                    .gap(rem(tokens.sp_6))
                    .px(rem(tokens.sp_24))
                    .pb(rem(tokens.sp_24))
                    .child(field_label("Name", &tokens))
                    .child(Input::new(self.name.input()).aria_label("Name").h(rem(tokens.control_h)))
                    .children(refusal.map(|refusal| {
                        div().id("nw-refusal").role(Role::Alert).test_support().aria_label(SharedString::from(refusal.clone())).text_color(tokens.danger).child(SharedString::from(refusal))
                    })),
            )
            .child(
                foot(&tokens)
                    .child(div().id("nw-hint").test_support().aria_label(NEW_WORKSPACE_HINT).flex_1().min_w_0().child(why(NEW_WORKSPACE_HINT, &tokens)))
                    .child(
                    button("nw-cancel", "Cancel", Look::Plain, false, &tokens)
                        .child("Cancel")
                        .child(esc(&tokens))
                        .on_click(move |_, window, cx| cancelling.update(cx, |this, cx| this.cancel(window, cx))),
                )
                .child(create),
            )
    }
}

/// Opens the dialog over the window, the field with the keys.
pub fn open(caller: Caller, heard: Entity<Heard>, window: &mut Window, cx: &mut App) {
    let dismissing = caller.clone();
    let view = cx.new(|cx| NewWorkspaceView::new(heard, caller, window, cx));
    Overlays::get(cx).open(
        Spec::new(Overlay::NewWorkspace).dismissed(Dismiss::Answers(Rc::new(move |_, _| dismissing.call(Call::DismissNewWorkspace)))),
        move |dialog: Dialog, window, cx, first| {
            if first {
                view.read(cx).name.clone().focus(window, cx);
            }
            let confirming = view.clone();
            let tokens = *Tokens::global(cx);
            dialog
                .w(pixels(tokens.dialog_w, window.rem_size()))
                .p_0()
                // Enter in the field asks, and the dialog stays until the daemon has answered.
                .on_ok(move |_, _, cx| {
                    confirming.update(cx, |this, cx| this.create(cx));
                    false
                })
                .child(view.clone())
        },
        window,
        cx,
    );
}

#[cfg(test)]
mod tests;
