//! Vim in the editor: the keys it is sent, and what comes back done to the buffer. Core's state
//! machine holds the text, the cursor and its own undo; the kit's editor shows what it did, so
//! every key it takes is applied to the buffer as a change that is neither typed nor undoable by
//! the kit, and told to the sync as the person's (`ts:code/editor.ts` hears Vim first,
//! EDITOR-VIM-1).
//!
//! Vim is on unless Settings turns it off, and switched live. Its text is the buffer's: a change it
//! did not make (an input method's, a paste from the menu, the disk's) is told to it as one it
//! must take in, and a cursor the person moved with the pointer is where its next key starts.

mod effects;
mod look;
mod matches;
mod panel;

use argus_desktop_core::code::grammar::conventions;
use argus_desktop_core::code::vim::page::vim_key;
use argus_desktop_core::code::vim::said::{said, Said};
use argus_desktop_core::code::vim::{Step, Vim};
use gpui_kit::{Context, Window};

use super::session::{Key, TAB};
use super::CodeEditor;
use crate::keys::layer::{Press, Verdict};

impl CodeEditor {
    /// A key while Vim is on: `None` when Vim does not take it, so the editor's own chords and
    /// the kit's handling hear it.
    pub(super) fn vim_key(&mut self, press: &Press, window: &mut Window, cx: &mut Context<Self>) -> Option<Verdict> {
        let key = vim_key(press.page)?;
        let at = self.shown.clone()?;
        let session = self.sessions.get_mut(&at)?;
        let state = session.state.clone()?;
        let (read_only, rows) = (session.read_only, state.read(cx).visible_row_range());
        // Vim's text is the buffer's as it stands when it first hears a key.
        let mut vim = session.vim.take().unwrap_or_else(|| Vim::new(&state.read(cx).text().to_string()));
        // The pointer or an input method may have moved the caret since the last key.
        let caret = state.read(cx).cursor();
        if caret != vim.cursor() {
            vim.place(caret);
        }
        vim.set_read_only(read_only);
        // `gc` and `=` are the loaded grammar's language's, and nothing while there is none (EDITOR-VIM-24).
        let language = session.language.as_deref();
        vim.set_comment_token(language.and_then(conventions::comment_token));
        vim.set_indent_unit(language.filter(|name| conventions::braced(name)).map(|_| TAB));
        if let Some(rows) = rows {
            vim.set_view(rows.start, rows.end.saturating_sub(1));
        }
        // The register named by `+` reads the clipboard as it is when the name is typed.
        if key.typed() == Some('+') {
            self.shared.regs.offer_clipboard(cx.read_from_clipboard().and_then(|item| item.text()));
        }
        let step = vim.key(&mut self.shared, key);
        let handled = step.handled;
        if let Some(session) = self.sessions.get_mut(&at) {
            session.vim = Some(vim);
        }
        if !handled {
            return None;
        }
        self.vim_done(&at, step, window, cx);
        Some(Verdict::Taken)
    }

    /// What a key did: the change made to the buffer and told to the sync, what Vim asks of the
    /// window, and where the cursor and the selection are drawn now.
    fn vim_done(&mut self, key: &Key, step: Step, window: &mut Window, cx: &mut Context<Self>) {
        let now = self.began.elapsed().as_millis() as u64;
        let Some(session) = self.sessions.get_mut(key) else { return };
        let Some(state) = session.state.clone() else { return };
        if let Some(set) = step.change {
            let edits: Vec<_> = set.changes().into_iter().map(|c| (c.from..c.to, c.insert)).collect();
            state.update(cx, |state, cx| state.apply_external_edits(&edits, window, cx));
            session.snapshot = state.read(cx).text().clone();
            self.told(key, &set, now, cx);
        }
        for effect in step.effects {
            self.vim_effect(key, effect, cx);
        }
        if let Some(text) = self.shared.regs.take_clipboard_write() {
            cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(text));
        }
        self.vim_look(key, cx);
        cx.notify();
    }

    /// What the bar says of Vim for the file shown: the mode, the keys of a command begun, the line
    /// being typed. Nothing while Vim is off.
    pub(super) fn vim_said(&self) -> Option<Said> {
        if !self.vim_on {
            return None;
        }
        let vim = self.shown.as_ref().and_then(|key| self.sessions.get(key)).and_then(|session| session.vim.as_ref());
        Some(match vim {
            Some(vim) => said(vim, &self.shared),
            None => Said { mode: "NORMAL", ..Said::default() },
        })
    }

    /// Vim was turned on or off in Settings: every editor goes to the keys it now takes, and what
    /// Vim drew goes with it. Switched on, an editor opens in normal mode (EDITOR-VIM-1).
    pub(super) fn vim_switched(&mut self, cx: &mut Context<Self>) {
        let keys: Vec<Key> = self.sessions.keys().cloned().collect();
        for key in keys {
            if let Some(session) = self.sessions.get_mut(&key) {
                session.vim = None;
            }
            self.vim_start(&key, cx);
        }
        cx.notify();
    }

    /// Vim for a file's session, made from the buffer as it stands when Vim is on, and what it
    /// draws put up at once: the page's editor shows its block before a key is pressed.
    pub(super) fn vim_start(&mut self, key: &Key, cx: &mut Context<Self>) {
        if self.vim_on {
            if let Some(session) = self.sessions.get_mut(key).filter(|session| session.vim.is_none()) {
                if let Some(state) = session.state.clone() {
                    let (text, caret) = (state.read(cx).text().to_string(), state.read(cx).cursor());
                    let mut vim = Vim::new(&text);
                    vim.place(caret);
                    session.vim = Some(vim);
                }
            }
        }
        self.vim_look(key, cx);
    }
}
