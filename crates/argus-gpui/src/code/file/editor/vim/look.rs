//! What Vim draws over the text: the cursor as a block on the character it stands on, a visual
//! selection, and the caret the kit shows put where Vim has the cursor. The kit's caret is a bar
//! of its own width and cannot be a block, so the block is a fill over the character: nothing
//! shows on an empty line or at the end of the text, and the character keeps its colour.

use argus_desktop_core::code::vim::Mode;
use gpui_kit::component::input::{RangeDecoration, RangeDecorationStyle};
use gpui_kit::Context;

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
        let look = session.look.get_or_insert_with(|| {
            state.update(cx, |state, cx| Look {
                block: state.create_range_decorations_collection(Vec::new(), cx),
                selected: state.create_range_decorations_collection(Vec::new(), cx),
            })
        });
        let (block, selected) = (look.block.clone(), look.selected.clone());
        let Some(vim) = session.vim.as_ref().filter(|_| on) else {
            block.clear(cx);
            selected.clear(cx);
            return;
        };
        let cursor = vim.cursor();
        let text = state.read(cx).text().clone();
        // A mode that is not one of the cursor's has none drawn: insert has the caret itself.
        let stands = matches!(vim.mode(), Mode::Normal | Mode::Visual | Mode::VisualLine | Mode::VisualBlock);
        let on_char = text.slice(cursor..text.len()).chars().next().filter(|c| *c != '\n');
        block.set(
            on_char
                .filter(|_| stands)
                .map(|c| RangeDecoration::new(cursor..cursor + c.len_utf8()).with_style(RangeDecorationStyle::Fill).with_color(tokens.accent.opacity(BLOCK)))
                .into_iter()
                .collect(),
            cx,
        );
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
    }
}
