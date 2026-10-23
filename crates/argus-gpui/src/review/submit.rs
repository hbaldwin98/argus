//! The review being sent: every draft listed, a summary to open it with, the agent it goes to, and
//! the discard and the send. Opened from the Review button over the diff, and shut by it, by its
//! close, or once a send has emptied the review.
//!
//! The drafts, the agents and whether a send is out are the session's (`DraftsPage`); the summary
//! and the agent picked are the panel's until it sends them, and a send that is refused keeps both.

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::review::drafts::{DraftGesture, DraftsPage};
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, Context, Entity, SharedString, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::button::{button, chip, Look};
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// How wide the panel stands over the diff, in the page's pixels.
const WIDE: f32 = 420.0;

pub struct Submit {
    heard: Entity<Heard>,
    caller: Caller,
    summary: Entity<TextareaState>,
    /// The agent picked here, over the one the session would send to.
    to: Option<u64>,
    pub open: bool,
    /// A send was out at the last update, so the review emptied by it shuts the panel.
    sending: bool,
}

impl Submit {
    pub fn new(heard: Entity<Heard>, caller: Caller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let summary = cx.new(|cx| TextareaState::new(window, cx).rows(3).placeholder("Leave a summary (optional)"));
        Self { heard, caller, summary, to: None, open: false, sending: false }
    }

    /// Follows the session: a send answered with an empty review shuts the panel and lets the
    /// summary go; one refused leaves both.
    pub fn follow(&mut self, page: Option<&DraftsPage>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(page) = page else {
            self.open = false;
            return;
        };
        if self.sending && !page.sending && page.drafts.is_empty() {
            self.open = false;
            self.to = None;
            self.summary.update(cx, |summary, cx| summary.set_value("", window, cx));
        }
        self.sending = page.sending;
        cx.notify();
    }

    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        self.open = !self.open;
        cx.notify();
    }

    fn ask(&self, gesture: DraftGesture, cx: &Context<Self>) {
        if !self.heard.read(cx).held() {
            self.caller.call(Call::Drafts { gesture });
        }
    }

    fn open_draft(&self, staged: bool, path: String, cx: &Context<Self>) {
        if !self.heard.read(cx).held() {
            self.caller.call(Call::OpenChange { staged, path });
        }
    }
}

impl Render for Submit {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let page = self.heard.read(cx).review.as_ref().and_then(|review| review.drafts.clone());
        let Some(page) = page.filter(|_| self.open) else {
            return div().into_any_element();
        };
        let to = self.to.filter(|p| page.recipients.iter().any(|r| r.pane == *p)).or(page.to);
        let count = page.drafts.len();
        let list = v_flex()
            .id("review-drafts")
            .test_support()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(rem(tokens.sp_6))
            .gap(rem(tokens.sp_2))
            .when(page.drafts.is_empty(), |list| {
                list.child(
                    div()
                        .p(rem(tokens.sp_16))
                        .typed(tokens.type_small)
                        .text_color(tokens.fg_faint)
                        .child("No pending comments. Hover a line and press + to add one."),
                )
            })
            .children(page.drafts.iter().map(|draft| {
                let (staged, path) = (draft.staged, draft.path.clone());
                v_flex()
                    .id(("review-draft", draft.id as usize))
                    .test_support()
                    .aria_label(SharedString::from(format!("{} {}", draft.at, draft.body)))
                    .gap(rem(tokens.sp_2))
                    .px(rem(tokens.sp_12))
                    .py(rem(tokens.sp_8))
                    .rounded(rem(tokens.r_sm))
                    .cursor_pointer()
                    .hover(|row| row.bg(tokens.fill))
                    .on_click(cx.listener(move |this, _, _, cx| this.open_draft(staged, path.clone(), cx)))
                    .child(
                        div()
                            .font_family(SharedString::from(tokens.font_mono))
                            .text_size(rem(tokens.fs_xs))
                            .text_color(tokens.fg_faint)
                            .child(SharedString::from(draft.at.clone())),
                    )
                    .child(div().typed(tokens.type_small).text_color(tokens.fg_muted).line_clamp(2).child(SharedString::from(draft.body.clone())))
            }));
        let agents = h_flex()
            .flex_wrap()
            .gap(rem(tokens.sp_6))
            .items_center()
            .px(rem(tokens.sp_12))
            .child(div().typed(tokens.type_small).text_color(tokens.fg_muted).child("Send to"))
            .when(page.recipients.is_empty(), |row| row.child(div().typed(tokens.type_small).text_color(tokens.fg_faint).child("no agent running here")))
            .children(page.recipients.iter().map(|recipient| {
                let pane = recipient.pane;
                chip(SharedString::from(format!("review-to-{pane}")), recipient.title.clone(), Some(pane) == to, &tokens)
                    .child(SharedString::from(recipient.title.clone()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.to = Some(pane);
                        cx.notify();
                    }))
            }));
        let words = if count == 1 { "Submit 1 comment".to_owned() } else { format!("Submit {count} comments") };
        let can_send = count > 0 && to.is_some() && !page.sending;
        v_flex()
            .id("review-submit")
            .test_support()
            .aria_label("Your review")
            .absolute()
            .top(rem(tokens.sp_8))
            .right(rem(tokens.sp_12))
            .w(rem(WIDE))
            .max_h(gpui_kit::relative(0.85))
            .rounded(rem(tokens.r_md))
            .border_1()
            .border_color(tokens.line_strong)
            .bg(tokens.bg_surface)
            .shadow_lg()
            .font_family(SharedString::from(tokens.font_ui))
            .occlude()
            .child(
                h_flex()
                    .gap(rem(tokens.sp_8))
                    .items_center()
                    .px(rem(tokens.sp_12))
                    .py(rem(tokens.sp_12))
                    .border_b_1()
                    .border_color(tokens.line)
                    .child(div().typed(tokens.type_title).text_color(tokens.fg).child("Your review"))
                    .child(div().typed(tokens.type_small).text_color(tokens.fg_faint).child(SharedString::from(format!("{count} pending"))))
                    .child(div().flex_1())
                    .child(
                        button("review-submit-close", "Close", Look::Plain, true, &tokens)
                            .child("Close")
                            .on_click(cx.listener(|this, _, _, cx| this.toggle(cx))),
                    ),
            )
            .child(list)
            .child(
                div()
                    .mx(rem(tokens.sp_12))
                    .my(rem(tokens.sp_8))
                    .rounded(rem(tokens.r_sm))
                    .border_1()
                    .border_color(tokens.line_strong)
                    .bg(tokens.bg_sunken)
                    .child(Textarea::new(&self.summary).w_full().appearance(false).bordered(false).aria_label("Summary")),
            )
            .child(agents)
            .child(
                h_flex()
                    .gap(rem(tokens.sp_8))
                    .items_center()
                    .p(rem(tokens.sp_12))
                    .child(
                        button("review-discard", "Discard all", Look::Plain, true, &tokens)
                            .child("Discard all")
                            .on_click(cx.listener(|this, _, _, cx| this.ask(DraftGesture::Discard, cx))),
                    )
                    .child(div().flex_1())
                    .child(if can_send {
                        button("review-send", words.clone(), Look::Primary, false, &tokens).child(SharedString::from(words)).on_click(cx.listener(move |this, _, _, cx| {
                            let summary = this.summary.read(cx).value().to_string();
                            this.ask(DraftGesture::Submit { summary, to }, cx);
                        }))
                    } else {
                        crate::kit::button::inert("review-send", words.clone(), Look::Primary, false, &tokens).child(SharedString::from(words))
                    }),
            )
            .into_any_element()
    }
}
