//! The console as the window draws it: a tab per console the daemon holds, the strip's buttons,
//! and the console's sides, each a screen of its own (two at once while it is split) headed by its
//! console.
//!
//! Everything it draws is `Heard`'s last `ConsolePage`, and every gesture is the `Call` the page
//! sends: nothing on screen changes until the `Console` event that answers it, so a refused
//! select, close or spawn leaves the console as it is. Where the console is (docked, the Console
//! stage, fullscreen) and which dock holds it are `layout`'s; this is what is in it.
//!
//! While the daemon is gone the console is dimmed and answers no press, as the sidebar does
//! (`Heard::held`).

pub mod focus;
pub mod head;
pub mod menu;
pub mod rename;
pub mod sides;
pub mod strip;

use std::cell::Cell;

use argus_desktop_core::connection::page::{Call, Side};
use argus_desktop_core::panes::console::ConsoleGesture;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, App, Context, Entity, FocusHandle, ScrollHandle, Subscription, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::panes::terminal::Terminal;
use crate::theme::scale::rem;
use crate::theme::Tokens;

pub struct ConsoleView {
    heard: Entity<Heard>,
    caller: Caller,
    first: Entity<Terminal>,
    second: Entity<Terminal>,
    /// The Workspace's pane, which has the keys when no console does.
    stage: FocusHandle,
    /// The screen the keys were last moved to, which is not the console's
    /// pane id: an unsplit or a trade of sides moves the keys between screens
    /// with the same pane typed.
    typed: Option<FocusHandle>,
    /// The strip's scroll, and the tab it was last scrolled to, so only a newly chosen one moves it.
    tabs: ScrollHandle,
    scrolled_to: Cell<Option<u64>>,
    _observing: Subscription,
}

impl ConsoleView {
    pub fn new(heard: Entity<Heard>, caller: Caller, stage: FocusHandle, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let side = |side, window: &mut Window, cx: &mut Context<Self>| {
            let (heard, caller) = (heard.clone(), caller.clone());
            cx.new(|cx| Terminal::new(heard, caller, side, window, cx))
        };
        let first = side(Side::First, window, cx);
        let second = side(Side::Second, window, cx);
        let observing = cx.observe_in(&heard, window, |this, _, window, cx| {
            this.follow_keys(window, cx);
            cx.notify();
        });
        let mut view = Self { heard, caller, first, second, stage, typed: None, tabs: ScrollHandle::new(), scrolled_to: Cell::default(), _observing: observing };
        view.typed = view.keyed_handle(cx);
        view
    }

    /// A gesture on the console, sent unless the daemon is gone.
    pub(super) fn gesture(&self, gesture: ConsoleGesture, cx: &App) {
        if !self.heard.read(cx).held() {
            self.caller.call(Call::Console { gesture });
        }
    }
}

impl Render for ConsoleView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let (page, docks, held) = {
            let heard = self.heard.read(cx);
            (heard.console.clone(), heard.docks.clone(), heard.held())
        };
        let head = self.head(&page, &docks, &tokens, cx);
        let body = match (page.showing, page.beside) {
            (Some(first), second) => {
                let first = self.side("console-first", first, self.first.clone(), &page, &tokens, cx);
                let second = second.map(|pane| {
                    self.side("console-second", pane, self.second.clone(), &page, &tokens, cx)
                });
                h_flex()
                    .id("console-sides")
                    .flex_1()
                    .min_h_0()
                    .child(first)
                    .when_some(second, |sides, second| {
                        sides.child(div().w(rem(tokens.hairline)).h_full().flex_shrink_0().bg(tokens.line)).child(second)
                    })
                    .into_any_element()
            }
            (None, _) => self.empty(&page, &tokens, cx).into_any_element(),
        };
        v_flex()
            .id("console")
            .test_support()
            .aria_label("Console")
            .size_full()
            .min_h_0()
            .bg(tokens.see_sunken)
            .when(held, |console| console.opacity(tokens.opacity_held))
            .child(head)
            .child(body)
    }
}

#[cfg(test)]
mod tests;
