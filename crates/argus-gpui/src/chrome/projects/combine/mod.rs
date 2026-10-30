//! The combine-projects dialog: fold the repositories of other projects into one, which keeps its
//! name and settings.
//!
//! The session holds the dialog (`Heard::combine`) and says everything on it: the project folded
//! into, which projects are ticked, what ticking would hold, the ask with the daemon and a refusal.
//! This draws that page and sends the gestures of `CombineGesture`; it predicts none of their
//! results, so a press on a row draws the tick when the page says it, and the dialog goes when the
//! daemon has answered and the session lets go of it.
//!
//! The dialog is an overlay of the host (`kit/overlay`). It is up exactly while the session holds
//! it, except that Escape, Cancel and the close button send `Close` and, as nothing can refuse
//! that, put it away without waiting for the page.

mod destination;
mod rows;
mod summary;

use std::rc::Rc;

use argus_desktop_core::chrome::combine_outcome::{button_label, can_combine, TITLE};
use argus_desktop_core::chrome::combine_page::{CombineGesture, CombinePage};
use argus_desktop_core::connection::page::Call;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::v_flex;
use gpui_kit::{div, prelude::*, App, Context, Entity, FocusHandle, Role, SharedString, Subscription, TestSupportExt as _, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::button::{button, off, Look};
use crate::kit::dialog::{esc, foot, head};
use crate::kit::overlay::host::{Dismiss, Overlays, Spec};
use crate::kit::overlay::kind::Overlay;
use crate::theme::scale::{pixels, rem};
use crate::theme::Tokens;

pub struct CombineView {
    heard: Entity<Heard>,
    caller: Caller,
    /// What has the keys when the dialog opens: Cancel, so Escape and Enter reach the dialog.
    focus: FocusHandle,
    /// The dialog was put away by a `Close` the page has not yet answered, so a page still open in
    /// the events before the answer does not bring it back.
    dismissed: bool,
    _heard: Subscription,
}

impl CombineView {
    pub fn new(heard: Entity<Heard>, caller: Caller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let subscription = cx.observe_in(&heard, window, |this, _, window, cx| this.follow(window, cx));
        Self { heard, caller, focus: cx.focus_handle(), dismissed: false, _heard: subscription }
    }

    /// What the session says the dialog is.
    fn page(&self, cx: &App) -> Option<CombinePage> {
        self.heard.read(cx).combine.clone()
    }

    /// Brings the dialog up and takes it away as the session holds it.
    fn follow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let host = Overlays::get(cx);
        if self.page(cx).is_none() {
            self.dismissed = false;
            host.close(Overlay::Combine, window, cx);
            return;
        }
        if !host.is_open(Overlay::Combine) && !self.dismissed {
            self.open(window, cx);
        }
        cx.notify();
    }

    fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (view, focus, closing, confirming) = (cx.entity(), self.focus.clone(), cx.entity(), cx.entity());
        Overlays::get(cx).open(
            Spec::new(Overlay::Combine).dismissed(Dismiss::Answers(Rc::new(move |_, cx| closing.update(cx, |this, cx| this.send_close(cx))))),
            move |dialog: Dialog, window, cx, first| {
                if first {
                    window.focus(&focus, cx);
                }
                let tokens = *Tokens::global(cx);
                let confirming = confirming.clone();
                dialog
                    .w(pixels(tokens.form_w, window.rem_size()))
                    .p_0()
                    // Enter asks, and the dialog stays until the daemon has answered.
                    .on_ok(move |_, _, cx| {
                        confirming.update(cx, |this, cx| this.combine(cx));
                        false
                    })
                    .child(view.clone())
            },
            window,
            cx,
        );
    }

    fn send(&self, gesture: CombineGesture) {
        self.caller.call(Call::CombineProjects { gesture });
    }

    fn combine(&mut self, cx: &mut Context<Self>) {
        let Some(page) = self.page(cx) else { return };
        if can_combine(&page) && !self.heard.read(cx).held() {
            self.send(CombineGesture::Combine);
        }
    }

    /// Escape, Cancel and the close button: the page is told, and the dialog goes.
    fn send_close(&mut self, _: &mut Context<Self>) {
        self.dismissed = true;
        self.send(CombineGesture::Close);
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.send_close(cx);
        Overlays::get(cx).close(Overlay::Combine, window, cx);
    }
}

impl Render for CombineView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let Some(page) = self.page(cx) else { return div().into_any_element() };
        let shut = cx.entity();
        let words = button_label(&page);
        let combine = if page.combining {
            off("cp-combine", "Combining\u{2026}", false, &tokens).child("Combining\u{2026}").into_any_element()
        } else if can_combine(&page) {
            button("cp-combine", words.clone(), Look::Primary, false, &tokens).child(SharedString::from(words)).on_click(cx.listener(|this, _, _, cx| this.combine(cx))).into_any_element()
        } else {
            off("cp-combine", words.clone(), false, &tokens).child(SharedString::from(words)).into_any_element()
        };
        v_flex()
            .id("combine-projects")
            .role(Role::Dialog)
            .test_support()
            .aria_label(TITLE)
            .max_h(rem(tokens.add_max_h))
            .child(head("cp", TITLE, move |window, cx| shut.update(cx, |this, cx| this.close(window, cx)), None, &tokens))
            .child(
                v_flex()
                    .id("cp-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .gap(rem(tokens.sp_12))
                    .px(rem(tokens.sp_24))
                    .pb(rem(tokens.sp_24))
                    .child(self.destination(&page, &tokens, cx))
                    .child(self.rows(&page, &tokens, cx))
                    .child(summary::lines(&page, &tokens))
                    .children(page.refusal.clone().map(|refusal| {
                        div().id("cp-refusal").role(Role::Alert).test_support().aria_label(SharedString::from(refusal.clone())).text_color(tokens.danger).child(SharedString::from(refusal))
                    })),
            )
            .child(
                foot(&tokens)
                    .justify_end()
                    .child(
                        button("cp-cancel", "Cancel", Look::Plain, false, &tokens)
                            .track_focus(&self.focus)
                            .focus_visible(|style| style)
                            .child("Cancel")
                            .child(esc(&tokens))
                            .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
                    )
                    .child(combine),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests;
