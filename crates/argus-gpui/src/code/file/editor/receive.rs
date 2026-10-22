//! What the editor does with the daemon's words about a file: the first text mounts the view, a
//! change from the disk is shown in the buffer without being typed, and what core's `Live` asks
//! of the daemon is sent (`ts:code/editor.ts` `receive`, `fromTheDisk` and `mountView`).

use std::ops::Range;
use std::time::Duration;

use argus_desktop_core::code::edit::EditEvent;
use argus_desktop_core::code::live::Heard as Said;
use argus_desktop_core::code::pace::{REREAD_MS, RETRY_MS};
use argus_desktop_core::code::change::ChangeSet;
use argus_desktop_core::code::stage::CodeGesture;
use argus_desktop_core::connection::page::Call;
use gpui_kit::component::input::{EditorState, InputEvent, Rope, TabSize};
use gpui_kit::{AppContext as _, Context, Focusable as _, Window};

use super::session::Key;
use super::CodeEditor;
use crate::keys::field::Fields;
use crate::theme::Tokens;

impl CodeEditor {
    /// One word of the daemon's, handed to the file it is about (every file, for `Lost`).
    pub(super) fn hear(&mut self, event: &EditEvent, window: &mut Window, cx: &mut Context<Self>) {
        for (checkout, path, said) in self.lives.heard(event) {
            self.apply((checkout, path), said, window, cx);
        }
        self.sweep(cx);
    }

    /// Does what `said` asks of the window and the daemon.
    pub(super) fn apply(&mut self, key: Key, said: Said, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = said.mount {
            self.mount(&key, text, window, cx);
        }
        if let Some(set) = said.shown {
            self.show(&key, &set, window, cx);
        }
        for gesture in said.asks {
            self.caller.call(Call::Edit { gesture });
        }
        if let Some(read_only) = said.read_only {
            // A file refused before its text came is the one on screen at once, so the bar says
            // why instead of the file before staying up (`ts:code/editor.ts` `closed`).
            if read_only && self.current.as_ref() == Some(&key) && self.sessions.get(&key).is_some_and(|s| s.state.is_none()) {
                self.shown = Some(key.clone());
            }
            if let Some(session) = self.sessions.get_mut(&key) {
                session.read_only = read_only;
                if let Some(state) = session.state.clone() {
                    state.update(cx, |state, cx| state.set_readonly(read_only, cx));
                }
            }
        }
        if said.retry {
            self.retry_later(&key, cx);
        }
        if said.reread {
            self.read_again(&key, cx);
        }
        // A clash the sync holds, or no longer holds, is marked in the text.
        self.clash(&key, cx);
        cx.notify();
    }

    /// The first text of a file: the editor is made with it, set to be written as it is typed in,
    /// and takes the keys when it is the file on screen. A file held over for the one that was
    /// opening is let go of now that its successor can be drawn.
    fn mount(&mut self, key: &Key, text: Rope, window: &mut Window, cx: &mut Context<Self>) {
        if !self.sessions.contains_key(key) {
            return;
        }
        let accent = Tokens::global(cx).accent;
        // A grammar already loaded colours the first paint, so the file opens coloured.
        let language = self.loaded_for(&key.1);
        let Some(session) = self.sessions.get_mut(key) else { return };
        session.language = language.clone();
        let state = cx.new(|cx| {
            // No wrapping and the page's four-space indent; the guides, the auto-closed brackets
            // and the context menu are CodeMirror's, which the page's editor does not have.
            let mut state = EditorState::new(window, cx).default_value(text.to_string()).soft_wrap(false).indent_guides(false).tab_size(TabSize { tab_size: 4, hard_tabs: false });
            if let Some(language) = language.clone() {
                state = state.language(language);
            }
            // The page's `.cm-cursor` is an accent bar; its text fields keep the foreground.
            state.set_caret_color(Some(accent));
            state
        });
        let owner = key.clone();
        session.typing_heard = Some(cx.subscribe_in(&state, window, move |this, _, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::Change) {
                this.typed(&owner, cx);
            }
        }));
        let weak = cx.weak_entity();
        let handle = state.focus_handle(cx);
        session.keys = Some(Fields::of(cx).register(window, handle, move |press, window, cx| weak.update(cx, |this, cx| this.key(press, window, cx)).unwrap_or(crate::keys::layer::Verdict::Held)));
        session.snapshot = text;
        session.state = Some(state.clone());
        let at = session.at;
        self.mark(key, cx);
        self.go_to(key, at, cx);
        // Vim opens in normal mode with its block already drawn, before any key (EDITOR-VIM-1).
        self.vim_start(key, cx);
        if self.current.as_ref() == Some(key) {
            self.shown = Some(key.clone());
            state.update(cx, |state, cx| state.focus(window, cx));
        }
        // With no grammar loaded yet, one is looked for: installed, or offered.
        if language.is_none() {
            self.find_grammar(key, cx);
        }
    }

    /// The disk's change, shown in the buffer without being typed: the person's cursor stays on
    /// the text it was on, undo never reaches it, and what it wrote is lit (EDITOR-DISK-1 to -4).
    fn show(&mut self, key: &Key, set: &ChangeSet, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.sessions.get_mut(key) else { return };
        let Some(state) = session.state.clone() else { return };
        let edits: Vec<(Range<usize>, String)> = set.changes().into_iter().map(|c| (c.from..c.to, c.insert)).collect();
        state.update(cx, |state, cx| state.apply_external_edits(&edits, window, cx));
        // The editor said nothing of it, so what is typed next is found against the text as it is.
        session.snapshot = state.read(cx).text().clone();
        // Vim's text is the buffer's, so it takes the change in too and keeps its cursor on its text.
        if let Some(vim) = &mut session.vim {
            vim.past(set);
        }
        self.vim_look(key, cx);
        let ranges = set.new_ranges();
        self.light(key, ranges, cx);
    }

    fn retry_later(&mut self, key: &Key, cx: &mut Context<Self>) {
        let again = key.clone();
        let Some(session) = self.sessions.get_mut(key) else { return };
        session.retry = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(RETRY_MS)).await;
            this.update(cx, |this, cx| this.write(&again, cx)).ok();
        }));
    }

    /// The file is read again once writes stop, for its change marks and outline; only the one on
    /// screen is, and a burst of writes is read once.
    fn read_again(&mut self, key: &Key, cx: &mut Context<Self>) {
        let again = key.clone();
        let Some(session) = self.sessions.get_mut(key) else { return };
        session.reread = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(REREAD_MS)).await;
            this.update(cx, |this, _| {
                if this.current.as_ref() == Some(&again) {
                    this.caller.call(Call::Code { gesture: CodeGesture::Written });
                }
            })
            .ok();
        }));
    }
}
