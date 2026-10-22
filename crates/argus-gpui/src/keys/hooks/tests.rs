//! Every layer and place of the keymap is hooked, or listed as arriving with its surface, and the
//! list is what a built window registers.

use argus_desktop_core::keys::keymap::{groups, Place, LAYERS};
use gpui_kit::TestAppContext;

use super::*;
use crate::keys::field::Fields;
use crate::keys::layer::Layers;
use crate::keys::root::tests::open;

/// Every layer of `LAYERS` has a handler in the built window exactly when the table says hooked, so
/// a surface that registers (or stops registering) must change the table with it.
#[gpui_kit::test]
fn every_layer_is_hooked_or_listed_as_arriving_with_its_surface(cx: &mut TestAppContext) {
    let _opened = open(cx);
    let layers = cx.update(Layers::of);
    for layer in LAYERS {
        let hooked = layers.hooked(layer.layer);
        match super::layer(layer.layer) {
            Hook::Hooked => assert!(hooked, "{:?} is listed hooked and nothing registered it", layer.layer),
            Hook::Arrives(surface) => {
                assert!(!hooked, "{:?} is registered but still listed as arriving with {surface}", layer.layer);
                assert!(!surface.is_empty());
            }
        }
    }
}

/// A place is heard when the root, the session, a hooked layer or a field answers it, and is listed
/// as arriving only while none does.
#[gpui_kit::test]
fn every_place_is_hooked_or_listed_as_arriving_with_its_surface(cx: &mut TestAppContext) {
    let _opened = open(cx);
    let layers = cx.update(Layers::of);
    let fields = cx.update(|cx| Fields::of(cx).places());
    for group in groups() {
        let place = group.place;
        let by_layer = LAYERS.iter().any(|layer| layer.places.is_some_and(|places| places.contains(&place)) && layers.hooked(layer.layer));
        let by_field = fields.contains(&place);
        let by_root = matches!(place, Place::Window | Place::Pane);
        match super::place(place) {
            Hook::Hooked => assert!(by_layer || by_field || by_root, "{place:?} is listed hooked and nothing hears it"),
            Hook::Arrives(surface) => {
                assert!(!by_field && !by_root, "{place:?} is answered but still listed as arriving with {surface}");
                // `Anywhere` is heard by every layer that lists it; no arriving place shares one.
                assert!(!by_layer, "{place:?} is heard by a hooked layer but still listed as arriving with {surface}");
            }
        }
    }
}
