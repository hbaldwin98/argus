//! Where the pick goes when the tree changes under it.

use super::*;
use crate::chrome::rail::picked::settled;

#[gpui_kit::test]
fn folding_the_checkout_of_the_picked_pane_moves_the_pick_to_the_checkout(cx: &mut TestAppContext) {
    let opened = open(cx, connected_and_busy());
    click(cx, &opened, "pane-2");
    click(cx, &opened, "checkout-110-fold");
    assert_eq!(taken(&opened), ["Select { pane: 2 }", "ToggleCheckout { checkout: 110 }"]);
    cx.update(|cx| {
        assert_eq!(opened.rail.read(cx).picked(), Some(Picked { checkout: 110, pane: None }));
    });
}

#[gpui_kit::test]
fn closing_the_picked_pane_moves_the_pick_to_its_checkout(cx: &mut TestAppContext) {
    let opened = open(cx, connected_and_busy());
    click(cx, &opened, "pane-2");
    let mut without = busy();
    let repo = &mut without.project.as_mut().unwrap().repositories[0];
    repo.checkouts[0].panes.retain(|p| p.id != 2);
    hear(cx, &opened, vec![Event::Rail(Box::new(without))]);
    assert!(!present(cx, &opened, "pane-2"));
    cx.update(|cx| {
        assert_eq!(opened.rail.read(cx).picked(), Some(Picked { checkout: 110, pane: None }));
    });
}

#[test]
fn a_pick_follows_the_tree_it_was_made_in() {
    let rail = busy();
    let on = |checkout, pane| Some(Picked { checkout, pane });
    assert_eq!(settled(None, &rail), None);
    assert_eq!(settled(on(110, Some(2)), &rail), on(110, Some(2)));
    assert_eq!(settled(on(110, Some(9)), &rail), on(110, None), "a pane that is gone");
    assert_eq!(settled(on(112, Some(5)), &rail), on(112, None), "a pane of a folded checkout");
    assert_eq!(settled(on(77, None), &rail), on(77, None), "a checkout that is gone is not the pick's to drop");
    assert_eq!(settled(on(100, None), &rail), on(100, None), "a checkout of a shut repository likewise");
}
