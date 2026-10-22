//! Which text a drag selects and what copies.

use gpui_kit::{point, px, Pixels, TestAppContext};

use super::*;

/// The point `columns` cells into row `ix`'s code, at its middle.
fn in_code(cx: &mut TestAppContext, opened: &Opened, ix: usize, columns: f32) -> gpui_kit::Point<Pixels> {
    let advance = cx.update(|cx| opened.view.read(cx).advance);
    let bounds = cx.update_window(opened.handle, |_, window, _| window.find(("diff-row", ix)).bounds()).unwrap();
    // The border, both number columns and the marker come first.
    point(bounds.origin.x + px(NUMBER * 2.0 + MARKER + 1.0 + 1.0) + advance * columns, bounds.center().y)
}

fn dragged(cx: &mut TestAppContext, opened: &Opened, from: gpui_kit::Point<Pixels>, to: gpui_kit::Point<Pixels>) {
    cx.update_window(opened.handle, |_, window, cx| window.drag(from, to, cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

fn copied(cx: &mut TestAppContext, opened: &Opened) -> (bool, Option<String>) {
    cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(String::new()));
    let had = cx.update(|cx| opened.view.update(cx, |view, cx| view.copy(cx)));
    (had, cx.read_from_clipboard().and_then(|item| item.text()))
}

#[gpui_kit::test]
fn a_drag_across_three_rows_selects_their_text_and_copy_puts_it_on_the_clipboard(cx: &mut TestAppContext) {
    let opened = open_picking(cx, true, true);
    show(cx, &opened, &[text_line(0, "alpha"), text_line(1, "beta"), text_line(2, "gamma delta")]);
    let (from, to) = (in_code(cx, &opened, 0, 0.0), in_code(cx, &opened, 2, 5.2));
    dragged(cx, &opened, from, to);
    assert_eq!(copied(cx, &opened), (true, Some("alpha
beta
gamma".to_owned())));
    // From the middle of a row to the middle of another.
    let (from, to) = (in_code(cx, &opened, 0, 2.2), in_code(cx, &opened, 2, 2.2));
    dragged(cx, &opened, from, to);
    assert_eq!(copied(cx, &opened), (true, Some("pha
beta
ga".to_owned())));
    // And upward, the same text.
    dragged(cx, &opened, to, from);
    assert_eq!(copied(cx, &opened), (true, Some("pha
beta
ga".to_owned())));
}

#[gpui_kit::test]
fn a_press_that_moves_under_five_pixels_is_the_rows_click_and_selects_nothing(cx: &mut TestAppContext) {
    let opened = open_picking(cx, true, true);
    show(cx, &opened, &[text_line(0, "alpha"), text_line(1, "beta")]);
    let from = in_code(cx, &opened, 0, 1.0);
    dragged(cx, &opened, from, point(from.x + px(3.0), from.y));
    assert_eq!(*opened.pressed.borrow(), [DiffEvent::Row { row: 0, extend: false }]);
    assert_eq!(copied(cx, &opened), (false, None));
}

#[gpui_kit::test]
fn a_drag_across_rows_in_a_view_that_picks_tells_its_first_and_last_row_and_a_click_is_not_sent_too(cx: &mut TestAppContext) {
    let opened = open_picking(cx, true, true);
    show(cx, &opened, &[text_line(0, "alpha"), text_line(1, "beta"), text_line(2, "gamma")]);
    let (from, to) = (in_code(cx, &opened, 0, 1.0), in_code(cx, &opened, 2, 1.0));
    dragged(cx, &opened, from, to);
    assert_eq!(*opened.pressed.borrow(), [DiffEvent::Range { first: 0, last: 2 }]);
    opened.pressed.borrow_mut().clear();
    // Within one row: text is selected, nothing is picked.
    let (from, to) = (in_code(cx, &opened, 1, 0.0), in_code(cx, &opened, 1, 3.0));
    dragged(cx, &opened, from, to);
    assert!(opened.pressed.borrow().is_empty(), "{:?}", opened.pressed.borrow());
    assert_eq!(copied(cx, &opened), (true, Some("bet".to_owned())));
}

#[gpui_kit::test]
fn a_view_that_does_not_pick_tells_nothing_of_a_drag_but_still_selects(cx: &mut TestAppContext) {
    let opened = open_picking(cx, false, false);
    show(cx, &opened, &[text_line(0, "alpha"), text_line(1, "beta")]);
    let (from, to) = (in_code(cx, &opened, 0, 0.0), in_code(cx, &opened, 1, 9.0));
    dragged(cx, &opened, from, to);
    assert!(opened.pressed.borrow().is_empty());
    assert_eq!(copied(cx, &opened), (true, Some("alpha
beta".to_owned())));
    // New rows clear it.
    show(cx, &opened, &[text_line(0, "other")]);
    assert!(!copied(cx, &opened).0);
}

#[gpui_kit::test]
fn a_split_pair_selects_within_the_half_the_press_began_in(cx: &mut TestAppContext) {
    let opened = open_picking(cx, true, true);
    settle(cx, &opened, &[pair(0, "left one", "right one"), pair(1, "left two", "right two")], true);
    let bounds = cx.update_window(opened.handle, |_, window, _| (window.find(("diff-row", 0usize)).bounds(), window.find(("diff-row", 1usize)).bounds())).unwrap();
    let start = point(bounds.0.origin.x + px(NUMBER + MARKER + 2.0), bounds.0.center().y);
    // Dragged out over the right half: still the left half's text.
    let end = point(bounds.1.right() - px(4.0), bounds.1.center().y);
    dragged(cx, &opened, start, end);
    assert_eq!(copied(cx, &opened), (true, Some("left one
left two".to_owned())));
}
