//! The address field (SHELL-BROWSER-8): the browser's url unless the person is typing in it.
//!
//! Its keys are its own (`Place::Address`): Enter sends `navigate` with the typed text made an
//! address by core, Escape puts the browser's url back, and both only blur the field, as the page does, so the next
//! key is the window's own (never the page's text). An
//! update never overwrites an address being typed (`Field::show` writes only to a field that does
//! not have the keys), and the url is what the field is put back to, so it is kept as the last
//! one heard.

use std::cell::RefCell;
use std::rc::Rc;

use argus_desktop_core::browser::address::address;
use argus_desktop_core::connection::page::Call;
use argus_desktop_core::keys::keymap::{Action, Place};
use argus_protocol::BrowserInput;
use gpui_kit::component::input::Input;
use gpui_kit::{prelude::*, App, Context, Focusable as _, IntoElement, SharedString, Window};

use crate::connection::caller::Caller;
use crate::kit::field::Field;
use crate::keys::field::{Fields, Registered};
use crate::theme::scale::rem;
use crate::theme::Tokens;

pub struct Address {
    pane: u64,
    field: Field,
    /// The browser's url as last heard: what Escape puts back.
    url: Rc<RefCell<SharedString>>,
    _keys: Registered,
}

impl Address {
    pub fn new<V: 'static>(pane: u64, caller: Caller, window: &mut Window, cx: &mut Context<V>) -> Self {
        let field = Field::new("", window, cx);
        let url = Rc::new(RefCell::new(SharedString::default()));
        let (typed, said) = (field.clone(), url.clone());
        let keys = Fields::of(cx).register_place(window, field.input().read(cx).focus_handle(cx), Place::Address, move |action, window, cx| {
            match action {
                Action::Choose => {
                    if let Some(url) = address(&typed.text(cx)) {
                        caller.call(Call::BrowserInput { pane, input: BrowserInput::Navigate { url } });
                    }
                }
                Action::Leave => typed.put(&said.borrow(), window, cx),
                _ => return false,
            }
            window.blur(cx);
            true
        });
        Self { pane, field, url, _keys: keys }
    }

    /// Takes in the browser's url: shown unless the field has the keys.
    pub fn show(&self, url: &str, window: &mut Window, cx: &mut App) {
        *self.url.borrow_mut() = SharedString::from(url.to_owned());
        self.field.show(url, window, cx);
    }

    /// The field, `.field.burl-field` (its `.mono` loses to `.field`'s `font: inherit`, so it is the UI face): as tall as a button, filling what the bar leaves.
    pub fn view(&self, tokens: &Tokens) -> impl IntoElement {
        gpui_kit::div().flex_1().min_w_0().child(
            Input::new(self.field.input()).id(SharedString::from(format!("browser-address-{}", self.pane))).h(rem(tokens.control_h)).aria_label("Address"),
        )
    }
}
