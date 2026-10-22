//! The gutters: sizing the two panes either side of one, sent once on release, held until the
//! session answers, evened by a double-click.

use std::time::Duration;

use argus_desktop_core::connection::page::Call;
use gpui_kit::{px, TestAppContext};

use super::*;
use crate::layout::holders::Holder;
use crate::panes::split::gutters::HOLD;

/// The shares the grid sent, in thousandths.
fn sent(opened: &Opened) -> Vec<(Vec<u16>, Vec<Vec<u16>>)> {
    opened
        .calls
        .borrow()
        .iter()
        .filter_map(|call| match call {
            Call::SetShares { heights, widths } => Some((heights.clone(), widths.clone())),
            _ => None,
        })
        .collect()
}

fn width_of(cx: &mut TestAppContext, opened: &Opened, id: &str) -> Pixels {
    bounds(cx, opened, id).size.width
}

fn resizing(cx: &mut TestAppContext, opened: &Opened) -> bool {
    cx.update(|cx| opened.heard.read(cx).resizing())
}

#[gpui_kit::test]
fn a_vertical_gutter_sits_after_every_pane_but_the_last_of_a_row_and_a_horizontal_under_each_row_with_one_below(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    assert!(has(cx, &opened, "gutter-across-0-0"));
    assert!(!has(cx, &opened, "gutter-across-0-1"), "none after the last");
    assert!(!has(cx, &opened, "gutter-across-1-0"), "a row of one has no vertical gutter");
    assert!(has(cx, &opened, "gutter-down-0"));
    assert!(!has(cx, &opened, "gutter-down-1"), "none under the last row");
    let seam = bounds(cx, &opened, "gutter-across-0-0");
    let first = bounds(cx, &opened, "cell-0");
    assert!((seam.center().x - (first.origin.x + first.size.width)).abs() < px(1.0), "centred on the seam");
}

#[gpui_kit::test]
fn dragging_a_gutter_sizes_its_two_panes_and_sends_the_shares_once_on_release(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    let before = width_of(cx, &opened, "cell-0");
    let grip = centre(cx, &opened, "gutter-across-0-0");
    down(cx, &opened, grip);
    drag_to(cx, &opened, by(grip, 60.0, 0.0));
    drag_to(cx, &opened, by(grip, 100.0, 0.0));
    assert!(width_of(cx, &opened, "cell-0") > before + px(90.0), "the cells follow the pointer");
    assert!(sent(&opened).is_empty(), "nothing is sent while it is held");
    assert!(resizing(cx, &opened), "the panes wait for the size to settle");
    up(cx, &opened, by(grip, 100.0, 0.0));
    let shares = sent(&opened);
    assert_eq!(shares.len(), 1, "once, on release: {shares:?}");
    let (heights, widths) = &shares[0];
    assert_eq!(heights, &[500, 500], "the rows are untouched");
    assert_eq!(widths[1], [1000], "so is the other row");
    assert!(widths[0][0] > 500 && widths[0][0] + widths[0][1] == 1000, "{widths:?}");
    assert!(!resizing(cx, &opened), "and the panes report again");
}

#[gpui_kit::test]
fn a_horizontal_gutter_sizes_the_rows(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    let grip = centre(cx, &opened, "gutter-down-0");
    down(cx, &opened, grip);
    drag_to(cx, &opened, by(grip, 0.0, -60.0));
    up(cx, &opened, by(grip, 0.0, -60.0));
    let (heights, widths) = sent(&opened).remove(0);
    assert!(heights[0] < 500 && heights.iter().sum::<u16>() == 1000, "{heights:?}");
    assert_eq!(widths, [vec![500, 500], vec![1000]]);
}

#[gpui_kit::test]
fn a_press_that_stays_under_five_pixels_sends_nothing(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    let grip = centre(cx, &opened, "gutter-across-0-0");
    down(cx, &opened, grip);
    drag_to(cx, &opened, by(grip, 3.0, 0.0));
    assert!(!resizing(cx, &opened));
    up(cx, &opened, by(grip, 3.0, 0.0));
    assert!(sent(&opened).is_empty());
}

#[gpui_kit::test]
fn escape_puts_the_sizes_back_and_sends_nothing(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    let before = width_of(cx, &opened, "cell-0");
    let grip = centre(cx, &opened, "gutter-across-0-0");
    down(cx, &opened, grip);
    drag_to(cx, &opened, by(grip, 100.0, 0.0));
    escape(cx, &opened);
    up(cx, &opened, by(grip, 100.0, 0.0));
    assert_eq!(width_of(cx, &opened, "cell-0"), before);
    assert!(sent(&opened).is_empty());
    assert!(!resizing(cx, &opened));
}

#[gpui_kit::test]
fn a_gutter_dragged_a_long_way_stops_where_the_panes_least_size_is(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    let grip = centre(cx, &opened, "gutter-across-0-0");
    down(cx, &opened, grip);
    drag_to(cx, &opened, by(grip, 5000.0, 0.0));
    up(cx, &opened, by(grip, 5000.0, 0.0));
    let (_, widths) = sent(&opened).remove(0);
    assert!(widths[0][1] > 0, "the pane keeps a size: {widths:?}");
    assert!(widths[0][1] < 500, "and the other grew: {widths:?}");
    // The least is the cells and the room round them: well over a hundredth of the width.
    assert!(widths[0][1] >= 20, "{widths:?}");
}

#[gpui_kit::test]
fn the_sizes_let_go_are_drawn_until_the_session_answers_and_then_the_sessions_are(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    let before = width_of(cx, &opened, "cell-0");
    let grip = centre(cx, &opened, "gutter-across-0-0");
    down(cx, &opened, grip);
    drag_to(cx, &opened, by(grip, 100.0, 0.0));
    up(cx, &opened, by(grip, 100.0, 0.0));
    assert!(width_of(cx, &opened, "cell-0") > before + px(90.0), "held, so the cells do not jump back");
    // The session declined in silence: its layout is the truth.
    cx.executor().advance_clock(HOLD + Duration::from_millis(50));
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    assert_eq!(width_of(cx, &opened, "cell-0"), before, "SHELL-GRID-9: 600 ms");
}

#[gpui_kit::test]
fn sizes_the_session_echoes_are_the_sizes_drawn_from_then_on(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    let before = width_of(cx, &opened, "cell-0");
    let grip = centre(cx, &opened, "gutter-across-0-0");
    down(cx, &opened, grip);
    drag_to(cx, &opened, by(grip, 100.0, 0.0));
    up(cx, &opened, by(grip, 100.0, 0.0));
    let (heights, widths) = sent(&opened).remove(0);
    let mut row = three();
    row.heights = heights;
    row.widths = widths;
    hear(cx, &opened, vec![rail(row)]);
    // Once echoed the hold is over: what the session says next is drawn at once, not after the wait.
    let mut next = three();
    next.widths = vec![vec![250, 750], vec![1000]];
    hear(cx, &opened, vec![rail(next)]);
    assert!(width_of(cx, &opened, "cell-0") < before * 0.6, "the session's newer sizes are the ones drawn");
}

#[gpui_kit::test]
fn a_double_click_evens_the_row_of_a_vertical_gutter_and_the_rows_for_a_horizontal(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    cx.update_window(opened.handle, |_, window, cx| window.double_click("gutter-across-0-0", cx)).unwrap();
    cx.update_window(opened.handle, |_, window, cx| window.double_click("gutter-down-0", cx)).unwrap();
    cx.run_until_parked();
    let calls = asked(&opened);
    assert!(calls.contains(&"EvenShares { row: Some(0) }".to_owned()), "{calls:?}");
    assert!(calls.contains(&"EvenShares { row: None }".to_owned()), "{calls:?}");
    assert!(sent(&opened).is_empty(), "evening is the session's: no sizes are made up here");
}

#[gpui_kit::test]
fn no_gutter_is_dragged_while_the_daemon_is_gone(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    let status = Event::Status(Status { connection: Connection::Connecting, detail: None, alarm: None, chime: true, host: None });
    hear(cx, &opened, vec![status]);
    let grip = centre(cx, &opened, "gutter-across-0-0");
    down(cx, &opened, grip);
    drag_to(cx, &opened, by(grip, 100.0, 0.0));
    up(cx, &opened, by(grip, 100.0, 0.0));
    assert!(sent(&opened).is_empty());
}

#[gpui_kit::test]
fn the_grid_holds_a_resize_under_its_own_name(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    let grip = centre(cx, &opened, "gutter-across-0-0");
    down(cx, &opened, grip);
    drag_to(cx, &opened, by(grip, 100.0, 0.0));
    assert!(cx.update(|cx| opened.heard.update(cx, |heard, _| heard.release_resize(Holder::Grid))), "it was the grid that held it");
    up(cx, &opened, by(grip, 100.0, 0.0));
}
