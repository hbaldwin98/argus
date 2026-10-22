//! A browser pane's cell in the Workspace's grid (SHELL-BROWSER-5): the tab strip, the toolbar and
//! the screen, drawn where a terminal would be (the page's `browser/pane.ts`).
//!
//! The view is made once for a browser and kept while it lasts, so the address being typed, the
//! scroll of the tabs and the keys survive the redraws that follow every title or tab change; only
//! what Rust says of the browser (`Heard`'s page) is drawn again. It holds no state of the
//! browser's own: every press is sent and the daemon's page says what came of it.
//!
//! A press anywhere in the cell gives it the keys, as a press on a terminal does, with the call
//! core's `key_pane` names (SHELL-BROWSER-15). The questions are apart: the strip is `tabs`, the
//! toolbar `bar`, the address `address`, who has the browser `chip`, the screen `screen`, its
//! keys `keys` and the room it reports `viewport`.

pub mod address;
pub mod bar;
pub mod chip;
pub mod keys;
pub mod screen;
pub mod tabs;
pub mod viewport;

use std::cell::Cell;
use std::rc::Rc;

use argus_desktop_core::browser::keys::{key_pane, keyed_browser};
use gpui_kit::component::v_flex;
use gpui_kit::{div, prelude::*, Bounds, Context, Entity, FocusHandle, Pixels, Role, ScrollHandle, SharedString, Subscription, TestSupportExt as _, Window};

use super::picture::PictureView;
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::keys::field::Registered;
use crate::theme::Tokens;
use address::Address;
use viewport::Reporter;

pub struct BrowserPane {
    heard: Entity<Heard>,
    caller: Caller,
    pane: u64,
    /// The picture, shared with every other place that draws this browser's.
    picture: Entity<PictureView>,
    address: Address,
    /// The screen's focus: where the page's keys go.
    focus: FocusHandle,
    /// The tabs' scroll, kept so a redraw does not reset it.
    tabs: ScrollHandle,
    /// The area the screen was laid out in, in the window's pixels.
    area: Rc<Cell<Bounds<Pixels>>>,
    reporter: Reporter,
    _observing: Subscription,
    _keys: Registered,
}

impl BrowserPane {
    pub fn new(heard: Entity<Heard>, caller: Caller, pane: u64, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        // Square: the picture is letterboxed in the box, so its corners are not the card's.
        let picture = cx.new(|cx| PictureView::new(heard.clone(), pane, 0.0, cx));
        let reporter = Reporter::new(pane, caller.clone());
        let arming = reporter.clone();
        Self {
            address: Address::new(pane, caller.clone(), window, cx),
            _observing: cx.observe(&heard, move |_, _, cx| {
                arming.arm(cx);
                cx.notify();
            }),
            _keys: keys::register(pane, caller.clone(), focus.clone(), window, cx),
            reporter,
            heard,
            caller,
            pane,
            picture,
            focus,
            tabs: ScrollHandle::new(),
            area: Rc::default(),
        }
    }

    /// The page's focus, which the grid follows the session's keys to.
    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus
    }

    /// A press anywhere in the cell, before any child: the cell takes the keys unless it has them.
    fn claim(&mut self, cx: &mut Context<Self>) {
        let heard = self.heard.read(cx);
        if heard.held() {
            return;
        }
        if let Some(call) = key_pane(heard.rail.shown.as_ref(), &heard.rail.row, self.pane) {
            self.caller.call(call);
        }
    }
}

impl Render for BrowserPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let (browser, keyed) = {
            let heard = self.heard.read(cx);
            let browser = heard.browsers.browsers.iter().find(|browser| browser.pane == self.pane).cloned();
            (browser, keyed_browser(heard.rail.shown.as_ref(), &heard.rail.row) == Some(self.pane))
        };
        let id = SharedString::from(format!("browser-{}", self.pane));
        let Some(browser) = browser else {
            // The tree names the pane before the page or after it is gone: the card stays empty.
            return div().id(id).test_support().size_full().into_any_element();
        };
        self.address.show(&browser.url, window, cx);
        v_flex()
            .id(id)
            .test_support()
            .role(Role::Group)
            .size_full()
            .min_h_0()
            .capture_any_mouse_down(cx.listener(|this, _, _, cx| this.claim(cx)))
            .child(tabs::strip(&browser, &self.tabs, &self.caller, &tokens))
            .child(bar::bar(self.pane, &self.address, &self.caller, &tokens))
            .child(self.screen(&browser, keyed, window, &tokens, cx))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests;
