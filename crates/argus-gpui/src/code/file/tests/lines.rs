//! The lines of the file, what stands where there are none, and where the file is scrolled
//! (CODE-35 to 39, 41).

use gpui_kit::TestAppContext;

use super::*;
use crate::checkouts::stage::tests::{click_id, label, present};

/// CODE-36: one row for each line, and a click on a line is the click on it, Shift extending.
#[gpui_kit::test]
fn each_line_is_a_row_and_a_click_on_it_sends_its_number(cx: &mut TestAppContext) {
    let opened = with(cx, reading(30, 1));
    settle(cx, &opened);
    assert!(present(cx, &opened, ("diff-row", 0usize)) && present(cx, &opened, ("diff-row", 5usize)));
    click_id(cx, &opened, ("diff-row", 4usize));
    assert_eq!(sent(&opened), ["Code { gesture: Click { target: Line { line: 5, extend: false } } }"]);
}

/// CODE-36: the cursor is the session's: a click changes nothing drawn until its next page.
#[gpui_kit::test]
fn a_click_moves_nothing_until_the_session_says_the_cursor_moved(cx: &mut TestAppContext) {
    let opened = with(cx, reading(30, 1));
    settle(cx, &opened);
    click_id(cx, &opened, ("diff-row", 4usize));
    assert_eq!(label(cx, &opened, "file-head-meta").as_deref(), Some("Ln 1 \u{b7} Rust \u{b7} 30 lines"), "still the cursor the session sent");
    says(cx, &opened, reading(30, 5));
    assert_eq!(label(cx, &opened, "file-head-meta").as_deref(), Some("Ln 5 \u{b7} Rust \u{b7} 30 lines"));
}

/// CODE-37: lines removed are a note above the line after them.
#[gpui_kit::test]
fn removed_lines_are_a_note_above_the_line_after_them_and_one_after_the_last(cx: &mut TestAppContext) {
    let mut page = reading(3, 1);
    page.lines[1].removed_before = 2;
    page.removed_at_end = 1;
    let opened = with(cx, page);
    settle(cx, &opened);
    // Rows: line 1, note, line 2, line 3, note.
    assert!(present(cx, &opened, ("diff-row", 4usize)) && !present(cx, &opened, ("diff-row", 5usize)));
    let note = bounds_of(cx, &opened, ("diff-row", 1usize));
    let first = bounds_of(cx, &opened, ("diff-row", 0usize));
    let second = bounds_of(cx, &opened, ("diff-row", 2usize));
    assert!(note.origin.y > first.origin.y && second.origin.y >= note.origin.y + note.size.height, "the note is between the two lines");
    click_id(cx, &opened, ("diff-row", 1usize));
    assert_eq!(sent(&opened), Vec::<String>::new(), "a note is not clicked");
}

/// CODE-35: a failure with no lines, a note and a read still out each stand in the lines' place.
#[gpui_kit::test]
fn a_failure_a_note_and_a_pending_read_stand_where_the_lines_would(cx: &mut TestAppContext) {
    let none = CodePage { lines: Vec::new(), ..reading(0, 0) };
    let opened = with(cx, CodePage { failed: Some("no such file".into()), ..none.clone() });
    assert_eq!(label(cx, &opened, "code-failed").as_deref(), Some("no such file"));
    says(cx, &opened, CodePage { note: Some("binary file".into()), ..none.clone() });
    assert_eq!(label(cx, &opened, "code-note").as_deref(), Some("src/lib.rs: binary file"));
    assert!(!present(cx, &opened, "code-failed"));
    says(cx, &opened, CodePage { file_loading: true, ..none });
    assert!(present(cx, &opened, "code-loading-file") && !present(cx, &opened, "code-note"));
    says(cx, &opened, reading(3, 1));
    assert!(present(cx, &opened, ("diff-row", 0usize)) && !present(cx, &opened, "code-loading-file"));
}

fn band_top(cx: &mut TestAppContext, opened: &Opened) -> f32 {
    bounds_of(cx, opened, "code-view").origin.y.as_f32() - bounds_of(cx, opened, "code-ruler").origin.y.as_f32()
}

/// CODE-41: a cursor that moves off screen is brought a third of the way down, and one already in
/// view leaves the file where it is.
#[gpui_kit::test]
fn a_moved_cursor_is_brought_into_view_only_when_it_is_off_screen(cx: &mut TestAppContext) {
    let opened = with(cx, reading(400, 1));
    settle(cx, &opened);
    assert_eq!(band_top(cx, &opened), 0.0);
    says(cx, &opened, reading(400, 3));
    settle(cx, &opened);
    assert_eq!(band_top(cx, &opened), 0.0, "line 3 is on screen: nothing moves");
    says(cx, &opened, reading(400, 200));
    settle(cx, &opened);
    assert!(band_top(cx, &opened) > 0.0, "line 200 is far below");
    // The cursor's row is drawn, a third of the way down the pane.
    let (pane, row) = (bounds_of(cx, &opened, "code-lines-pane"), bounds_of(cx, &opened, ("diff-row", 199usize)));
    let third = (row.origin.y - pane.origin.y).as_f32() / pane.size.height.as_f32();
    assert!((0.2..0.45).contains(&third), "line 200 stands {third} of the way down");
}

/// CODE-41: a redraw that moved no cursor leaves the scroll alone, however far it was scrolled.
#[gpui_kit::test]
fn a_redraw_with_the_cursor_where_it_was_leaves_the_scroll(cx: &mut TestAppContext) {
    let opened = with(cx, reading(400, 1));
    settle(cx, &opened);
    says(cx, &opened, reading(400, 200));
    settle(cx, &opened);
    let was = band_top(cx, &opened);
    let mut again = reading(400, 200);
    again.lines[10].added = true;
    says(cx, &opened, again);
    settle(cx, &opened);
    assert_eq!(band_top(cx, &opened), was);
}

/// CODE-41: each file is scrolled where it was left, by path: a few lines past where the cursor put it, with the cursor still on screen.
#[gpui_kit::test]
fn each_file_comes_back_scrolled_where_it_was_left(cx: &mut TestAppContext) {
    let opened = with(cx, reading(400, 1));
    settle(cx, &opened);
    says(cx, &opened, reading(400, 200));
    settle(cx, &opened);
    let placed = band_top(cx, &opened);
    let pane = bounds_of(cx, &opened, "code-lines-pane");
    cx.update_window(opened.handle, |_, window, cx| {
        let wheel = gpui_kit::ScrollWheelEvent {
            position: pane.center(),
            delta: gpui_kit::ScrollDelta::Pixels(gpui_kit::point(gpui_kit::px(0.0), gpui_kit::px(-60.0))),
            ..Default::default()
        };
        window.dispatch_event(gpui_kit::InputEvent::to_platform_input(wheel), cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(cx, &opened);
    let there = band_top(cx, &opened);
    assert!(there > placed, "the wheel scrolled the file on from where the cursor put it");
    let other = CodePage { path: Some("src/other.rs".into()), ..reading(400, 1) };
    says(cx, &opened, other);
    settle(cx, &opened);
    assert_eq!(band_top(cx, &opened), 0.0, "another file starts at the top");
    says(cx, &opened, reading(400, 200));
    settle(cx, &opened);
    assert!((band_top(cx, &opened) - there).abs() < 0.5, "back where it was left, not where the cursor would put it");
}
