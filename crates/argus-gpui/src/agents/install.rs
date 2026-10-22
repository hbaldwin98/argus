//! What the window does once for the agents: puts the composer where the stage draws it, the sessions
//! dialog's two layers in the key walk, and brings the dialog up while the session holds it.
//!
//! The shell calls [`install`] when it is built and [`follow`] each time it hears the session; nothing
//! else knows which entities draw the composer and the dialog.

use std::rc::Rc;

use argus_desktop_core::connection::page::Stage;
use argus_desktop_core::keys::keymap::Layer;
use gpui_kit::{App, AppContext as _, Entity, Window};

use super::compose::ComposeView;
use super::sessions::keys::SessionsKeys;
use super::sessions::{Held, SessionsView};
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::keys::layer::Layers;
use crate::layout::bodies::Contents;

pub fn install(heard: &Entity<Heard>, caller: &Caller, window: &mut Window, cx: &mut App) {
    let compose = cx.new(|cx| ComposeView::new(heard.clone(), caller.clone(), window, cx));
    Contents::of(cx).offer_stage(Stage::Composer, compose.into());
    let sessions = cx.new(|cx| SessionsView::new(heard.clone(), caller.clone(), window, cx));
    let layers = Layers::of(cx);
    layers.register(Layer::Sessions, Rc::new(SessionsKeys { view: sessions.downgrade(), reading: false }));
    layers.register(Layer::Transcript, Rc::new(SessionsKeys { view: sessions.downgrade(), reading: true }));
    cx.set_global(Held(sessions));
}

/// Puts the sessions dialog up or away to match what the session holds.
pub fn follow(window: &mut Window, cx: &mut App) {
    if let Some(Held(view)) = cx.try_global::<Held>().cloned() {
        SessionsView::follow(&view, window, cx);
    }
}
