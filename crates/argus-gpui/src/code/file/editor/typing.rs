//! What the person types, and when it is written: a change of the buffer is found against the text
//! last seen and handed to core's `Live`, which keeps what is unwritten, and a write goes out once
//! typing pauses, or at the first key a second in (`code::pace`). A file that is no longer on
//! screen is written and then let go (`ts:code/editor.ts` `typed`, `write`, `retire`, `done`).

use std::time::Duration;

use argus_desktop_core::code::change::ChangeSet;
use argus_desktop_core::code::pace::Then;
use argus_desktop_core::code::sync::Keep;
use argus_desktop_core::code::typed;
use argus_desktop_core::connection::page::Call;
use gpui_kit::{Context, Window};

use super::session::Key;
use super::CodeEditor;

impl CodeEditor {
    /// The editor's text changed by the person: the change is found, told to the sync, and a
    /// write is arranged.
    pub(super) fn typed(&mut self, key: &Key, cx: &mut Context<Self>) {
        let now = self.began.elapsed().as_millis() as u64;
        let Some(session) = self.sessions.get_mut(key) else { return };
        let Some(state) = session.state.clone() else { return };
        let text = state.read(cx).text().clone();
        let Some(set) = typed::set(&session.snapshot, &text) else { return };
        session.snapshot = text;
        // Vim's text is the buffer's, so a change it did not make (an input method's, a paste
        // from the menu) is one it must take in.
        if let Some(vim) = &mut session.vim {
            vim.past(&set);
        }
        self.told(key, &set, now, cx);
    }

    /// `set` was made on the buffer by the person (typed, or a Vim command): it is the sync's to
    /// keep until written, and a write is arranged.
    pub(super) fn told(&mut self, key: &Key, set: &ChangeSet, now: u64, cx: &mut Context<Self>) {
        if let Some(live) = self.lives.get_mut(key.0, &key.1) {
            live.typed(set);
        }
        self.clash(key, cx);
        let Some(session) = self.sessions.get_mut(key) else { return };
        match session.typing.typed(now) {
            Then::Write => self.write(key, cx),
            Then::After(ms) => {
                let again = key.clone();
                session.quiet = Some(cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(Duration::from_millis(ms)).await;
                    this.update(cx, |this, cx| this.write(&again, cx)).ok();
                }));
            }
        }
        cx.notify();
    }

    /// Writes what is typed now, unless a write is in flight, a clash is held, or the file is
    /// closed or not reachable: it is written with the next answer.
    pub(super) fn write(&mut self, key: &Key, cx: &mut Context<Self>) {
        if let Some(session) = self.sessions.get_mut(key) {
            session.quiet = None;
            session.typing.written();
        }
        let ask = self.lives.get_mut(key.0, &key.1).and_then(|live| live.write());
        if let Some(gesture) = ask {
            self.caller.call(Call::Edit { gesture });
        }
        cx.notify();
    }

    /// The file is no longer the one shown: what is typed is written, and it is closed once
    /// nothing is unwritten.
    pub(super) fn retire(&mut self, key: &Key) {
        // A file that never got its text was never on screen, so there is nothing to write or
        // close: it stays as it is, and is shown again if the stage comes back to it.
        if !self.sessions.get(key).is_some_and(|session| session.state.is_some()) {
            return;
        }
        let ask = self.lives.get_mut(key.0, &key.1).and_then(|live| live.retire());
        if let Some(gesture) = ask {
            self.caller.call(Call::Edit { gesture });
        }
    }

    /// Lets go of every file that is retired, written and not on screen: its close is sent and its
    /// view dropped.
    pub(super) fn sweep(&mut self, cx: &mut Context<Self>) {
        let done: Vec<Key> = self
            .sessions
            .keys()
            .filter(|key| Some(*key) != self.shown.as_ref() && self.current.as_ref() != Some(*key))
            .filter(|key| self.lives.get(key.0, &key.1).is_some_and(|live| live.done()))
            .cloned()
            .collect();
        for key in done {
            if let Some(live) = self.lives.remove(key.0, &key.1) {
                self.caller.call(Call::Edit { gesture: live.close() });
            }
            self.sessions.remove(&key);
        }
        cx.notify();
    }

    /// The person's word on a held clash: the buffer takes the disk's change or keeps its own, the
    /// result is written at once, and the keys return to the editor.
    pub(super) fn resolve(&mut self, keep: Keep, window: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = self.current.clone() else { return };
        let Some(live) = self.lives.get_mut(key.0, &key.1) else { return };
        let said = live.resolve(keep);
        self.apply(key.clone(), said, window, cx);
        if let Some(state) = self.sessions.get(&key).and_then(|s| s.state.clone()) {
            state.update(cx, |state, cx| state.focus(window, cx));
        }
    }

}
