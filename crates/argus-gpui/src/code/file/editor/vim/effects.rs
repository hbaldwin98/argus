//! What Vim asks of the window beyond a change to the text: write now, step to a change mark,
//! scroll the cursor's line to a place in the view, and put a selection on the clipboard. A message
//! or a prompt is drawn by the bar from Vim's own state, so it is not an effect here.

use argus_desktop_core::code::vim::screen::Place;
use argus_desktop_core::code::vim::Effect;
use gpui_kit::{point, px, ClipboardItem, Context};

use super::super::session::Key;
use super::super::CodeEditor;

impl CodeEditor {
    pub(super) fn vim_effect(&mut self, key: &Key, effect: Effect, cx: &mut Context<Self>) {
        match effect {
            Effect::Write => self.write(key, cx),
            Effect::NextChange | Effect::PreviousChange => {
                // The step moves the editor's caret, and Vim's cursor goes with it.
                if self.step(effect == Effect::NextChange, cx) {
                    self.vim_follows(key, cx);
                }
            }
            Effect::Message(_) => {}
            Effect::Reveal { line, at } => self.reveal(key, line, at, cx),
            Effect::Copy(text) => cx.write_to_clipboard(ClipboardItem::new_string(text)),
        }
    }

    /// Vim's cursor goes to where the editor's caret is: the editor moved it for a command.
    fn vim_follows(&mut self, key: &Key, cx: &mut Context<Self>) {
        let Some(session) = self.sessions.get_mut(key) else { return };
        let Some(state) = session.state.clone() else { return };
        let caret = state.read(cx).cursor();
        if let Some(vim) = &mut session.vim {
            vim.place(caret);
        }
    }

    /// Scrolls so `line` (from zero) is at `at` in the view: `zt`, `zz`, `zb`, `Ctrl-e`, `Ctrl-d`.
    fn reveal(&mut self, key: &Key, line: usize, at: Place, cx: &mut Context<Self>) {
        let Some(state) = self.sessions.get(key).and_then(|session| session.state.clone()) else { return };
        state.update(cx, |state, cx| {
            let (Some(height), Some(bounds)) = (state.line_height(), state.text_bounds()) else { return };
            let (line_height, view) = (f32::from(height), f32::from(bounds.size.height));
            let top = line as f32 * line_height;
            let top = match at {
                Place::Top => top,
                Place::Center => top - (view - line_height) / 2.0,
                Place::Bottom => top - (view - line_height),
            };
            state.set_scroll_offset(point(px(0.), px(-top.max(0.0))), cx);
        });
    }
}
