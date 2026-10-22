//! Which text a drag selects, and what copies it (cross-cutting Decided 4).

use gpui_kit::test::TestEventExt as _;
use gpui_kit::{point, px, TestAppContext};

use super::*;

#[gpui_kit::test]
fn a_drag_inside_a_paragraph_selects_the_text_it_covers(cx: &mut TestAppContext) {
    let opened = open(cx, "alpha beta gamma");
    let (from, to) = (at(cx, &opened, 0, 6), at(cx, &opened, 0, 10));
    drag(cx, &opened, from, to);
    assert_eq!(selected(cx, &opened).as_deref(), Some("beta"));
    // And back the other way: the same text.
    drag(cx, &opened, to, from);
    assert_eq!(selected(cx, &opened).as_deref(), Some("beta"));
}

#[gpui_kit::test]
fn a_drag_across_blocks_selects_the_ends_of_the_first_and_last_and_all_between(cx: &mut TestAppContext) {
    let opened = open(cx, "first line\n\n- item one\n- item two\n\nlast para");
    let (from, to) = (at(cx, &opened, 0, 6), at(cx, &opened, 3, 4));
    drag(cx, &opened, from, to);
    assert_eq!(selected(cx, &opened).as_deref(), Some("line\nitem one\nitem two\nlast"));
}

#[gpui_kit::test]
fn a_drag_across_a_table_copies_its_cells_with_tabs_between_a_row(cx: &mut TestAppContext) {
    let opened = open(cx, "| name | n |\n|---|---|\n| ada | 1 |\n| bob | 2 |\n");
    let (from, to) = (at(cx, &opened, 0, 0), at(cx, &opened, 5, 1));
    drag(cx, &opened, from, to);
    assert_eq!(selected(cx, &opened).as_deref(), Some("name\tn\nada\t1\nbob\t2"));
}

#[gpui_kit::test]
fn a_drag_that_ends_below_the_text_selects_to_its_end_and_one_that_starts_above_from_its_start(cx: &mut TestAppContext) {
    let opened = open(cx, "one\n\ntwo");
    let (from, below) = (at(cx, &opened, 0, 1), point(px(300.0), px(450.0)));
    drag(cx, &opened, from, below);
    assert_eq!(selected(cx, &opened).as_deref(), Some("ne\ntwo"));
    let above = point(px(300.0), px(-20.0));
    let start = at(cx, &opened, 1, 2);
    drag(cx, &opened, start, above);
    assert_eq!(selected(cx, &opened).as_deref(), Some("one\ntw"), "past the top, from the start of the first text");
}

#[gpui_kit::test]
fn a_press_that_moves_under_five_pixels_selects_nothing(cx: &mut TestAppContext) {
    let opened = open(cx, "alpha beta");
    let from = at(cx, &opened, 0, 2);
    drag(cx, &opened, from, point(from.x + px(3.0), from.y));
    assert_eq!(selected(cx, &opened), None);
    assert!(opened.calls.borrow().is_empty(), "a click on plain text asks for nothing");
}

#[gpui_kit::test]
fn a_click_puts_a_selection_away_and_so_does_a_press_on_no_text(cx: &mut TestAppContext) {
    let opened = open(cx, "alpha beta\n\nsecond");
    let (from, to) = (at(cx, &opened, 0, 0), at(cx, &opened, 0, 5));
    drag(cx, &opened, from, to);
    assert!(selected(cx, &opened).is_some());
    click_in(cx, &opened, 1, 1);
    assert_eq!(selected(cx, &opened), None);
    drag(cx, &opened, from, to);
    // In the gap the blocks are spaced by: inside the document, on no text.
    let gap = cx.update(|cx| {
        let layouts = opened.view.read(cx).layouts.borrow();
        let (first, second) = (layouts[&0].bounds(), layouts[&1].bounds());
        point(first.left() + px(2.0), (first.bottom() + second.top()) / 2.0)
    });
    click(cx, &opened, gap);
    assert_eq!(selected(cx, &opened), None, "a press between two blocks");
}

#[gpui_kit::test]
fn escape_during_a_drag_puts_the_selection_away(cx: &mut TestAppContext) {
    let opened = open(cx, "alpha beta gamma");
    let (from, to) = (at(cx, &opened, 0, 0), at(cx, &opened, 0, 5));
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(from, gpui_kit::MouseButton::Left, cx);
        window.pointer_move(to, Some(gpui_kit::MouseButton::Left), cx);
    })
    .unwrap();
    settle(cx, &opened);
    assert_eq!(selected(cx, &opened).as_deref(), Some("alpha"), "selected while the button is down");
    cx.update_window(opened.handle, |_, window, cx| window.key_down("escape", false, cx)).unwrap();
    cx.update_window(opened.handle, |_, window, cx| window.pointer_up(to, gpui_kit::MouseButton::Left, cx)).unwrap();
    settle(cx, &opened);
    assert_eq!(selected(cx, &opened), None);
}

#[gpui_kit::test]
fn copying_asks_rust_for_exactly_the_selection_and_with_none_asks_nothing(cx: &mut TestAppContext) {
    let opened = open(cx, "alpha **beta** gamma");
    assert!(!cx.update(|cx| opened.view.read(cx).copy_selection()));
    assert!(opened.calls.borrow().is_empty());
    let (from, to) = (at(cx, &opened, 0, 3), at(cx, &opened, 0, 9));
    drag(cx, &opened, from, to);
    assert!(cx.update(|cx| opened.view.read(cx).copy_selection()));
    let calls = opened.calls.borrow();
    assert!(matches!(&calls[..], [Call::CopyText { text }] if text == "ha bet"), "the words as drawn, marks gone: {calls:?}");
}

#[gpui_kit::test]
fn the_selection_is_the_leaves_ranges_and_nothing_outside_it(cx: &mut TestAppContext) {
    let opened = open(cx, "one\n\ntwo\n\nthree");
    let (from, to) = (at(cx, &opened, 0, 1), at(cx, &opened, 2, 2));
    drag(cx, &opened, from, to);
    let ranges: Vec<_> = cx.update(|cx| (0..3).map(|leaf| opened.view.read(cx).selected_in(leaf)).collect());
    assert_eq!(ranges, [Some(1..3), Some(0..3), Some(0..2)]);
}
