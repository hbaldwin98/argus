//! A text field that Rust also writes to, and that never loses what a person is typing.
//!
//! A surface hears its page again whenever anything changes, and the page carries the field's text
//! as the session last heard it. Writing that back into a field that has the keys would move the
//! caret, drop an edit made since, and clear the undo history, so the text being typed wins over
//! Rust's echo of it (SHARED-4, CROSS-27). [`Field::show`] writes only when the field has not got
//! the keys and says something else. The kit's `set_value` sends no change, so what is shown is
//! not an edit to send.

use gpui_kit::component::input::InputState;
use gpui_kit::{App, AppContext as _, Context, Entity, Focusable as _, SharedString, Window};

#[derive(Clone)]
pub struct Field {
    input: Entity<InputState>,
}

impl Field {
    /// A field with `placeholder` showing while it is empty.
    pub fn new<V: 'static>(placeholder: impl Into<SharedString>, window: &mut Window, cx: &mut Context<V>) -> Self {
        let placeholder = placeholder.into();
        Self { input: cx.new(|cx| InputState::new(window, cx).placeholder(placeholder)) }
    }

    /// The kit's state, for the input that draws it and the subscription that hears its edits.
    pub fn input(&self) -> &Entity<InputState> {
        &self.input
    }

    /// What the field says now.
    pub fn text(&self, cx: &App) -> SharedString {
        self.input.read(cx).value()
    }

    /// Whether the field has the keys, or something inside it does.
    pub fn typing(&self, window: &Window, cx: &App) -> bool {
        self.input.read(cx).focus_handle(cx).contains_focused(window, cx)
    }

    /// Rust says the field reads `said`. It is written when the field does not read that already
    /// and nobody is typing in it; whether it was.
    pub fn show(&self, said: &str, window: &mut Window, cx: &mut App) -> bool {
        if self.typing(window, cx) || self.text(cx) == said {
            return false;
        }
        self.input.update(cx, |input, cx| input.set_value(said, window, cx));
        true
    }

    /// Puts the text in unconditionally: a field that is opening, or being emptied because what it
    /// was about has gone.
    pub fn put(&self, text: &str, window: &mut Window, cx: &mut App) {
        self.input.update(cx, |input, cx| input.set_value(text, window, cx));
    }

    /// The field takes the keys.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }
}

#[cfg(test)]
mod tests;
