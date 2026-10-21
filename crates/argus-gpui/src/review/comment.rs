//! The comment box under the picked lines: what it is about, the agent it
//! goes to, the words being written and the two ways out.
//!
//! Whether it is open, which agent is lit and what the lines are is the
//! session's (`CommentPage`); the words are the box's own, kept in its
//! textarea across every update and let go only when the session says the
//! box is shut, so the next starts empty and a refused send keeps them
//! (COMMENT-7). Ctrl Enter and Meta Enter both send, Escape puts it away, a
//! plain Enter is a newline, and none of the three reaches what is under the
//! box: the engine inserts the newline for every Enter before it says it was
//! pressed, so the send is caught ahead of it, on the card, in the capture
//! phase. The engine binds one of Ctrl and Meta as its secondary key (by
//! platform); the other arrives as a plain key and is caught the same way.
//! A box that is open takes the keys on every update, from wherever they were
//! (REVIEW-37): the Field layer holds every key, so the review's own keys
//! are not heard while one is written.
//!
//! The Code stage will share this box (`code-comment`); its calls are the
//! review's here until that stage is ported and gives it its own.

use argus_desktop_core::connection::page::{Call, ReviewClick};
use argus_desktop_core::review::comment_box::CommentPage;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Enter, Escape, Textarea, TextareaState};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::{h_flex, v_flex, Selectable as _, Sizable as _};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, px, Context, Entity, KeyDownEvent, Keystroke, Pixels, SharedString, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::diff::view::GUTTER;
use crate::theme::Ledger;

/// The room the box takes among the diff's rows: its card, and the margin
/// around it.
pub const HEIGHT: Pixels = px(188.0);
const ROWS: usize = 3;

pub struct CommentBox {
    heard: Entity<Heard>,
    caller: Caller,
    text: Entity<TextareaState>,
    page: Option<CommentPage>,
}

impl CommentBox {
    pub fn new(heard: Entity<Heard>, caller: Caller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let text = cx.new(|cx| TextareaState::new(window, cx).rows(ROWS).placeholder("Comment"));
        Self { heard, caller, text, page: None }
    }

    /// Follows the session: the page of the box while it is open. An open box
    /// takes the keys back on every update; a shut one lets its words go.
    pub fn follow(&mut self, page: Option<&CommentPage>, window: &mut Window, cx: &mut Context<Self>) {
        match page {
            Some(page) => {
                if self.page.as_ref() != Some(page) {
                    self.page = Some(page.clone());
                    cx.notify();
                }
                self.text.update(cx, |text, cx| text.focus(window, cx));
            }
            None => {
                if self.page.take().is_some() {
                    self.text.update(cx, |text, cx| text.set_value("", window, cx));
                    cx.notify();
                }
            }
        }
    }

    /// What is written now.
    pub fn words(&self, cx: &gpui_kit::App) -> SharedString {
        self.text.read(cx).value()
    }

    /// Sends the call unless the daemon is gone and it would reach no one.
    fn ask(&self, call: Call, cx: &Context<Self>) {
        if !self.heard.read(cx).held() {
            self.caller.call(call);
        }
    }

    /// The words as typed: the session trims, and refuses an empty one in a notice.
    fn send(&self, cx: &Context<Self>) {
        self.ask(Call::ReviewComment { body: Some(self.words(cx).to_string()) }, cx);
    }

    fn cancel(&self, cx: &Context<Self>) {
        self.ask(Call::ReviewComment { body: None }, cx);
    }

    /// Who it goes to: one agent that is lit is said, not offered; several, or
    /// none lit because the one chosen has gone, are each a button.
    fn recipients(&self, page: &CommentPage, ledger: &Ledger, cx: &mut Context<Self>) -> impl IntoElement {
        let lit = page.recipients.iter().find(|r| Some(r.pane) == page.to);
        let label = div().text_xs().text_color(ledger.faint).child("Send to");
        match lit {
            Some(lit) if page.recipients.len() == 1 => h_flex()
                .gap_2()
                .items_center()
                .child(label)
                .child(
                    div()
                        .id("comment-recipient")
                        .test_support()
                        .aria_label(SharedString::from(lit.title.clone()))
                        .text_sm()
                        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                        .child(SharedString::from(format!("\u{25c6} {}", lit.title))),
                )
                .into_any_element(),
            // A row that scrolls sideways rather than wrapping, which would grow the card.
            _ => h_flex()
                .id("comment-recipients")
                .gap_2()
                .items_center()
                .min_w_0()
                .overflow_x_scroll()
                .child(label)
                .children(page.recipients.iter().map(|recipient| {
                    let pane = recipient.pane;
                    Button::new(SharedString::from(format!("comment-recipient-{pane}")))
                        .small()
                        .selected(Some(pane) == page.to)
                        .label(SharedString::from(format!("\u{25c6} {}", recipient.title)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.ask(Call::ReviewClick { target: ReviewClick::Recipient { pane } }, cx)
                        }))
                }))
                .into_any_element(),
        }
    }
}

impl Render for CommentBox {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ledger = *Ledger::global(cx);
        let Some(page) = self.page.clone() else {
            return div().into_any_element();
        };
        let card = v_flex()
            .id("review-comment")
            .test_support()
            .size_full()
            .gap_2()
            .p_3()
            .rounded(px(8.0))
            .border_1()
            .border_color(ledger.line_strong)
            .bg(ledger.chrome)
            // Ahead of the textarea's own Enter, which would put a newline in
            // before it said anything was pressed, and ahead of its Escape.
            .capture_action(cx.listener(|this, action: &Enter, _, cx| {
                if action.secondary {
                    cx.stop_propagation();
                    this.send(cx);
                }
            }))
            // The engine binds only its own secondary key (Cmd on macOS, Ctrl
            // elsewhere); the other of Ctrl and Meta is no binding at all, so
            // it is caught here as a key, still ahead of the textarea (COMMENT-6).
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                let keys = &event.keystroke;
                if keys.key == "enter" && (keys.modifiers.control || keys.modifiers.platform) {
                    cx.stop_propagation();
                    this.send(cx);
                }
            }))
            .capture_action(cx.listener(|this, _: &Escape, _, cx| {
                cx.stop_propagation();
                this.cancel(cx);
            }))
            .child(
                h_flex()
                    .gap_2()
                    .items_baseline()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                            .text_color(ledger.accent)
                            .child("Comment"),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .text_color(ledger.faint)
                            .child(SharedString::from(page.about.clone())),
                    ),
            )
            .child(Textarea::new(&self.text).w_full().aria_label("Comment"))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(div().flex_1().min_w_0().child(self.recipients(&page, &ledger, cx)))
                    .child(
                        Button::new("comment-cancel")
                            .small()
                            .label("Cancel")
                            .on_click(cx.listener(|this, _, _, cx| this.cancel(cx))),
                    )
                    .child(
                        Button::new("comment-send")
                            .small()
                            .primary()
                            .child(
                                h_flex().gap_2().items_center().child("Comment").child(Kbd::new(
                                    Keystroke::parse("secondary-enter").expect("a key"),
                                )),
                            )
                            .on_click(cx.listener(|this, _, _, cx| this.send(cx))),
                    ),
            );
        div().size_full().pl(px(GUTTER)).pr_3().py_2().child(card).into_any_element()
    }
}
