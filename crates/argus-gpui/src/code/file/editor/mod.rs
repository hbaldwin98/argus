//! The editor that stands in for the file's lines when the stage edits it: a session for each file
//! opened, the kit's editor for the one on screen, and the bar over it (`ts:code/editor.ts`, and
//! `gpui/parity/editor.md`'s OPEN, SWITCH, KEYS, GUTTER and STATE).
//!
//! The buffer is this view's, as it is the page's: a key must not wait on the session or the
//! daemon, so the text, the sync and the status live here and in core's `Live` (`code::live`), and
//! Rust is told only of what the stage must know (a write was made, a selection to comment on).
//! The daemon's words reach it past the session as `Edited`; the connection coming back opens
//! every file the connection took (EDITOR-RECONN-11). Choosing another file keeps the previous
//! file's editor on screen until the new one's text arrives, so no frame is blank or a skeleton
//! (EDITOR-SWITCH-2).
//!
//! The kit's editor is patched (`patches/gpui-base`): an agent's change is applied to it without
//! moving the cursor or entering undo, and the gutter can carry marks.

mod bar;
mod clash;
mod grammar;
mod keys;
mod marks;
mod moves;
mod receive;
mod session;
mod typing;
mod vim;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use argus_desktop_core::code::grammar::load::Loader;
use argus_desktop_core::code::grammar::net::Http;
#[cfg(not(test))]
use argus_desktop_core::code::grammar::net::Net;
#[cfg(not(test))]
use argus_desktop_core::code::grammar::store::Store;
use argus_desktop_core::code::live::{Lives, Status};
use argus_desktop_core::code::stage::CodePage;
use argus_desktop_core::code::steps::marks;
use argus_desktop_core::code::vim::Shared;
use argus_desktop_core::connection::page::{Call, Connection};
use gpui_kit::component::input::{Editor, EditorState};
use gpui_kit::component::v_flex;
use gpui_kit::{div, prelude::*, Context, Entity, Focusable as _, Subscription, TestSupportExt as _, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::{Edited, Heard};
use crate::theme::Tokens;
use session::{Key, Session};

pub struct CodeEditor {
    caller: Caller,
    lives: Lives,
    sessions: HashMap<Key, Session>,
    /// The file the stage names.
    current: Option<Key>,
    /// The file whose editor is drawn: the current one once it has its text, until then the one
    /// before it.
    shown: Option<Key>,
    /// Whether the daemon was reachable at the last status, to see it come back.
    connected: bool,
    /// What every file's Vim shares, so a yank in one is pasted in another (EDITOR-VIM-17).
    shared: Shared,
    /// Settings: Vim's keys, on once the daemon has said so; and a pinned grammar installed
    /// without asking.
    vim_on: bool,
    installs: bool,
    /// The grammars installed and loaded in this window, where they come from, and the ones the
    /// person said "Not now" to.
    loader: Arc<Loader>,
    net: Arc<dyn Http + Send + Sync>,
    declined: HashSet<String>,
    began: Instant,
    _hearing: Subscription,
    _observing: Subscription,
}

impl CodeEditor {
    pub fn new(heard: Entity<Heard>, caller: Caller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        #[cfg(test)]
        tests::remember(cx.weak_entity());
        let hearing = cx.subscribe_in(&heard, window, |this, _, Edited(event), window, cx| this.hear(event, window, cx));
        let observing = cx.observe(&heard, |this, heard, cx| {
            let chosen = heard.read(cx).settings.as_ref().map(|page| (page.chosen.vim, page.chosen.install_grammars));
            let (vim, installs) = chosen.unwrap_or((false, false));
            this.installs = installs;
            if vim != this.vim_on {
                this.vim_on = vim;
                this.vim_switched(cx);
            }
            let up = heard.read(cx).status.as_ref().is_some_and(|status| status.connection == Connection::Connected);
            if up && !this.connected {
                // The files the connection took are opened again now, not when the stage next draws.
                for gesture in this.lives.connected() {
                    this.caller.call(Call::Edit { gesture });
                }
            }
            this.connected = up;
        });
        #[cfg(not(test))]
        let (loader, net): (Arc<Loader>, Arc<dyn Http + Send + Sync>) = (Arc::new(Loader::new(Store::in_config())), Arc::new(Net::new()));
        // A test reaches no network and reads no grammar of the person's own.
        #[cfg(test)]
        let (loader, net) = tests::grammars();
        Self {
            caller,
            lives: Lives::default(),
            sessions: HashMap::new(),
            current: None,
            shown: None,
            connected: false,
            shared: Shared::default(),
            vim_on: false,
            installs: false,
            loader,
            net,
            declined: HashSet::new(),
            began: Instant::now(),
            _hearing: hearing,
            _observing: observing,
        }
    }

    /// Follows the stage's page while it edits a file: a file not seen before is asked for, the
    /// file left is retired, the change marks are the page's, and a line Rust moved to is gone to.
    pub fn follow(&mut self, page: &CodePage, cx: &mut Context<Self>) {
        let Some(path) = page.path.clone() else { return };
        let key = (page.checkout, path);
        if self.current.as_ref() != Some(&key) {
            if let Some(old) = self.current.take() {
                self.retire(&old);
            }
            self.current = Some(key.clone());
            if let Some(gesture) = self.lives.open(key.0, &key.1) {
                self.caller.call(Call::Edit { gesture });
            }
            let closed = matches!(self.lives.get(key.0, &key.1).map(|live| live.status()), Some(Status::Closed(_)));
            let held = !closed && self.shown.as_ref().is_some_and(|shown| self.sessions.get(shown).is_some_and(|s| s.state.is_some()));
            self.sessions.entry(key.clone()).or_insert_with(|| Session::new(page.at));
            // The file before stays on screen until this one's text is here, unless it has none.
            if !held || self.sessions[&key].state.is_some() {
                self.shown = Some(key.clone());
            }
        }
        let Some(session) = self.sessions.get_mut(&key) else { return };
        session.wanted = marks(&page.lines);
        let moved = page.at != session.at;
        session.at = page.at;
        self.mark(&key, cx);
        if moved {
            self.go_to(&key, page.at, cx);
        }
        self.sweep(cx);
    }

    /// The editor is no longer up (the stage reads, or has no file): what is typed is written, and
    /// the file is let go.
    pub fn leave(&mut self, cx: &mut Context<Self>) {
        if let Some(old) = self.current.take() {
            self.retire(&old);
        }
        self.shown = None;
        self.sweep(cx);
    }

    /// What the bar says of the file shown.
    pub fn status(&self) -> Option<Status> {
        self.shown.as_ref().and_then(|key| self.lives.get(key.0, &key.1)).map(|live| live.status().clone())
    }

    /// The editor's field is the window's focus: it holds the keys, whether or not it is drawn.
    pub fn has_keys(&self, window: &Window, cx: &gpui_kit::App) -> bool {
        self.shown_state().is_some_and(|state| state.focus_handle(cx).is_focused(window))
    }

    /// Gives the keys to the field, as opening the file does.
    pub fn take_keys(&self, window: &mut Window, cx: &mut gpui_kit::App) {
        if let Some(state) = self.shown_state() {
            state.update(cx, |state, cx| state.focus(window, cx));
        }
    }

    fn shown_state(&self) -> Option<Entity<EditorState>> {
        self.sessions.get(self.shown.as_ref()?)?.state.clone()
    }

    /// The text of the file shown, as the editor holds it.
    pub fn text(&self, cx: &gpui_kit::App) -> Option<String> {
        let state = self.shown.as_ref().and_then(|key| self.sessions.get(key)).and_then(|s| s.state.as_ref())?;
        Some(state.read(cx).text().to_string())
    }
}

impl Render for CodeEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        if let Some(key) = self.shown.clone() {
            self.settle(&key, cx);
        }
        let shown = self.shown.as_ref().and_then(|key| self.sessions.get(key));
        let (state, read_only) = (shown.and_then(|s| s.state.clone()), shown.is_some_and(|s| s.read_only));
        v_flex()
            .id("code-editor")
            .test_support()
            .size_full()
            .min_h_0()
            .bg(tokens.bg_sunken)
            .child(self.bar(&tokens, cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .children(state.map(|state| Editor::new(&state).readonly(read_only).appearance(false).bordered(false).size_full()))
                    .children(self.theirs(window, cx, &tokens)),
            )
            .children(self.vim_panel(&tokens))
    }
}

#[cfg(test)]
mod tests;
