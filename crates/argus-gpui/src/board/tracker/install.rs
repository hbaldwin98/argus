//! What the window does once for the tracker dialogs: makes the link dialog and the reconcile dialog,
//! which open themselves over the window while the session holds them, and puts their two layers in
//! the key walk.
//!
//! Called once by the shell; nothing else knows which entities draw the dialogs.

use argus_desktop_core::keys::keymap::Layer;
use gpui_kit::{App, AppContext as _, Entity, Global, Window};
use std::rc::Rc;

use super::keys::DialogKeys;
use super::link::LinkView;
use super::reconcile::ReconcileView;
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::keys::layer::Layers;
use crate::kit::overlay::kind::Overlay;

/// The two dialogs, kept for as long as the window is.
pub struct Dialogs {
    pub link: Entity<LinkView>,
    pub reconcile: Entity<ReconcileView>,
}

impl Global for Dialogs {}

pub fn install(heard: &Entity<Heard>, caller: &Caller, window: &mut Window, cx: &mut App) {
    let link = cx.new(|cx| LinkView::new(heard.clone(), caller.clone(), window, cx));
    let reconcile = cx.new(|cx| ReconcileView::new(heard.clone(), caller.clone(), window, cx));
    let layers = Layers::of(cx);
    layers.register(Layer::Link, Rc::new(DialogKeys { overlay: Overlay::Link, sink: LinkView::sink(&link) }));
    layers.register(Layer::Reconcile, Rc::new(DialogKeys { overlay: Overlay::Reconcile, sink: ReconcileView::sink(&reconcile) }));
    cx.set_global(Dialogs { link, reconcile });
}
