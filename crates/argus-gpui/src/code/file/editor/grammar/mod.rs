//! The colours of the file in the editor: the grammar installed for it colours it at once, and
//! otherwise one is offered, or installed where Settings says not to ask for one Argus pinned. The
//! decisions are core's (`code::grammar`); this is the window's half: running them off the draw,
//! putting a grammar that loaded where the kit's highlighter reads languages, and colouring every
//! open editor of the file's kind (`ts:code/editor.ts` `findGrammar`, `installOffer`, `colourWith`).

pub mod register;

use std::sync::Arc;

use argus_desktop_core::code::grammar::fetch::Offer;
use argus_desktop_core::code::grammar::install::{find, install, Found};
use argus_desktop_core::code::grammar::load::Loaded;
use argus_desktop_core::code::grammar::slot::{same_kind, Slot};
use gpui_kit::{AppContext as _, Context, Window};

use super::session::Key;
use super::CodeEditor;

impl CodeEditor {
    /// The grammar already loaded for `path`, put in the kit's registry: the name to colour a
    /// first paint with, so a file opens coloured and not plain for a frame (EDITOR-SWITCH-7).
    pub(super) fn loaded_for(&self, path: &str) -> Option<String> {
        let loaded = self.loader.loaded_for(path)?;
        register::loaded(&loaded);
        Some(loaded.name.clone())
    }

    /// Looks for a grammar for the file in `key`, off the draw: what is installed is loaded, or
    /// something is offered, or nothing is.
    pub(super) fn find_grammar(&mut self, key: &Key, cx: &mut Context<Self>) {
        let (path, loader, net) = (key.1.clone(), self.loader.clone(), self.net.clone());
        let (auto, declined) = (self.installs, self.declined.clone());
        let looking = cx.background_spawn(async move { find(&path, &loader, &*net, auto, &declined) });
        let key = key.clone();
        cx.spawn(async move |this, cx| {
            let found = looking.await;
            this.update(cx, |this, cx| this.found(&key, found, cx)).ok();
        })
        .detach();
    }

    fn found(&mut self, key: &Key, found: Found, cx: &mut Context<Self>) {
        if !self.sessions.contains_key(key) {
            return;
        }
        match &found {
            Found::Colour(loaded) => self.colour_with(loaded, &key.1, cx),
            Found::Install(offer) => {
                self.install_offer(key, offer.clone(), cx);
                return;
            }
            _ => {}
        }
        if let Some(session) = self.sessions.get_mut(key) {
            session.grammar = Slot::found(&found);
        }
        cx.notify();
    }

    /// The person said yes to the offer in the bar.
    pub(super) fn install_grammar(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.shown.clone() else { return };
        let Some(offer) = self.sessions.get_mut(&key).and_then(|session| session.grammar.install()) else { return };
        self.install_offer(&key, offer, cx);
    }

    /// Downloads, loads and keeps `offer`, off the draw; the bar says it is installing meanwhile.
    fn install_offer(&mut self, key: &Key, offer: Offer, cx: &mut Context<Self>) {
        let (loader, net) = (self.loader.clone(), self.net.clone());
        if let Some(session) = self.sessions.get_mut(key) {
            session.grammar = Slot::Installing(offer.clone());
        }
        let (key, tried) = (key.clone(), offer.clone());
        let installing = cx.background_spawn(async move { install(&offer, &loader, &*net) });
        cx.spawn(async move |this, cx| {
            let done = installing.await;
            this.update(cx, |this, cx| match done {
                Ok(loaded) => this.colour_with(&loaded, &key.1, cx),
                Err(message) => {
                    if let Some(session) = this.sessions.get_mut(&key) {
                        session.grammar = Slot::installed(&tried, &Err(message));
                    }
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// "Not now": the grammar is not offered again this window, and the keys go back to the editor.
    pub(super) fn not_now(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = self.shown.clone() else { return };
        if let Some(session) = self.sessions.get_mut(&key) {
            session.grammar.not_now(&mut self.declined);
            if let Some(state) = session.state.clone() {
                state.update(cx, |state, cx| state.focus(window, cx));
            }
        }
        cx.notify();
    }

    /// Colours every open editor of the file's kind with `loaded`, and clears what the bar said.
    fn colour_with(&mut self, loaded: &Arc<Loaded>, path: &str, cx: &mut Context<Self>) {
        register::loaded(loaded);
        for (key, session) in self.sessions.iter_mut() {
            if !same_kind(&key.1, path) {
                continue;
            }
            session.grammar = Slot::None;
            session.language = Some(loaded.name.clone());
            if let Some(state) = session.state.clone() {
                state.update(cx, |state, cx| state.set_highlighter(loaded.name.clone(), cx));
            }
        }
        cx.notify();
    }
}
