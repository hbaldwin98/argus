//! Which layers and places of the keymap this window hears, and which arrive with their surface.
//!
//! `argus-desktop-core` lists 24 layers and 26 places. A key bound in one the window does not hear
//! would be dead without anything saying so, so each is named here as hooked (a surface registered
//! its handler, `keys/layer`, or a field its place, `keys/field`) or as arriving with the surface
//! that owns its state. The matches are exhaustive: a layer or place added to the keymap does not
//! compile until it is listed, and the tests hold the list to what a built window registers.

use argus_desktop_core::keys::keymap::{Layer, Place};

/// Whether the window hears a layer or place now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hook {
    Hooked,
    /// Heard once this surface is in the window, which registers it when it is built.
    Arrives(&'static str),
}

pub fn layer(layer: Layer) -> Hook {
    match layer {
        Layer::Field
        | Layer::ExtensionDialog
        | Layer::Settings
        | Layer::Sheet
        | Layer::AddProject
        | Layer::Review
        | Layer::History
        | Layer::Rail
        | Layer::Start
        | Layer::Menu
        | Layer::Stage => Hook::Hooked,
        Layer::Link | Layer::Reconcile => Hook::Arrives("the tracker dialogs"),
        Layer::Transcript | Layer::Sessions => Hook::Arrives("the sessions dialog"),
        Layer::Extension | Layer::ExtensionPage => Hook::Arrives("extension views"),
        Layer::Capture | Layer::Diagram | Layer::Outline | Layer::Features => Hook::Arrives("the Feature stage"),
        Layer::Files | Layer::Checkouts => Hook::Arrives("the Checkouts stage (audit 59 to 64)"),
        Layer::Code => Hook::Arrives("the Code stage (audit 65 to 71)"),
    }
}

pub fn place(place: Place) -> Hook {
    match place {
        // The Stage layer and every layer that lists it; the session for a key after the leader.
        Place::Anywhere | Place::Pane => Hook::Hooked,
        // The root, before any layer.
        Place::Window => Hook::Hooked,
        // The layers above.
        Place::Rail | Place::Start | Place::Menu | Place::Review | Place::History | Place::Sheet => Hook::Hooked,
        // The fields that answer them: the jump field, the history's filter, the comment box, the palette's search.
        Place::Jump | Place::Filter | Place::Writing | Place::Palette => Hook::Hooked,
        Place::Feature | Place::Card | Place::Outline | Place::Diagram => Hook::Arrives("the Feature stage"),
        Place::Checkouts | Place::Files => Hook::Arrives("the Checkouts stage (audit 59 to 64)"),
        Place::Sessions | Place::Transcript => Hook::Arrives("the sessions dialog"),
        Place::Address => Hook::Arrives("the Browsers stage"),
        Place::Extension => Hook::Arrives("extension views"),
        Place::Code => Hook::Arrives("the Code stage (audit 65 to 71)"),
        Place::Link | Place::Reconcile => Hook::Arrives("the tracker dialogs"),
    }
}

#[cfg(test)]
mod tests;
