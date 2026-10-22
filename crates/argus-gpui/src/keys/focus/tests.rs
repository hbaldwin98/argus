use super::*;
use argus_desktop_core::keys::keymap::LAYERS;

fn names<'a>(layers: impl Iterator<Item = &'a KeyLayer>) -> Vec<Layer> {
    layers.map(|layer| layer.layer).collect()
}

/// SHELL-KEYS-10: the first layer up in `LAYERS` order has the keys, whatever order they came up in.
#[test]
fn the_layers_up_come_in_the_tables_order() {
    let open = [Layer::Stage, Layer::Rail, Layer::Sheet, Layer::Menu];
    let up = names(up(LAYERS, |layer| open.contains(&layer)));
    assert_eq!(up, [Layer::Sheet, Layer::Rail, Layer::Menu, Layer::Stage]);
}

/// SHELL-KEYS-10: the stage is always up and last.
#[test]
fn the_stage_alone_is_up_when_nothing_else_is() {
    let up = names(up(LAYERS, |layer| layer == Layer::Stage));
    assert_eq!(up, [Layer::Stage]);
}

/// The walk is lazy: a key that the first layer holds never asks the layers below it.
#[test]
fn the_layers_are_asked_only_as_far_as_the_walk_goes() {
    let mut asked = Vec::new();
    let first = up(LAYERS, |layer| {
        asked.push(layer);
        true
    })
    .next()
    .map(|layer| layer.layer);
    assert_eq!(first, Some(Layer::Field));
    assert_eq!(asked, [Layer::Field]);
}

/// SHELL-KEYS-10: the dialogs over a pane are the ones that take its keys.
#[test]
fn a_dialog_over_a_pane_is_the_layer_that_takes_its_keys() {
    assert!(takes_from_pane(LAYERS, |layer| layer == Layer::Review));
    assert!(takes_from_pane(LAYERS, |layer| layer == Layer::AddProject));
    assert!(!takes_from_pane(LAYERS, |layer| layer == Layer::Rail));
    assert!(!takes_from_pane(LAYERS, |layer| layer == Layer::Sheet), "the sheet leaves the pane's keys alone");
}

/// SHELL-KEYS-19: Escape asks each layer in order, up or not, and stops at the first that puts
/// something away.
#[test]
fn escape_puts_away_the_first_thing_in_the_order_up_or_not() {
    let mut asked = Vec::new();
    let put = escape_from(LAYERS, |layer| {
        asked.push(layer);
        layer == Layer::Capture
    });
    assert!(put);
    assert_eq!(asked.last(), Some(&Layer::Capture));
    assert!(!asked.contains(&Layer::Outline), "the hooks after the one that answered are not asked");
    let menu = asked.iter().position(|&layer| layer == Layer::Menu).expect("asked");
    let sessions = LAYERS.iter().position(|layer| layer.layer == Layer::Sessions).expect("listed");
    assert!(menu > sessions, "the menu is asked in the table's order, after the sessions dialog");
}

#[test]
fn escape_with_nothing_to_put_away_says_so() {
    assert!(!escape_from(LAYERS, |_| false));
}
