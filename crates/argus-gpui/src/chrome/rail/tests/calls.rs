//! What a press asks of the daemon, and that nothing is drawn before the tree says it.

use super::*;

/// What a view draws of the tree: marks, rows and the pick.
fn drawn(cx: &mut TestAppContext, opened: &Opened) -> Vec<String> {
    let mut ids: Vec<String> = Nav::ALL.iter().map(|nav| nav.id().to_owned()).collect();
    ids.extend(["repo-10", "repo-11", "checkout-110", "checkout-112", "pane-1", "pane-2", "pane-4", "pane-5"].map(String::from));
    let mut seen: Vec<String> = ids
        .into_iter()
        .map(|id| {
            let found = cx.update_window(opened.handle, |_, window, _| window.try_find(id.clone()).map(|f| f.selected())).unwrap();
            format!("{id} {found:?}")
        })
        .collect();
    seen.push(format!("picked {:?}", picked(cx, opened)));
    seen
}

#[gpui_kit::test]
fn a_call_the_daemon_refuses_leaves_the_rail_as_it_was(cx: &mut TestAppContext) {
    // CROSS-27: a request's result is never drawn before Rust says it; a
    // refusal reaches the views as a notice and no rail.
    let opened = open(cx, connected_and_busy());
    open_quiet(cx, &opened);
    let before = drawn(cx, &opened);
    let refused = |cx: &mut TestAppContext, what: &str| {
        hear(cx, &opened, vec![Event::Notice { text: format!("{what} refused") }]);
        assert_eq!(drawn(cx, &opened), before, "{what} changed the rail before a Rail was heard");
    };
    hover(cx, &opened, "checkout-110");
    click(cx, &opened, "checkout-110-fold");
    refused(cx, "ToggleCheckout");
    click(cx, &opened, "repo-10");
    refused(cx, "ChooseRepository");
    click(cx, &opened, "checkout-110");
    refused(cx, "RailPick");
    click(cx, &opened, "pane-2");
    refused(cx, "Select");
    // Git is the window's as much as the session's: pressed with no checkout it puts its
    // stage up on the window's word alone, and `review/stage` has its own refusal test.
    for nav in Nav::ALL.into_iter().filter(|nav| *nav != Nav::Git) {
        click(cx, &opened, nav.id());
        refused(cx, nav.id());
    }
    let at = cx.update_window(opened.handle, |_, window, _| window.find("pane-2").bounds().center()).unwrap();
    press(cx, &opened, at, MouseButton::Middle);
    refused(cx, "Close");
    let asked = taken(&opened);
    for call in [
        "ToggleCheckout { checkout: 110 }",
        "ChooseRepository { repository: 10 }",
        "RailPick { checkout: 110, pane: None }",
        "Close { pane: 2 }",
    ] {
        assert!(asked.contains(&call.to_owned()), "{call} was never asked: {asked:?}");
    }
}

#[gpui_kit::test]
fn a_closed_pane_stays_on_the_rail_until_the_tree_drops_it(cx: &mut TestAppContext) {
    // CROSS-27 for the `x` key: the session moves the pick back at once, the row only when the
    // daemon's tree says.
    let opened = open_shell(cx);
    click(cx, &opened, "pane-2");
    hear(cx, &opened, vec![picking(110, Some(2))]);
    type_key(cx, &opened, "x");
    assert!(taken(&opened).contains(&"RailKey { action: Close }".to_owned()));
    hear(cx, &opened, vec![Event::Notice { text: "refused".into() }]);
    assert!(present(cx, &opened, "pane-2"), "a refused close drew the pane gone");
    let mut without = busy();
    without.project.as_mut().unwrap().repositories[0].checkouts[0].panes.retain(|p| p.id != 2);
    hear(cx, &opened, vec![Event::Rail(Box::new(without))]);
    assert!(!present(cx, &opened, "pane-2"));
}

#[gpui_kit::test]
fn what_a_call_asks_is_drawn_once_a_rail_says_it(cx: &mut TestAppContext) {
    let opened = open(cx, connected_and_busy());
    open_quiet(cx, &opened);
    click(cx, &opened, "repo-10");
    assert_eq!(selected(cx, &opened, "repo-10"), Some(false));
    let mut chosen = busy();
    chosen.repository = Some(10);
    hear(cx, &opened, vec![Event::Rail(Box::new(chosen))]);
    assert_eq!(selected(cx, &opened, "repo-10"), Some(true));
    assert_eq!(selected(cx, &opened, "repo-11"), Some(false));
    hover(cx, &opened, "checkout-110");
    click(cx, &opened, "checkout-110-fold");
    assert!(present(cx, &opened, "pane-2"));
    let mut folded = busy();
    folded.project.as_mut().unwrap().repositories[0].checkouts[0].open = false;
    hear(cx, &opened, vec![Event::Rail(Box::new(folded))]);
    assert!(!present(cx, &opened, "pane-2"));
}

#[gpui_kit::test]
fn a_middle_click_on_a_pane_row_closes_that_pane(cx: &mut TestAppContext) {
    let opened = open(cx, connected_and_busy());
    let at = cx.update_window(opened.handle, |_, window, _| window.find("pane-2").bounds().center()).unwrap();
    press(cx, &opened, at, MouseButton::Middle);
    assert_eq!(taken(&opened), ["Close { pane: 2 }"]);
}

#[gpui_kit::test]
fn the_rows_above_the_panes_send_the_pages_calls(cx: &mut TestAppContext) {
    let opened = open(cx, connected_and_busy());
    click(cx, &opened, "repo-11");
    click(cx, &opened, "nav-checkouts");
    click(cx, &opened, "nav-workspace");
    assert_eq!(
        taken(&opened),
        ["ChooseRepository { repository: 11 }", "ShowCheckouts { open: true }", "ShowFeatures { open: false }"]
    );
}

#[gpui_kit::test]
fn a_chevron_folds_without_picking_and_a_checkout_row_asks_to_be_picked(cx: &mut TestAppContext) {
    let opened = open(cx, connected_and_busy());
    assert_eq!(label(cx, &opened, "checkout-110-fold").as_deref(), Some("Fold argus main"));
    hover(cx, &opened, "checkout-110");
    click(cx, &opened, "checkout-110-fold");
    assert_eq!(taken(&opened), ["ToggleCheckout { checkout: 110 }"]);
    assert_eq!(picked(cx, &opened), None);
    let at = cx
        .update_window(opened.handle, |_, window, _| {
            let found = window.find("checkout-110");
            found.bounds().origin + point(px(120.0), found.bounds().size.height / 2.0)
        })
        .unwrap();
    press(cx, &opened, at, MouseButton::Left);
    assert_eq!(taken(&opened), ["ToggleCheckout { checkout: 110 }", "RailPick { checkout: 110, pane: None }"]);
    hear(cx, &opened, vec![picking(110, None)]);
    assert_eq!(picked(cx, &opened), Some((110, None)));
}

#[gpui_kit::test]
fn a_press_on_a_shut_repositorys_row_or_its_chevron_chooses_it(cx: &mut TestAppContext) {
    let opened = open(cx, connected_and_busy());
    open_quiet(cx, &opened);
    click(cx, &opened, "repo-10-chevron");
    assert_eq!(taken(&opened), ["ChooseRepository { repository: 10 }"]);
    click(cx, &opened, "repo-11");
    assert_eq!(taken(&opened).last().map(String::as_str), Some("ChooseRepository { repository: 11 }"));
}
