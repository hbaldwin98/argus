//! A cell's screen (SHELL-BROWSER-5, -10, -13): the picture of the page letterboxed in the box, or
//! the words for no tab, and the person's hand on it.
//!
//! The box has no padding: it is the viewport the page is laid out at (`viewport`), so the
//! picture fills it one pixel to one. A press on it takes the keys and, when the point is on the
//! picture, clicks there in the page's own pixels (core's `within` finds them); the wheel scrolls
//! the page and never the window. A key is in `keys`.

use argus_desktop_core::browser::within::{within, Box2};
use argus_desktop_core::browser::PageBrowser;
use argus_desktop_core::connection::page::Call;
use argus_protocol::BrowserInput;
use gpui_kit::component::v_flex;
use gpui_kit::{canvas, div, ElementInputHandler, prelude::*, Context, MouseButton, MouseDownEvent, Point, Pixels, ScrollWheelEvent, SharedString, TestSupportExt as _, Window};

use super::bar::text_button;
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
            .on_scroll_wheel(cx.listener(Self::wheeled))
            .child(self.reporter.measure(self.area.clone(), cx.entity_id()))
            .child(self.takes_text(cx));
        // The page lets any button press (the page tests none).
        for button in [MouseButton::Left, MouseButton::Middle, MouseButton::Right] {
            screen = screen.on_mouse_down(button, cx.listener(Self::pressed));
        }
        if browser.shown.is_some() {
            screen = screen.child(div().absolute().size_full().child(self.picture.clone()));
        } else {
            screen = screen.child(self.blank(tokens));
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

    /// The point of a pointer's place in the page's own pixels, or none in the bands either side of
    /// the picture (and while there is no picture to be on).
    fn point_in_page(&self, at: Point<Pixels>, cx: &gpui_kit::App) -> Option<(f64, f64)> {
        let (width, height) = {
            let heard = self.heard.read(cx);
            let picture = heard.pictures.of(self.pane)?;
            (picture.width, picture.height)
        };
        let area = self.area.get();
        let room = Box2 {
            left: f64::from(area.origin.x),
            top: f64::from(area.origin.y),
            width: f64::from(area.size.width),
            height: f64::from(area.size.height),
        };
        within(room, (width, height), f64::from(at.x), f64::from(at.y)).map(|at| (at.x, at.y))
    }

    fn pressed(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        if let Some((x, y)) = self.point_in_page(event.position, cx) {
            self.caller.call(Call::BrowserInput { pane: self.pane, input: BrowserInput::Click { x, y } });
        }
    }

    /// The wheel over the screen is the page's, whether or not it is over the picture: the window
    /// does not scroll.
    fn wheeled(&mut self, event: &ScrollWheelEvent, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        let Some((x, y)) = self.point_in_page(event.position, cx) else { return };
        // GPUI's deltas are positive toward the top and left, the page's `deltaX` and `deltaY`
        // positive toward the bottom and right.
        let delta = event.delta.pixel_delta(window.line_height());
        let (dx, dy) = (-f64::from(delta.x), -f64::from(delta.y));
        self.caller.call(Call::BrowserInput { pane: self.pane, input: BrowserInput::Wheel { x, y, dx, dy } });
    }
}
