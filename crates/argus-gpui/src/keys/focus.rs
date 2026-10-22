//! Which layer of the window has a key outside a pane: the order is `LAYERS` in
//! `argus-desktop-core`, and what each layer does is the surface's own (`keys/layer`).
//!
//! No window in it, as `page/src/keys/focus.ts` has none: the walk is given what asks whether a
//! layer is up and what puts a layer's Escape away, so the order, the first-up rule and the
//! ladder are tested once with plain closures.

use argus_desktop_core::keys::keymap::{KeyLayer, Layer};

/// The layers that are up, the one with the keys first. Lazy, since most keys stop at the first.
pub fn up<'a>(order: &'a [KeyLayer], mut open: impl FnMut(Layer) -> bool + 'a) -> impl Iterator<Item = &'a KeyLayer> + 'a {
    order.iter().filter(move |layer| open(layer.layer))
}

/// Whether a layer that takes the keys from a pane is up.
pub fn takes_from_pane(order: &[KeyLayer], mut open: impl FnMut(Layer) -> bool) -> bool {
    order.iter().any(|layer| layer.takes_from_pane && open(layer.layer))
}

/// Puts away the first thing Escape finds, in the layers' order. Each hook is asked whether its
/// layer is up or not: a question outlives the menu drawn over it and is still answered no
/// (SHELL-KEYS-19).
pub fn escape_from(order: &[KeyLayer], mut escape: impl FnMut(Layer) -> bool) -> bool {
    order.iter().any(|layer| escape(layer.layer))
}

#[cfg(test)]
mod tests;
