//! The text an input method commits into the pane: GPUI's input handler
//! for the terminal view, which holds no text of its own.
//!
//! The page sends the committed text once composition ends and shows no
//! preedit while composing (`panes/input.ts`, SHELL-INPUT-3), and this does
//! the same: a marked range is accepted and drawn nowhere, so what the
//! child sees is the commit. A plain key never gets here, since the view
//! takes it as a `Call::Key` and stops it; this is for IME commits, dead
//! keys and compose sequences, and a key the platform types as several
//! characters.

use std::ops::Range;

use gpui_kit::{
    point, size, Bounds, ClipboardItem, Context, EntityInputHandler, Pixels, UTF16Selection, Window,
};

use crate::panes::terminal::Terminal;

impl EntityInputHandler for Terminal {
    fn text_for_range(
        &mut self,
        _range: Range<usize>,
        _adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        None
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection { range: 0..0, reversed: false })
    }

    fn marked_text_range(&self, _window: &mut Window, _cx: &mut Context<Self>) -> Option<Range<usize>> {
        None
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}

    fn paste(&mut self, item: ClipboardItem, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(text) = item.text() {
            Terminal::paste(self, text);
        }
    }

    fn replace_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        self.text(text);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        _new_text: &str,
        _new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }

    /// The cursor's cell, so a candidate window opens beside what is typed.
    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let (cursor, (cell_w, cell_h)) = self.cursor_cell(cx)?;
        let origin = point(
            element_bounds.origin.x + cell_w * f32::from(cursor.col),
            element_bounds.origin.y + cell_h * f32::from(cursor.row),
        );
        Some(Bounds { origin, size: size(cell_w, cell_h) })
    }

    fn character_index_for_point(
        &mut self,
        _point: gpui_kit::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}
