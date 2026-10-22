//! How wide the rail is, its edge, and what it draws when it is narrow.

use super::*;

fn width_of(cx: &mut TestAppContext, opened: &Opened) -> f32 {
    cx.update_window(opened.handle, |_, window, _| window.find("rail").bounds().size.width.as_f32()).unwrap()
}

fn with_width(width: Option<u32>) -> Event {
    let mut rail = busy();
    rail.width = width;
    Event::Rail(Box::new(rail))
}

#[gpui_kit::test]
fn the_rail_is_the_stylesheets_width_until_one_is_dragged_and_then_the_width_rust_kept(cx: &mut TestAppContext) {
    // SHELL-RAIL-32.
    let opened = open(cx, connected_and_busy());
    assert_eq!(width_of(cx, &opened), 288.0);
    hear(cx, &opened, vec![with_width(Some(360))]);
    assert_eq!(width_of(cx, &opened), 360.0);
    hear(cx, &opened, vec![with_width(None)]);
    assert_eq!(width_of(cx, &opened), 288.0, "a double-click gave the usual width back");
}

#[gpui_kit::test]
fn a_window_of_1200_or_less_folds_the_rail_whatever_it_was_dragged_to_and_hides_its_sash(cx: &mut TestAppContext) {
    // SHELL-STAGE-15.
    let opened = open_wide(cx, vec![status(Connection::Connected), with_width(Some(400))], 1200.0);
    assert_eq!(width_of(cx, &opened), 56.0);
    assert!(!present(cx, &opened, "rail-sash"), "the sash goes with the words");
    let wide = open_wide(cx, vec![status(Connection::Connected), with_width(Some(400))], 1201.0);
    assert_eq!(width_of(cx, &wide), 400.0);
    assert!(present(cx, &wide, "rail-sash"));
}

fn sash_at(cx: &mut TestAppContext, opened: &Opened) -> gpui_kit::Point<gpui_kit::Pixels> {
    cx.update_window(opened.handle, |_, window, _| window.find("rail-sash").bounds().center()).unwrap()
}

fn drag(cx: &mut TestAppContext, opened: &Opened, from: gpui_kit::Point<gpui_kit::Pixels>, to_x: f32) {
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(from, MouseButton::Left, cx);
        window.pointer_move(point(from.x + px(8.0), from.y), Some(MouseButton::Left), cx);
        window.pointer_move(point(px(to_x), from.y), Some(MouseButton::Left), cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_edge_draws_the_width_live_and_reports_it_once_on_release(cx: &mut TestAppContext) {
    // SHELL-EDGE-1, 2: the column keeps the point it was grabbed at under the pointer.
    let opened = open(cx, connected_and_busy());
    let from = sash_at(cx, &opened);
    let grabbed = from.x.as_f32();
    drag(cx, &opened, from, grabbed + 62.0);
    assert_eq!(width_of(cx, &opened), 288.0 + 62.0, "drawn live");
    assert!(opened.heard.read_with(cx, |heard, _| heard.resizing()), "the panes wait for the size to settle");
    assert!(taken(&opened).is_empty(), "nothing is sent while it is held: {:?}", taken(&opened));
    cx.update_window(opened.handle, |_, window, cx| window.pointer_up(point(px(grabbed + 62.0), from.y), MouseButton::Left, cx)).unwrap();
    cx.run_until_parked();
    assert_eq!(taken(&opened), ["RailWidth { width: Some(350) }"]);
    assert!(!opened.heard.read_with(cx, |heard, _| heard.resizing()));
}

#[gpui_kit::test]
fn a_drag_let_go_nearer_the_folded_width_is_reported_as_it_is_for_rust_to_fold(cx: &mut TestAppContext) {
    // SHELL-EDGE-3: Rust holds the bounds and the fold; the window draws and says.
    let opened = open(cx, connected_and_busy());
    let from = sash_at(cx, &opened);
    drag(cx, &opened, from, 100.0);
    assert!(width_of(cx, &opened) < 128.0, "drawn live at the pointer");
    cx.update_window(opened.handle, |_, window, cx| window.pointer_up(point(px(100.0), from.y), MouseButton::Left, cx)).unwrap();
    cx.run_until_parked();
    let sent = taken(&opened);
    let width: u32 = sent[0].trim_start_matches("RailWidth { width: Some(").trim_end_matches(") }").parse().expect("a width");
    assert!(width < 128, "{sent:?}: under the fold's halfway point Rust folds it to 56");
}

#[gpui_kit::test]
fn a_double_click_on_the_edge_asks_for_the_usual_width(cx: &mut TestAppContext) {
    // SHELL-EDGE-2.
    let opened = open(cx, vec![status(Connection::Connected), with_width(Some(400))]);
    let at = sash_at(cx, &opened);
    cx.update_window(opened.handle, |_, window, cx| {
        window.dispatch_event(
            gpui_kit::InputEvent::to_platform_input(gpui_kit::MouseDownEvent { button: MouseButton::Left, position: at, click_count: 2, ..Default::default() }),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(taken(&opened), ["RailWidth { width: None }"]);
}

#[gpui_kit::test]
fn a_narrow_rail_draws_icons_for_the_stages_and_tiles_of_initials_for_the_panes(cx: &mut TestAppContext) {
    // SHELL-RAIL-16, 33.
    let opened = open_wide(cx, connected_and_busy(), 900.0);
    for nav in Nav::ALL {
        assert!(present(cx, &opened, nav.id()), "{}", nav.label());
    }
    assert_eq!(label(cx, &opened, "pane-1").as_deref(), Some("Trim context · agent, working"), "every name is in the tooltip");
    assert!(present(cx, &opened, "pane-1-dot"), "the status is a dot on the tile's corner");
    assert!(!present(cx, &opened, "pane-4-word"), "no words");
    assert!(!present(cx, &opened, "checkout-110"), "no checkout rows");
    assert!(!present(cx, &opened, "checkout-110-fold"));
    assert!(!present(cx, &opened, "repo-11-count"), "a repository is a hairline");
    assert!(present(cx, &opened, "repo-11"));
    assert!(!present(cx, &opened, "foot-need-you"));
    assert!(present(cx, &opened, "foot-console") && present(cx, &opened, "foot-chime") && present(cx, &opened, "foot-settings"));
    let tile = cx.update_window(opened.handle, |_, window, _| window.find("pane-1").bounds().size).unwrap();
    assert_eq!((tile.width.as_f32(), tile.height.as_f32()), (40.0, 40.0));
}

#[gpui_kit::test]
fn a_tile_still_picks_and_shows_its_pane_and_a_wide_rail_has_no_tiles(cx: &mut TestAppContext) {
    let opened = open_wide(cx, connected_and_busy(), 900.0);
    click(cx, &opened, "pane-2");
    assert_eq!(taken(&opened), ["RailPick { checkout: 110, pane: Some(2) }", "Select { pane: 2 }"]);
    let wide = open(cx, connected_and_busy());
    assert!(!present(cx, &wide, "pane-1-dot"));
}

#[gpui_kit::test]
fn a_rail_dragged_to_120_or_less_draws_icons_and_one_wider_draws_words(cx: &mut TestAppContext) {
    // The container query (`max-width: 120px`) is on the rail's own width, not the window's.
    let narrow = open(cx, vec![status(Connection::Connected), with_width(Some(56))]);
    assert!(present(cx, &narrow, "pane-1-dot"));
    let wide = open(cx, vec![status(Connection::Connected), with_width(Some(200))]);
    assert!(!present(cx, &wide, "pane-1-dot"));
}
