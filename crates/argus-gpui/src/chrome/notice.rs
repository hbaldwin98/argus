//! The line the window says something along its bottom in: an info mark, the words and a way to
//! put it away (SHELL-NOTICE-1).
//!
//! What it says is the session's: a refusal's reason, a copy that failed, a key nobody binds.
//! `Heard` holds the newest, which replaces the one showing; this draws it and ends it after
//! [`NOTICE_FOR`], the page's six seconds, counted from when it arrived. The kit's notification
//! would stack them and wait on the pointer; a notice here is one line that is replaced.

use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, px, Context, Entity, Role, SharedString, Subscription, Task, Window};

use crate::connection::heard::{Heard, NOTICE_FOR};
use crate::kit::icon_button::icon_button;
use crate::kit::icons::Glyph;
use crate::style::Typed as _;
use crate::theme::Tokens;

/// The share of the stage a notice may take before its words wrap.
const WIDEST: f32 = 0.7;

pub struct NoticeView {
    heard: Entity<Heard>,
    /// Which notice the timer is for.
    timed: Option<u64>,
    timer: Option<Task<()>>,
    _observing: Subscription,
}

impl NoticeView {
    pub fn new(heard: Entity<Heard>, cx: &mut Context<Self>) -> Self {
        let observing = cx.observe(&heard, |this, heard, cx| this.start_timer(&heard, cx));
        Self { heard, timed: None, timer: None, _observing: observing }
    }

    /// Starts the timer of a notice that is new, and ends the timer of one that is gone. A
    /// replacement has a serial of its own, so its six seconds start over.
    fn start_timer(&mut self, heard: &Entity<Heard>, cx: &mut Context<Self>) {
        let serial = heard.read(cx).notice.as_ref().map(|notice| notice.serial);
        if serial == self.timed {
            return;
        }
        self.timed = serial;
        self.timer = serial.map(|serial| {
            let heard = heard.clone();
            cx.spawn(async move |_, cx| {
                cx.background_executor().timer(NOTICE_FOR).await;
                heard.update(cx, |heard, cx| {
                    if heard.dismiss_notice(serial) {
                        cx.notify();
                    }
                });
            })
        });
    }
}

impl Render for NoticeView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The first render, before any change was observed.
        if self.timed.is_none() {
            let heard = self.heard.clone();
            self.start_timer(&heard, cx);
        }
        let tokens = *Tokens::global(cx);
        let Some(notice) = self.heard.read(cx).notice.clone() else {
            return div().into_any_element();
        };
        let (heard, serial) = (self.heard.clone(), notice.serial);
        // Along the bottom of the stage, in the middle: `.notice`, `bottom: --sp-24`. The page
        // lifts it over its own command band; this window has none.
        div()
            .absolute()
            .bottom(px(tokens.sp_24))
            .left_0()
            .w_full()
            .flex()
            .justify_center()
            .child(
                h_flex()
                    .id("notice")
                    .role(Role::Status)
                    .test_support()
                    .aria_label(SharedString::from(notice.text.clone()))
                    .max_w(gpui_kit::relative(WIDEST))
                    .gap(px(tokens.sp_8))
                    .items_center()
                    .py(px(tokens.sp_4))
                    .pl(px(tokens.sp_12))
                    .pr(px(tokens.sp_4))
                    .bg(tokens.bg_raised)
                    .border_1()
                    .border_color(tokens.line_strong)
                    .rounded(px(tokens.r_md))
                    .shadow(tokens.shadow_pop())
                    .typed(tokens.type_body)
                    .text_color(tokens.fg)
                    .child(div().flex_shrink_0().text_color(tokens.attention).child(Glyph::Info.at(tokens.icon_sm)))
                    .child(div().min_w_0().py(px(tokens.sp_4)).child(SharedString::from(notice.text)))
                    .child(icon_button("notice-dismiss", "Dismiss", false, &tokens).child(Glyph::Close.at(tokens.icon_sm)).on_click(
                        move |_, _, cx| {
                            heard.update(cx, |heard, cx| {
                                if heard.dismiss_notice(serial) {
                                    cx.notify();
                                }
                            });
                        },
                    )),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests;
