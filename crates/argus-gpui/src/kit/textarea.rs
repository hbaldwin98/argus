//! A multi-line text field that Rust also writes to, and that never loses what a person is typing:
//! the kit's `Field` for a box of several lines (a tracker map, a reconcile ask, a line's text).
//!
//! The same rule as `Field`: the text being typed wins over Rust's echo of it, so [`Area::show`]
//! writes only when the box has not got the keys and says something else, and the kit's `set_value`
//! sends no change, so what is shown is not an edit to send.

use gpui_kit::component::input::{Position, TextareaState};
use gpui_kit::{App, AppContext as _, Context, Entity, Focusable as _, SharedString, Window};

#[derive(Clone)]
pub struct Area {
    state: Entity<TextareaState>,
}

impl Area {
    /// A box `rows` lines tall with `placeholder` showing while it is empty.
    pub fn new<V: 'static>(placeholder: impl Into<SharedString>, rows: usize, window: &mut Window, cx: &mut Context<V>) -> Self {
        let placeholder = placeholder.into();
        Self { state: cx.new(|cx| TextareaState::new(window, cx).rows(rows).placeholder(placeholder)) }
    }

    /// The kit's state, for the element that draws it and the subscription that hears its edits.
    pub fn state(&self) -> &Entity<TextareaState> {
        &self.state
    }

    pub fn text(&self, cx: &App) -> SharedString {
        self.state.read(cx).value()
    }

    /// Whether the box has the keys, or something inside it does.
    pub fn typing(&self, window: &Window, cx: &App) -> bool {
        self.state.focus_handle(cx).contains_focused(window, cx)
    }

    /// Rust says the box reads `said`: written when it does not read that already and nobody is
    /// typing in it; whether it was.
    pub fn show(&self, said: &str, window: &mut Window, cx: &mut App) -> bool {
        if self.typing(window, cx) || self.text(cx) == said {
            return false;
        }
        self.put(said, window, cx);
        true
    }

    /// Puts the text in unconditionally: a box that is opening, or another line's words.
    pub fn put(&self, text: &str, window: &mut Window, cx: &mut App) {
        self.state.update(cx, |state, cx| state.set_value(text, window, cx));
    }

    /// The box takes the keys, the caret after its last character.
    pub fn focus_at_end(&self, window: &mut Window, cx: &mut App) {
        let text = self.text(cx);
        let line = text.matches('\n').count() as u32;
        let last = text.rsplit('\n').next().map_or(0, |last| last.encode_utf16().count()) as u32;
        self.state.update(cx, |state, cx| state.set_cursor_position(Position::new(line, last), window, cx));
    }

    /// The box takes the keys.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.state.update(cx, |state, cx| state.focus(window, cx));
    }
}
