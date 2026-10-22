//! What one draw of a view's blocks holds: the theme, the clock, where presses go, what the window
//! remembers (`views/memory`) and keeps (`kept`), and the app to make entities in.
//!
//! Every block's drawing takes the pen, so a block that needs a Markdown with a selection, a scroll
//! that outlives the draw or the width its box was measured at asks for it here, by a key naming its
//! place, and none builds one of its own.

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::views::saved::{PageSaved, SavedViewsPage};
use gpui_kit::component::date_picker::DatePickerState;
use gpui_kit::component::input::{InputState, TextareaState};
use gpui_kit::component::select::SelectState;
use gpui_kit::{prelude::*, AnyElement, App, Entity, EntityId, ScrollHandle, Window};

use super::kept::Kept;
use crate::kit::diff::DiffView;
use crate::views::ctx::{Draw, ViewCtx};
use crate::views::memory::{LogPlace, Shared};

pub struct Pen<'a> {
    /// What a value is drawn against; its `view` is set per block by [`Pen::at`].
    pub draw: Draw<'a>,
    pub memory: &'a Shared,
    /// The saved views with their bodies, which an embed draws.
    pub saved: Option<&'a SavedViewsPage>,
    /// The view to tell when the person changes what a block shows (a tab, a fold) or the width
    /// changes: it is drawn again, and nothing else is.
    pub repaint: EntityId,
    pub kept: &'a mut Kept,
    pub window: &'a mut Window,
    pub cx: &'a mut App,
}

impl<'a> Pen<'a> {
    /// What a value in the block of `ctx` is drawn against.
    pub fn at<'c>(&self, ctx: &'c ViewCtx) -> Draw<'c>
    where
        'a: 'c,
    {
        Draw { view: Some(ctx), ..self.draw }
    }

    /// The saved view held under `id`.
    pub fn held(&self, id: &str) -> Option<&'a PageSaved> {
        self.saved.and_then(|page| page.views.iter().find(|view| view.id == id))
    }

    /// The width, in the page's pixels, the box under `key` was last laid out at; zero before it
    /// was, which the grid rules read as one column.
    pub fn width(&self, key: &str) -> f32 {
        self.kept.widths.get(key)
    }

    /// Fills the box it is put in, measuring its width under `key` and drawing the view again when it
    /// changes. It takes no room.
    pub fn watch(&self, key: String) -> AnyElement {
        self.kept.widths.watch(key, self.repaint).into_any_element()
    }

    /// Hands what the press of a button in a block asks for to Rust.
    pub fn call(&self, call: Call) {
        self.draw.caller.call(call);
    }

    /// A Markdown with its text selectable, kept under `key`; `muted` for what is said under a title.
    pub fn markdown(&mut self, key: &str, text: &str, muted: bool) -> AnyElement {
        gpui_kit::div().child(self.kept.markdown(key, text, muted, self.draw.caller, self.cx)).into_any_element()
    }

    /// A Copy button, kept under `key`, that copies `text`.
    pub fn copy(&mut self, key: &str, text: &str) -> AnyElement {
        self.copy_as(key, "Copy", text)
    }

    /// A copy button reading `label` (`Copy as TSV`), kept under `key`.
    pub fn copy_as(&mut self, key: &str, label: &str, text: &str) -> AnyElement {
        self.kept.copy(key, label, text, self.draw.caller, self.cx).into_any_element()
    }

    /// The scroll of the box under `key` (a board, a matrix, a code body), which the wheel's chaining
    /// reads where it was.
    pub fn scroll(&mut self, key: &str) -> ScrollHandle {
        self.kept.scroll(key).0
    }

    /// A text field kept under `key` (a table's filter), with `text` in it when it is made and the
    /// view drawn again, so what it governs follows, each time it is edited. `typed` is told the
    /// text first.
    pub fn field(&mut self, key: &str, placeholder: &str, text: &str, typed: impl Fn(&str, &mut App) + 'static) -> Entity<InputState> {
        self.kept.field(key, placeholder, text, typed, self.window, self.cx)
    }

    /// A several-line field kept under `key`, told and drawn as [`Pen::field`] is.
    pub fn area(&mut self, key: &str, placeholder: &str, text: &str, typed: impl Fn(&str, &mut App) + 'static) -> Entity<TextareaState> {
        self.kept.area(key, placeholder, text, typed, self.window, self.cx)
    }

    /// A select among `options` kept under `key`, showing `shown`; `typed` is told the option picked.
    pub fn choice(&mut self, key: &str, options: &[String], shown: &str, typed: impl Fn(&str, &mut App) + 'static) -> Entity<SelectState<Vec<String>>> {
        self.kept.pickers.choice(key, options, shown, typed, self.window, self.cx)
    }

    /// A date picker kept under `key` showing `text` (`YYYY-MM-DD`); `typed` is told the date as that.
    pub fn date(&mut self, key: &str, text: &str, typed: impl Fn(&str, &mut App) + 'static) -> Entity<DatePickerState> {
        self.kept.pickers.date(key, text, typed, self.window, self.cx)
    }

    /// A number box kept under `key` whose arrows step within `bounds`, told as [`Pen::field`] is.
    pub fn number(&mut self, key: &str, text: &str, bounds: (Option<f64>, Option<f64>), typed: impl Fn(&str, &mut App) + 'static) -> Entity<InputState> {
        self.kept.pickers.number(key, text, bounds, typed, self.window, self.cx)
    }

    /// A diff view kept under `key` that takes no clicks, for a diff an agent wrote.
    pub fn diff(&mut self, key: &str) -> Entity<DiffView> {
        self.kept.diff(key, self.cx)
    }

    /// The scroll of the log at `path`, put where the person left it: at the tail when they were at
    /// it, else at the line they stopped on. What it says now is remembered, so a log whose handle
    /// was let go of (its tab switched away from) comes back where it was.
    pub fn log_scroll(&mut self, path: &str) -> ScrollHandle {
        let (handle, fresh) = self.kept.scroll(path);
        if fresh {
            match self.memory.borrow().log(path) {
                Some(LogPlace { top, tail: false }) => handle.set_offset(gpui_kit::point(gpui_kit::px(0.0), gpui_kit::px(-top))),
                _ => handle.scroll_to_bottom(),
            }
        }
        handle
    }
}
