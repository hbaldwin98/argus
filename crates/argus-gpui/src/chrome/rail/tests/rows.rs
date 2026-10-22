//! What the rail lists of the tree it was given.

use super::*;

#[gpui_kit::test]
fn the_rail_lists_the_rows_of_the_rail_it_was_given(cx: &mut TestAppContext) {
    let opened = open(cx, connected_and_busy());
    for nav in Nav::ALL {
        assert_eq!(label(cx, &opened, nav.id()).as_deref(), Some(nav.label()));
    }
    assert_eq!(label(cx, &opened, "project-1").as_deref(), Some("src"));
    assert_eq!(label(cx, &opened, "project-2").as_deref(), Some("other"));
    assert_eq!(label(cx, &opened, "repo-11").as_deref(), Some("argus"));
    assert_eq!(label(cx, &opened, "repo-11-count").as_deref(), Some("3"));
    // A quiet repository says how long since it was used, and draws nothing under it while shut.
    assert_eq!(label(cx, &opened, "repo-10-count").as_deref(), Some("16m"));
    assert!(!present(cx, &opened, "checkout-100"));
    assert!(!present(cx, &opened, "pane-6"));
    assert_eq!(label(cx, &opened, "checkout-110").as_deref(), Some("main checkout on main"));
    assert_eq!(label(cx, &opened, "pane-1").as_deref(), Some("Trim context · agent, working"));
    assert_eq!(label(cx, &opened, "pane-2").as_deref(), Some("zsh · shell, idle"));
    // Only what asks for a look is said in words.
    assert_eq!(label(cx, &opened, "pane-4-word").as_deref(), Some("review"));
    assert!(!present(cx, &opened, "pane-1-word"));
    assert!(!present(cx, &opened, "pane-2-word"));
    // A folded checkout draws no pane and speaks for them: how many, and who shares it.
    assert!(!present(cx, &opened, "pane-5"));
    assert_eq!(label(cx, &opened, "checkout-112-folded").as_deref(), Some("1 pane folded away"));
    assert_eq!(label(cx, &opened, "checkout-112-shared").as_deref(), Some("shared by 2 agents"));
}

#[gpui_kit::test]
fn the_pane_on_screen_and_the_chosen_repository_are_marked(cx: &mut TestAppContext) {
    let opened = open(cx, connected_and_busy());
    assert_eq!(selected(cx, &opened, "pane-1"), Some(true));
    assert_eq!(selected(cx, &opened, "pane-2"), Some(false));
    assert_eq!(selected(cx, &opened, "repo-11"), Some(true));
    assert_eq!(selected(cx, &opened, "project-1"), Some(true));
    assert_eq!(selected(cx, &opened, "nav-workspace"), Some(true));
    assert_eq!(selected(cx, &opened, "nav-code"), Some(false));
}

#[gpui_kit::test]
fn a_rail_not_yet_loaded_lists_nothing_and_an_empty_one_says_so(cx: &mut TestAppContext) {
    let opened = open(cx, vec![status(Connection::Connecting)]);
    assert!(present(cx, &opened, "nav-workspace"));
    assert!(!present(cx, &opened, "rail-empty"));
    assert!(present(cx, &opened, "rail-wait"));
    let empty = Rail { loaded: true, ..Rail::default() };
    hear(cx, &opened, vec![Event::Rail(Box::new(empty))]);
    assert!(present(cx, &opened, "rail-empty"));
    assert!(!present(cx, &opened, "rail-wait"));
}

#[gpui_kit::test]
fn a_click_on_a_pane_row_selects_that_pane_and_picks_the_row(cx: &mut TestAppContext) {
    let opened = open(cx, connected_and_busy());
    click(cx, &opened, "pane-2");
    assert_eq!(taken(&opened), ["Select { pane: 2 }"]);
    cx.update(|cx| {
        assert_eq!(opened.rail.read(cx).picked(), Some(Picked { checkout: 110, pane: Some(2) }));
    });
    // The mark on screen is the daemon's to move, not the click's.
    assert_eq!(selected(cx, &opened, "pane-1"), Some(true));
    assert_eq!(selected(cx, &opened, "pane-2"), Some(false));
}

#[gpui_kit::test]
fn a_row_a_carried_pane_would_move_to_is_drawn_as_the_target(cx: &mut TestAppContext) {
    use crate::kit::spots::Spots;
    let opened = open(cx, connected_and_busy());
    assert!(!present(cx, &opened, "checkout-112-target"));
    cx.update(|cx| Spots::of(cx).light(Some(112)));
    cx.update_window(opened.handle, |_, window, cx| {
        window.refresh();
        window.render_frame(cx)
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| opened.rail.update(cx, |_, cx| cx.notify()));
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    assert!(present(cx, &opened, "checkout-112-target"));
    assert!(present(cx, &opened, "checkout-112-ring"));
    // The pane's own checkout is not offered, and a row not lit is not ringed unless picked.
    assert!(!present(cx, &opened, "checkout-110-target"));
    assert!(!present(cx, &opened, "checkout-110-ring"));
    cx.update(|cx| Spots::of(cx).light(None));
    cx.update(|cx| opened.rail.update(cx, |_, cx| cx.notify()));
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    assert!(!present(cx, &opened, "checkout-112-target"));
}
