//! The diff view as the Code stage draws a file read whole: one number column, the places lines
//! were removed in red, no scrolling of its own to the cursor, and where each row stands.

use argus_desktop_core::code::file::rows as file_rows;
use argus_desktop_core::code::stage::{CodePage, CodeRow};
use gpui_kit::{px, TestAppContext};

use super::*;

fn file(lines: u32, removed_before: &[(u32, u32)], cursor: u32) -> Vec<DiffRow> {
    let lines = (1..=lines)
        .map(|no| CodeRow {
            no,
            parts: vec![(format!("line {no}"), "")],
            added: false,
            removed_before: removed_before.iter().find(|(at, _)| *at == no).map_or(0, |(_, n)| *n),
            cursor: no == cursor,
            picked: false,
        })
        .collect();
    file_rows(&CodePage { lines, ..CodePage::default() })
}

/// CODE-36: a line is one number and a marker before its code, in the wider column; a review's has two.
#[gpui_kit::test]
fn a_line_of_a_file_has_one_number_column_and_a_diffs_has_two(cx: &mut TestAppContext) {
    let reading = open_reading(cx);
    show(cx, &reading, &file(3, &[], 0));
    let one = cx.update_window(reading.handle, |_, window, _| window.find(("diff-row", 0usize)).bounds().size.width).unwrap();
    assert_eq!(one, px(600.0), "the row fills the pane");
    let widest = cx.update(|cx| reading.view.read(cx).numbers_w);
    assert_eq!(widest, 52.0, "`.cline`'s 52px column, not the diff's two of 44");
    let diff = open(cx, true);
    assert_eq!(cx.update(|cx| diff.view.read(cx).numbers_w), NUMBER * 2.0);
}

/// CODE-37: a removal note is a line and a rule tall, not a hunk's.
#[gpui_kit::test]
fn a_removal_note_is_its_own_height_in_a_file_and_a_hunks_in_a_diff(cx: &mut TestAppContext) {
    let rows = file(3, &[(2, 4)], 0);
    let reading = open_reading(cx);
    settle(cx, &reading, &rows, false);
    assert_eq!(cx.update(|cx| reading.view.read(cx).heights.clone()), [shape::LINE, shape::GONE, shape::LINE, shape::LINE]);
    assert_eq!(rows[1].text, "\u{2212} 4 lines removed here");
    let diff = open(cx, true);
    settle(cx, &diff, &rows, false);
    assert_eq!(cx.update(|cx| diff.view.read(cx).heights[1]), shape::HUNK, "a diff says a note as a heading");
}

/// The cursor's row is brought into view by the owner of a file, not by the view: a new set of rows
/// leaves the scroll where it was.
#[gpui_kit::test]
fn a_file_view_does_not_scroll_itself_to_the_cursor(cx: &mut TestAppContext) {
    let reading = open_reading(cx);
    settle(cx, &reading, &file(400, &[], 300), false);
    let scroll = cx.update(|cx| reading.view.read(cx).scroll().clone());
    assert_eq!(scroll.offset().y, px(0.0), "the cursor is 300 lines down and the view stays at the top");
    let diff = open(cx, true);
    let mut rows = file(400, &[], 300);
    for row in &mut rows {
        row.left.as_mut().unwrap().old = row.left.as_ref().unwrap().new;
    }
    settle(cx, &diff, &rows, false);
    let scroll = cx.update(|cx| diff.view.read(cx).scroll().clone());
    assert_ne!(scroll.offset().y, px(0.0), "a diff brings its cursor into view");
}

/// What a ruler is placed from: each line's top and bottom among the rows, the markers included, and
/// how far the list is scrolled.
#[gpui_kit::test]
fn rows_are_found_by_number_among_the_markers_and_the_scroll_is_set_in_pixels(cx: &mut TestAppContext) {
    let reading = open_reading(cx);
    settle(cx, &reading, &file(100, &[(3, 2)], 0), false);
    let (span, total, first) = cx.update(|cx| {
        let view = reading.view.read(cx);
        (view.rows_span(3, 4), view.content_px(), view.rows_span(1, 1))
    });
    assert_eq!(first, Some((0.0, shape::LINE)));
    // Lines 1 and 2, then the note, then line 3 and 4 together.
    assert_eq!(span, Some((2.0 * shape::LINE + shape::GONE, 4.0 * shape::LINE + shape::GONE)));
    assert_eq!(total, 100.0 * shape::LINE + shape::GONE);
    assert_eq!(cx.update(|cx| reading.view.read(cx).rows_span(101, 101)), None);
    cx.update(|cx| reading.view.read(cx).scroll_to_px(240.0));
    assert_eq!(cx.update(|cx| reading.view.read(cx).scrolled_px()), 240.0);
}
