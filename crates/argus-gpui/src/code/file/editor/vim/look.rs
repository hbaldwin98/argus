//! What Vim draws over the text: the cursor as a block on the character it stands on with that
//! character drawn in the editor's own colour, or an outline of it while the editor has not the
//! keys (`.cm-fat-cursor`, EDITOR-COLOUR-8), a visual selection, and the caret the kit shows put
//! where Vim has the cursor.
//!
//! The kit's caret is a bar of its own width and cannot be a block, so the block is a fill over
//! the character. A line break is a cell the width of a space, so on an empty line the block is
//! that cell; the one place nothing can be filled is the empty last line after a final break.

use argus_desktop_core::code::vim::Mode;
use gpui_kit::component::input::{RangeDecoration, RangeDecorationStyle, TextDecoration};
use gpui_kit::{Context, HighlightStyle};

use super::super::session::{Key, Look};
use super::super::CodeEditor;
use crate::theme::Tokens;

/// How much of the accent the block is filled with (`--accent` at 70%).
const BLOCK: f32 = 0.70;

impl CodeEditor {
    /// The caret where Vim's cursor is, and the block and the selection drawn from Vim's state, or
    /// taken away while Vim is off.
    pub(in crate::code::file::editor) fn vim_look(&mut self, key: &Key, cx: &mut Context<Self>) {
        let tokens = *Tokens::global(cx);
        let on = self.vim_on;
        let Some(session) = self.sessions.get_mut(key) else { return };
        let Some(state) = session.state.clone() else { return };
        let focused = session.focused;
        let look = session.look.get_or_insert_with(|| {
            state.update(cx, |state, cx| Look {
                block: state.create_range_decorations_collection(Vec::new(), cx),
                outline: state.create_range_decorations_collection(Vec::new(), cx),
                inverted: state.create_decorations_collection(Vec::new(), cx),
                selected: state.create_range_decorations_collection(Vec::new(), cx),
                matches: state.create_range_decorations_collection(Vec::new(), cx),
            })
        });
        let (block, outline, inverted, selected) = (look.block.clone(), look.outline.clone(), look.inverted.clone(), look.selected.clone());
        let Some(vim) = session.vim.as_ref().filter(|_| on) else {
            block.clear(cx);
            outline.clear(cx);
            inverted.clear(cx);
            selected.clear(cx);
            self.vim_matches(key, cx);
            return;
        };
        let cursor = vim.cursor();
        let text = state.read(cx).text().clone();
        // A mode that is not one of the cursor's has none drawn: insert has the caret itself.
        let stands = matches!(vim.mode(), Mode::Normal | Mode::Visual | Mode::VisualLine | Mode::VisualBlock);
        let under = text.slice(cursor..text.len()).chars().next();
        let cell = under.filter(|_| stands).map(|c| cursor..cursor + c.len_utf8());
        let drawn = |style, colour| cell.clone().map(|range| RangeDecoration::new(range).with_style(style).with_color(colour)).into_iter().collect::<Vec<_>>();
        block.set(if focused { drawn(RangeDecorationStyle::Fill, tokens.accent.opacity(BLOCK)) } else { Vec::new() }, cx);
        outline.set(if focused { Vec::new() } else { drawn(RangeDecorationStyle::Frame, tokens.accent) }, cx);
        // The page's block takes the editor's background for its text, so the glyph shows on the fill.
        let glyph = cell.filter(|_| focused && under != Some('\n')).map(|range| TextDecoration::new(range, HighlightStyle { color: Some(tokens.bg_sunken), ..HighlightStyle::default() }));
        inverted.set(glyph.into_iter().collect(), cx);
        selected.set(
            vim.selections()
                .into_iter()
                .filter(|(from, to)| from < to)
                .map(|(from, to)| RangeDecoration::new(from..to).with_style(RangeDecorationStyle::Fill).with_color(tokens.selection))
                .collect(),
            cx,
        );
        if state.read(cx).cursor() != cursor {
            state.update(cx, |state, cx| state.set_selected_range(cursor..cursor, cx));
        }
        self.vim_matches(key, cx);
    }

    /// The editor gained or lost the keys: Vim's block is filled or an outline.
    pub(in crate::code::file::editor) fn focus_moved(&mut self, key: &Key, focused: bool, cx: &mut Context<Self>) {
        let Some(session) = self.sessions.get_mut(key) else { return };
        if session.focused != focused {
            session.focused = focused;
            self.vim_look(key, cx);
            cx.notify();
        }
    }
}
