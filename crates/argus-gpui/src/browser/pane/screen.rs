//! A cell's screen (SHELL-BROWSER-5, -10, -13): the picture of the page letterboxed in the box, or
//! the words for no tab, and the person's hand on it.
//!
//! The box has no padding: it is the viewport the page is laid out at (`viewport`), so the
//! picture fills it one pixel to one. A press on it takes the keys; what the pointer does to the
//! page, and the cursor drawn over it, is `pointer`'s. A key is in `keys`.

use argus_desktop_core::browser::PageBrowser;
use argus_desktop_core::connection::page::Call;
use argus_protocol::BrowserInput;
use gpui_kit::component::v_flex;
use gpui_kit::{canvas, div, ElementInputHandler, prelude::*, Context, MouseButton, SharedString, TestSupportExt as _, Window};

use super::bar::text_button;
use super::pointer::style;
use super::tabs::NEW_TAB;
use super::BrowserPane;
use crate::theme::scale::rem;
use crate::theme::Tokens;

pub const NO_TAB: &str = "No tab open";
pub const OPEN_TAB: &str = "Open a tab";

impl BrowserPane {
    /// `.bpane-screen`: the picture, or the blank for a browser with no tab.
    pub(super) fn screen(&self, browser: &PageBrowser, keyed: bool, window: &Window, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let pane = self.pane;
        let ring = keyed && self.focus.is_focused(window);
        let mut screen = div()
            .id(SharedString::from(format!("browser-screen-{pane}")))
            .test_support()
            .aria_label(SharedString::from(format!("{}'s browser", browser.agent)))
            .track_focus(&self.focus)
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .bg(tokens.bg_chrome)
            .cursor(style(browser.cursor))
            .on_scroll_wheel(cx.listener(Self::wheeled))
            .on_mouse_move(cx.listener(|this, event, _, cx| this.moved(event, cx)))
            .on_hover(cx.listener(|this, hovered: &bool, _, _| {
                if !hovered {
                    this.left();
                }
            }))
            .child(self.reporter.measure(self.area.clone(), cx.entity_id()))
            .child(self.takes_text(cx));
        // The page lets any button press (the page tests none).
        for button in [MouseButton::Left, MouseButton::Middle, MouseButton::Right] {
            screen = screen
                .on_mouse_down(button, cx.listener(Self::pressed))
                .on_mouse_up(button, cx.listener(|this, event, _, cx| this.released(event, cx)))
                .on_mouse_up_out(button, cx.listener(|this, event, _, cx| this.released(event, cx)));
        }
        if browser.shown.is_some() {
            screen = screen.child(div().absolute().size_full().child(self.picture.clone()));
        } else {
            screen = screen.child(self.blank(tokens));
        }
        if let Some(dialog) = &browser.dialog {
            screen = screen.child(self.dialog(dialog, tokens));
        }
        screen.when(ring, |screen| {
            screen.child(div().id(SharedString::from(format!("browser-ring-{pane}"))).test_support().absolute().size_full().border(rem(tokens.ring_outline)).border_color(tokens.ring))
        })
    }

    /// Hands the platform this view as the text input while the screen has the keys, so an input
    /// method (and the emoji panel) types into the page; without it only keys would arrive.
    fn takes_text(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (focus, pane) = (self.focus.clone(), cx.entity());
        canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| window.handle_input(&focus, ElementInputHandler::new(bounds, pane.clone()), cx),
        )
        .absolute()
        .size_full()
    }

    /// `.bpane-blank`: what a browser with no tab says in place of its page, and a way to open one.
    fn blank(&self, tokens: &Tokens) -> impl IntoElement {
        let (pane, caller) = (self.pane, self.caller.clone());
        v_flex()
            .id(SharedString::from(format!("browser-blank-{pane}")))
            .test_support()
            .absolute()
            .size_full()
            .items_center()
            .justify_center()
            .gap(rem(tokens.sp_8))
            .text_color(tokens.fg_muted)
            // The page's `<p>` keeps its 1em margin under it, which a flex column does not collapse.
            .child(div().mb(rem(tokens.fs_md)).child(NO_TAB))
            .child(text_button(format!("browser-blank-new-tab-{pane}"), NEW_TAB, OPEN_TAB, tokens, move |_, _| {
                caller.call(Call::BrowserInput { pane, input: BrowserInput::NewTab })
            }))
    }
}
