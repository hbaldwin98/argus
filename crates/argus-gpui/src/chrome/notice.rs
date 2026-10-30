//! The pill the window says something in, left of the Dictation chip in the title bar: an icon
//! for the tone, the words and, where there is something to put away, a way to put it away.
//!
//! What it says is the session's: a refusal's reason, a copy that failed, a commit made. `Heard`
//! holds the newest, which replaces the one showing; this draws it and ends it after
//! [`NOTICE_FOR`], the page's six seconds, counted from when it arrived, fading over the last of
//! them. The kit's notification would stack them and wait on the pointer; a notice here is one
//! pill that is replaced.
//!
//! The tone is the source's (`NoticeTone`), so the colour and icon say what the words mean
//! without this reading them. A done notice needs nothing from the person and has no close; the
//! others do. A paste has no undo to press (the daemon keeps the file and pastes its path in one
//! go), so its close only puts the pill away.

use argus_desktop_core::connection::page::NoticeTone;
use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, Animation, AnimationExt as _, Context, ElementId, Entity, Hsla, Role, SharedString, Subscription, Task, Window};

use crate::connection::heard::{Heard, NOTICE_FOR};
use crate::kit::icons::Glyph;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The widest a notice grows before its words are cut with an ellipsis (the whole text is its
/// tooltip).
const WIDEST: f32 = 360.0;

/// How much of the six seconds is the fade at its end.
const FADE: f32 = 0.08;

/// The pill's opacity `delta` of the way through its time: whole, then gone over the last [`FADE`].
fn opacity(delta: f32) -> f32 {
    if delta <= 1.0 - FADE { 1.0 } else { ((1.0 - delta) / FADE).clamp(0.0, 1.0) }
}

/// The colour, mark and whether it has a close, by tone.
fn look(tone: NoticeTone, tokens: &Tokens) -> (Hsla, Glyph, bool) {
    match tone {
        NoticeTone::Done => (tokens.ok, Glyph::Tick, false),
        NoticeTone::Arrived => (tokens.accent, Glyph::Picture, true),
        NoticeTone::Refused => (tokens.attention, Glyph::Info, true),
        NoticeTone::Failed => (tokens.danger, Glyph::Warn, true),
    }
}

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
        let (colour, glyph, closes) = look(notice.tone, &tokens);
        let words = SharedString::from(notice.text);
        // The pill is the Dictation chip's height and shape, tinted as a status pill is.
        h_flex()
            .id("notice")
            .role(if notice.tone == NoticeTone::Failed { Role::Alert } else { Role::Status })
            .test_support()
            .aria_label(words.clone())
            .tooltip(tip(words.clone()))
            .flex_none()
            .max_w(rem(WIDEST))
            .h(rem(tokens.dictate_chip_h))
            .pl(rem(tokens.sp_8))
            .pr(rem(if closes { tokens.sp_4 } else { tokens.sp_8 }))
            .gap(rem(tokens.sp_6))
            .items_center()
            .rounded(rem(tokens.r_sm))
            .border_1()
            .border_color(colour.opacity(0.40))
            .bg(colour.opacity(0.12))
            .typed(tokens.type_small)
            .text_size(rem(tokens.fs_xs))
            .text_color(tokens.fg)
            .child(div().flex_none().text_color(colour).child(glyph.at(tokens.icon_sm)))
            .child(div().min_w_0().overflow_hidden().text_ellipsis().whitespace_nowrap().child(words))
            .when(closes, |pill| {
                pill.child(
                    div()
                        .id("notice-dismiss")
                        .role(Role::Button)
                        .test_support()
                        .aria_label(SharedString::from("Dismiss"))
                        .tooltip(tip("Dismiss"))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(rem(tokens.sp_16))
                        .rounded(rem(tokens.r_xs))
                        .text_color(tokens.fg_muted)
                        .cursor_pointer()
                        .hover(move |style| style.text_color(tokens.fg).bg(tokens.hover_fill))
                        .child(Glyph::Close.at(tokens.icon_xs))
                        .on_click(move |_, _, cx| {
                            heard.update(cx, |heard, cx| {
                                if heard.dismiss_notice(serial) {
                                    cx.notify();
                                }
                            });
                        }),
                )
            })
            .with_animation(ElementId::from(("notice-fade", serial)), Animation::new(NOTICE_FOR), |pill, delta| pill.opacity(opacity(delta)))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests;
