//! Where the cursor goes besides where a key puts it: to the line Rust names, to the next or
//! previous change, and to the lines a selection covers for a comment (`ts:code/editor.ts`
//! `goTo`, `stepChange`, `comment`).

use argus_desktop_core::code::steps::step;
use argus_desktop_core::connection::page::Call;
use argus_desktop_core::code::stage::{CodeClick, CodeGesture};
use argus_desktop_core::keys::keymap::Action;
use gpui_kit::component::input::RopeExt as _;
use gpui_kit::{point, px, Context};

use super::session::Key;
use super::CodeEditor;

impl CodeEditor {
    /// The cursor at the start of `line` (from one, held to the document), with the line brought to
    /// the vertical middle once the editor knows its height.
    pub(super) fn go_to(&mut self, key: &Key, line: u32, cx: &mut Context<Self>) {
        let Some(session) = self.sessions.get_mut(key) else { return };
        let Some(state) = session.state.clone() else { return };
        let row = (line.max(1) as usize - 1).min(state.read(cx).text().lines_len().saturating_sub(1));
        session.centre = Some(row);
        state.update(cx, |state, cx| {
            let at = state.text().line_start_offset(row);
            state.set_selected_range(at..at, cx);
        });
        cx.notify();
    }

    /// Puts a waiting line in the middle, if the editor has been laid out.
    pub(super) fn settle(&mut self, key: &Key, cx: &mut Context<Self>) {
        let Some(session) = self.sessions.get_mut(key) else { return };
        let (Some(row), Some(state)) = (session.centre, session.state.clone()) else { return };
        let laid_out = state.update(cx, |state, cx| {
            let (Some(height), Some(bounds)) = (state.line_height(), state.text_bounds()) else { return false };
            let top = f32::from(height) * row as f32 - f32::from(bounds.size.height) / 2.0;
            state.set_scroll_offset(point(px(0.), px(-top.max(0.0))), cx);
            true
        });
        if laid_out {
            session.centre = None;
        }
    }

    /// `]c` and Alt-]: the first line of the next run of changed lines after the cursor's, or of the
    /// last before it. Whether there was one.
    pub(super) fn step(&mut self, forward: bool, cx: &mut Context<Self>) -> bool {
        let Some(key) = self.current.clone() else { return false };
        let Some(marks) = self.sessions.get(&key).and_then(|s| s.marks.as_ref()) else { return false };
        let Some(state) = self.sessions.get(&key).and_then(|s| s.state.clone()) else { return false };
        let text = state.read(cx).text().clone();
        let mut lines: Vec<usize> = marks.added.get_ranges(cx).into_iter().chain(marks.removed.get_ranges(cx)).map(|range| text.offset_to_point(range.start).row + 1).collect();
        lines.sort_unstable();
        lines.dedup();
        let here = text.offset_to_point(state.read(cx).cursor()).row + 1;
        let Some(line) = step(&lines, here, forward) else { return false };
        self.go_to(&key, line as u32, cx);
        true
    }

    /// What is selected: Vim's selection while it has one, else the editor's. Vim's `to` is past the
    /// last character it covers, so the end is the character before it (a line selection ends on its
    /// line's break).
    fn selection(&self, editors: std::ops::Range<usize>) -> std::ops::Range<usize> {
        let vim = self.shown.as_ref().and_then(|key| self.sessions.get(key)).and_then(|session| session.vim.as_ref()).filter(|_| self.vim_on);
        match vim.and_then(|vim| vim.selection()) {
            Some((from, to)) => from..to.saturating_sub(1).max(from),
            None => editors,
        }
    }

    /// Alt-c: the lines the selection covers become the stage's pick and the comment box opens on
    /// them, as `v` and `c` do in a review. A selection on one line is a cursor, not a pick.
    pub(super) fn comment(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(state) = self.current.as_ref().and_then(|key| self.sessions.get(key)).and_then(|s| s.state.clone()) else { return false };
        let (text, range) = (state.read(cx).text().clone(), self.selection(state.read(cx).selected_range()));
        let (from, to) = (text.offset_to_point(range.start).row as u32 + 1, text.offset_to_point(range.end).row as u32 + 1);
        self.caller.call(Call::Code { gesture: CodeGesture::Click { target: CodeClick::Range { from, to } } });
        self.caller.call(Call::Code { gesture: CodeGesture::Action { action: Action::Comment } });
        true
    }

    /// The file asked for, from the stage's go-to and search keys, which are the stage's.
    pub(super) fn stage_asks(&self, action: Action) {
        self.caller.call(Call::Code { gesture: CodeGesture::Action { action } });
    }
}

