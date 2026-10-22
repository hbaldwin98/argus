//! What a layer of the window is to the key walk: the three questions each answers (is it up, what
//! does it do with a key, what does Escape put away in it), and the registry the surfaces put
//! themselves in.
//!
//! `argus-desktop-core` orders the layers and says which places each hears (`LAYERS`); the
//! surface that owns a layer's state implements [`Handler`] for it and registers it when it is
//! built, as the page's `handlers` table does. The registry holds no view: a handler keeps a weak
//! handle on its own, so one whose surface is gone is simply not up.

use std::cell::RefCell;
use std::rc::Rc;

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::keys::keymap::{Action, Layer, Place, BINDINGS};
use argus_desktop_core::keys::page_key::PageKey;
use gpui_kit::{App, Global, Keystroke, Window};

use super::calls;
use crate::connection::caller::Caller;

/// The key being walked: GPUI's keystroke, and the page's key it translates to.
pub struct Press<'a> {
    pub keystroke: &'a Keystroke,
    pub page: &'a PageKey,
}

/// What a layer made of a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Not this layer's: the next layer up hears it (SHELL-KEYS-23).
    Pass,
    /// This layer has the key and does nothing with it: a field's own typing, or a dialog that
    /// answers its own keys. The platform goes on with it, and no layer above hears it.
    Held,
    /// This layer ran it. Nothing else sees the key, the platform's text included.
    Taken,
}

/// What one layer does while it has the keys.
pub trait Handler {
    /// Whether the layer is up, the keys being where `window` has them.
    fn open(&self, window: &Window, cx: &App) -> bool;

    /// A key, with the places the layer hears in the order it asks them (empty for a layer that
    /// answers its own keys). A layer holds only the keys it binds (SHELL-KEYS-23) unless it says
    /// otherwise, so the default holds and does nothing.
    fn keys(&self, press: &Press, places: &[Place], window: &mut Window, cx: &mut App) -> Verdict {
        let _ = (press, places, window, cx);
        Verdict::Held
    }

    /// What Escape puts away here, saying whether there was anything. Asked whether the layer is
    /// up or not (SHELL-KEYS-19).
    fn escape(&self, window: &mut Window, cx: &mut App) -> bool {
        let _ = (window, cx);
        false
    }
}

/// What `page` is in `places`, asked in the order given, a leader's chords aside: the first
/// binding that matches wins, so a place listed first takes a key from one after it (an open
/// feature's digits are its lenses before they are the stages, SHELL-KEYS-2).
pub fn action_for(places: &[Place], page: &PageKey) -> Option<Action> {
    places.iter().find_map(|&place| {
        BINDINGS
            .iter()
            .filter(|binding| binding.place == place)
            .find(|binding| binding.chords.iter().any(|chord| !chord.leader && chord.matches(page)))
            .map(|binding| binding.action)
    })
}

#[derive(Default)]
struct State {
    handlers: Vec<(Layer, Rc<dyn Handler>)>,
    /// The way out for the window's own commands, which the session runs.
    caller: Option<Caller>,
}

/// The window's layers and the one way out they share. A handle: clones share one registry.
#[derive(Clone, Default)]
pub struct Layers(Rc<RefCell<State>>);

impl Global for Layers {}

impl Layers {
    /// The app's registry, made on first use so a view built alone, in a test, still has one.
    pub fn of(cx: &mut App) -> Self {
        cx.default_global::<Layers>().clone()
    }

    /// Puts `handler` in as `layer`'s, in place of the one there.
    pub fn register(&self, layer: Layer, handler: Rc<dyn Handler>) {
        let mut state = self.0.borrow_mut();
        state.handlers.retain(|(held, _)| *held != layer);
        state.handlers.push((layer, handler));
    }

    pub fn handler(&self, layer: Layer) -> Option<Rc<dyn Handler>> {
        self.0.borrow().handlers.iter().find(|(held, _)| *held == layer).map(|(_, handler)| handler.clone())
    }

    /// Whether a surface has registered for `layer`.
    pub fn hooked(&self, layer: Layer) -> bool {
        self.handler(layer).is_some()
    }

    /// Whether `layer` is up now.
    pub fn is_up(&self, layer: Layer, window: &Window, cx: &App) -> bool {
        self.handler(layer).is_some_and(|handler| handler.open(window, cx))
    }

    /// Gives the window the way it asks the session to run a command.
    pub fn serve(&self, caller: Caller) {
        self.0.borrow_mut().caller = Some(caller);
    }

    /// Asks the session to run `action` as the page does (`keys/calls`); whether there is a call
    /// for it and somewhere to send it.
    pub fn command(&self, action: Action) -> bool {
        calls::of(action).is_some_and(|call| self.call(call))
    }

    /// Sends `call` where the window serves them; whether there was somewhere.
    pub fn call(&self, call: Call) -> bool {
        let caller = self.0.borrow().caller.clone();
        caller.map(|caller| caller.call(call)).is_some()
    }
}

#[cfg(test)]
mod tests;
