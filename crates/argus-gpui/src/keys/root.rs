//! The key layer at the root: every keystroke of the window is heard here before any focused
//! child, and walked through the layers `argus-desktop-core` orders (`LAYERS`).
//!
//! GPUI offers `App::intercept_keystrokes` for this: it runs before bindings, actions and the
//! platform's text for the key, and a `stop_propagation` in it stops all three (and, on Windows,
//! the `WM_CHAR` that would type it). A wrapper element's `capture_key_down` is too late, since
//! the kit's input has already bound its keys by then; bindings cannot do the leader or the
//! layered "first layer up has it" order. So the one interceptor translates the keystroke to the
//! page's key, gives the window's own keys first (zoom, SHELL-KEYS-3), then asks each layer that
//! is up in order what it makes of it: a layer passes it on, holds it, or runs it.
//!
//! A key the page has no name for (a bare modifier, an input method's text) is not walked; it goes
//! on to the input handler, as it did before.

use std::rc::Rc;

use argus_desktop_core::keys::keymap::{Layer, Place, LAYERS};
use gpui_kit::{App, Global, KeystrokeEvent, Subscription, Window};

use super::field::Fields;
use super::focus;
use super::layer::{action_for, Handler as _, Layers, Press, Verdict};
use super::layers::{figure::Figure, menu::Menu, own::Own, sheet::Sheet, stage::Stage};
use super::page_key;
use crate::kit::overlay::kind::Overlay;

/// Registers the layers that need no surface and listens to every keystroke. Called once when the
/// window starts, after the overlay host is installed.
pub fn install(cx: &mut App) {
    let layers = Layers::of(cx);
    let fields = Fields::of(cx);
    layers.register(Layer::Field, Rc::new(fields));
    layers.register(Layer::ExtensionDialog, Rc::new(Own(Overlay::Extension)));
    layers.register(Layer::Settings, Rc::new(Own(Overlay::Settings)));
    layers.register(Layer::Sheet, Rc::new(Sheet));
    layers.register(Layer::AddProject, Rc::new(Own(Overlay::AddProject)));
    layers.register(Layer::Menu, Rc::new(Menu));
    layers.register(Layer::Stage, Rc::new(Stage));
    // One interceptor however often the window is initialised: two would walk every key twice.
    if !cx.has_global::<Intercepting>() {
        let listening = cx.intercept_keystrokes(intercept);
        cx.set_global(Intercepting(listening));
    }
}

/// The interceptor, kept for as long as the app runs.
struct Intercepting(#[allow(dead_code)] Subscription);

impl Global for Intercepting {}

fn intercept(event: &KeystrokeEvent, window: &mut Window, cx: &mut App) {
    let Some(page) = page_key::from_keystroke(&event.keystroke) else { return };
    // The push-to-talk chord is the window's before anything's, as the leader is (`dictation::keys`).
    if crate::dictation::keys::heard(&page, window, cx) {
        cx.stop_propagation();
        return;
    }
    let press = Press { keystroke: &event.keystroke, page: &page };
    // A drawing opened larger holds every key, before the window's own and every layer.
    if Figure.open(window, cx) {
        if Figure.keys(&press, &[], window, cx) == Verdict::Taken {
            cx.stop_propagation();
        }
        return;
    }
    let layers = Layers::of(cx);
    // The window's keys come first: no field, dialog or pane hears them.
    if let Some(action) = action_for(&[Place::Window], &page) {
        if layers.command(action) {
            cx.stop_propagation();
            return;
        }
    }
    let up: Vec<_> = focus::up(LAYERS, |layer| layers.is_up(layer, window, cx)).map(|layer| (layer.layer, layer.places)).collect();
    for (layer, places) in up {
        let Some(handler) = layers.handler(layer) else { continue };
        match handler.keys(&press, places.unwrap_or(&[]), window, cx) {
            Verdict::Pass => {}
            Verdict::Held => return,
            Verdict::Taken => {
                cx.stop_propagation();
                return;
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
