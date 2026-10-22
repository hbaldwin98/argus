//! What the window does once for the extensions: puts the three layers of their keys in the walk, has the strip name the page by its view, and
//! brings up the two questions (an action's, and an extension's own) while the session holds them.
//!
//! An extension's view is drawn by the element each panel asks for (`bodies`), and the two questions
//! belong to the window and not to one surface, so the shell calls this when it is built and each
//! time it hears the session.

use std::rc::Rc;

use argus_desktop_core::chrome::strip::Says;
use argus_desktop_core::connection::page::Stage;
use argus_desktop_core::extensions::views::page_title;
use argus_desktop_core::keys::keymap::Layer;
use gpui_kit::{App, Entity, Window};

use super::dialog::keys::CardKeys;
use super::keys::{Clicked, Page};
use super::{confirm, dialog};
use crate::connection::caller::Caller;
use crate::chrome::header::slots::{Offer, Slots};
use crate::connection::heard::Heard;
use crate::keys::layer::Layers;

/// Registers the extension layers: a view clicked, the page, and the question's card.
pub fn layers(heard: &Entity<Heard>, cx: &mut App) {
    let layers = Layers::of(cx);
    layers.register(Layer::Extension, Rc::new(Clicked));
    layers.register(Layer::ExtensionPage, Rc::new(Page { heard: heard.downgrade() }));
    layers.register(Layer::ExtensionDialog, Rc::new(CardKeys));
    // The strip names the page by its view, as the page's `extensionPageTitle` does.
    let titled = heard.downgrade();
    Slots::of(cx).offer(
        Stage::Extension,
        Offer::default().says(move |cx| Says {
            title: titled.upgrade().and_then(|heard| page_title(&heard.read(cx).extension_tables).map(str::to_owned)),
            ..Says::default()
        }),
    );
}

/// Puts each question up or away to match what the session holds.
pub fn follow(heard: &Entity<Heard>, caller: &Caller, window: &mut Window, cx: &mut App) {
    let asking = heard.read(cx).extension_confirm.clone();
    confirm::follow(asking.as_ref(), caller, window, cx);
    dialog::follow(heard, caller, window, cx);
}
