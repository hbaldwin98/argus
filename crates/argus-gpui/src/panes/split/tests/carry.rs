//! A pane carried by its header or from the rail: what lights, what is asked, and what is not.

use gpui_kit::{px, TestAppContext};

use super::*;
use crate::kit::spots::Spots;

/// Presses `id` and moves the pointer to `to` with the button held: the drag is on.
fn lift(cx: &mut TestAppContext, opened: &Opened, id: &str, to: Point<Pixels>) {
    let at = centre(cx, opened, id);
    down(cx, opened, at);
    drag_to(cx, opened, to);
}

fn lit(cx: &mut TestAppContext) -> Option<u64> {
    cx.update(|cx| Spots::of(cx).lit())
}

#[gpui_kit::test]
fn a_header_let_go_on_another_cell_moves_the_pane_there_by_the_zone(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    let target = centre(cx, &opened, "cell-1");
    lift(cx, &opened, "cell-head-0", target);
    assert!(has(cx, &opened, "drag-ghost"), "the pane's name follows the pointer");
    assert_eq!(label(cx, &opened, "drag-ghost").as_deref(), Some("one"));
    assert_eq!(label(cx, &opened, "drop-zone").as_deref(), Some("Swap places"));
    up(cx, &opened, target);
    assert_eq!(asked(&opened), ["MovePane { pane: Pane(1), onto: Pane(2), zone: Centre }"]);
    assert!(!has(cx, &opened, "drag-ghost") && !has(cx, &opened, "drop-zone"), "both go with the drag");
}

#[gpui_kit::test]
fn near_the_edge_of_a_cell_the_zone_is_that_edge(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    let cell = bounds(cx, &opened, "cell-2");
    let near_top = point(cell.center().x, cell.origin.y + px(10.0));
    lift(cx, &opened, "cell-head-0", near_top);
    assert_eq!(label(cx, &opened, "drop-zone").as_deref(), Some("Place above"));
    up(cx, &opened, near_top);
    assert_eq!(asked(&opened), ["MovePane { pane: Pane(1), onto: Pane(3), zone: Top }"]);
}

#[gpui_kit::test]
fn a_header_let_go_on_its_own_cell_asks_for_nothing(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    let own = centre(cx, &opened, "cell-0");
    lift(cx, &opened, "cell-head-0", by(own, 0.0, 40.0));
    assert!(!has(cx, &opened, "drop-zone"), "a pane is not dropped on itself");
    up(cx, &opened, by(own, 0.0, 40.0));
    assert!(asked(&opened).is_empty());
}

#[gpui_kit::test]
fn escape_puts_a_carried_header_back_and_asks_for_nothing(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    let target = centre(cx, &opened, "cell-1");
    lift(cx, &opened, "cell-head-0", target);
    escape(cx, &opened);
    assert!(!has(cx, &opened, "drag-ghost"));
    up(cx, &opened, target);
    assert!(asked(&opened).is_empty());
}

#[gpui_kit::test]
fn a_press_that_stays_under_five_pixels_is_no_drag(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    let at = centre(cx, &opened, "cell-head-0");
    down(cx, &opened, at);
    drag_to(cx, &opened, by(at, 3.0, 3.0));
    assert!(!has(cx, &opened, "drag-ghost"));
    up(cx, &opened, by(at, 3.0, 3.0));
    assert!(asked(&opened).is_empty());
}

#[gpui_kit::test]
fn a_press_on_the_close_does_not_start_a_move(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    let close = centre(cx, &opened, "cell-close-1");
    down(cx, &opened, close);
    drag_to(cx, &opened, by(close, -150.0, 0.0));
    assert!(!has(cx, &opened, "drag-ghost"));
    up(cx, &opened, by(close, -150.0, 0.0));
    assert!(asked(&opened).is_empty(), "{:?}", asked(&opened));
}

#[gpui_kit::test]
fn the_cells_do_not_move_under_a_carried_pane_when_the_session_redraws(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    let target = centre(cx, &opened, "cell-1");
    lift(cx, &opened, "cell-head-0", target);
    let held = bounds(cx, &opened, "cell-0").size.width;
    let mut row = three();
    row.widths = vec![vec![250, 750], vec![1000]];
    hear(cx, &opened, vec![rail(row)]);
    assert_eq!(bounds(cx, &opened, "cell-0").size.width, held, "SHELL-GRID-17: busy while carried");
    escape(cx, &opened);
    up(cx, &opened, target);
    hear(cx, &opened, vec![]);
    assert!(bounds(cx, &opened, "cell-0").size.width < held * 0.6, "and the session's layout is drawn when it ends");
}

#[gpui_kit::test]
fn a_rail_pane_let_go_on_a_cell_is_dropped_there_by_the_zone(cx: &mut TestAppContext) {
    let opened = opened(cx, alone());
    let target = centre(cx, &opened, "cell-0");
    lift(cx, &opened, "pane-2", target);
    assert_eq!(label(cx, &opened, "drag-ghost").as_deref(), Some("two"));
    up(cx, &opened, target);
    assert_eq!(asked(&opened), ["DropPane { pane: 2, onto: Some(Pane(1)), zone: Centre }"]);
}

#[gpui_kit::test]
fn a_rail_pane_let_go_over_the_stage_with_no_cell_opens_there(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    let stage = bounds(cx, &opened, "split");
    // The stage's own margin, which no cell covers.
    let margin = point(stage.origin.x + px(1.0), stage.center().y);
    lift(cx, &opened, "pane-4", margin);
    assert_eq!(label(cx, &opened, "drop-stage").as_deref(), Some("Open on the stage"));
    up(cx, &opened, margin);
    assert_eq!(asked(&opened), ["DropPane { pane: 4, onto: None, zone: Centre }"]);
}

#[gpui_kit::test]
fn a_rail_pane_let_go_on_its_own_cell_asks_for_nothing(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    let own = centre(cx, &opened, "cell-0");
    lift(cx, &opened, "pane-1", own);
    up(cx, &opened, own);
    assert!(asked(&opened).is_empty(), "{:?}", asked(&opened));
}

#[gpui_kit::test]
fn a_click_on_a_rail_pane_is_still_its_click(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    cx.update_window(opened.handle, |_, window, cx| window.click("pane-2", cx)).unwrap();
    cx.run_until_parked();
    assert_eq!(asked(&opened), ["RailPick { checkout: 110, pane: Some(2) }", "Select { pane: 2 }"]);
}

#[gpui_kit::test]
fn a_drag_that_ends_on_a_rail_row_is_not_also_its_click(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    let row = centre(cx, &opened, "pane-3");
    lift(cx, &opened, "pane-2", by(row, 0.0, 6.0));
    up(cx, &opened, by(row, 0.0, 6.0));
    assert!(!asked(&opened).iter().any(|call| call.starts_with("Select")), "{:?}", asked(&opened));
}

#[gpui_kit::test]
fn a_rail_pane_let_go_on_another_checkouts_row_is_asked_to_move_there(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    let target = centre(cx, &opened, "checkout-112");
    lift(cx, &opened, "pane-2", target);
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    assert_eq!(lit(cx), Some(112), "the row is lit as a target");
    up(cx, &opened, target);
    assert_eq!(asked(&opened), ["MoveToCheckout { pane: 2, checkout: 112 }"]);
    assert_eq!(lit(cx), None, "and not after");
}

#[gpui_kit::test]
fn the_panes_own_checkout_is_no_target(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    let own = centre(cx, &opened, "checkout-110");
    lift(cx, &opened, "pane-2", own);
    assert_eq!(lit(cx), None);
    up(cx, &opened, own);
    assert!(asked(&opened).is_empty(), "{:?}", asked(&opened));
}

#[gpui_kit::test]
fn escape_on_a_rail_pane_over_a_row_lights_nothing_and_asks_nothing(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    let target = centre(cx, &opened, "checkout-112");
    lift(cx, &opened, "pane-2", target);
    escape(cx, &opened);
    assert_eq!(lit(cx), None);
    up(cx, &opened, target);
    assert!(asked(&opened).is_empty());
}

#[gpui_kit::test]
fn nothing_is_carried_while_the_daemon_is_gone(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    let status = Event::Status(Status { connection: Connection::Connecting, detail: None, alarm: None, chime: true, host: None });
    hear(cx, &opened, vec![status]);
    let target = centre(cx, &opened, "cell-1");
    lift(cx, &opened, "cell-head-0", target);
    assert!(!has(cx, &opened, "drag-ghost"));
    up(cx, &opened, target);
    assert!(asked(&opened).is_empty());
}
