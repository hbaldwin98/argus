//! The change ruler (CODE-43, 44).

use gpui_kit::test::TestEventExt as _;
use gpui_kit::{point, px, MouseButton, TestAppContext};

use super::*;
use crate::checkouts::stage::tests::{click_id, label, present};

fn marked(lines: u32, marks: Vec<CodeMark>) -> CodePage {
    CodePage { ruler: marks, ..reading(lines, 1) }
}

/// CODE-43: a mark for each change, titled by what it is, and a press on one goes to its line.
#[gpui_kit::test]
fn each_mark_is_a_button_that_goes_to_its_line(cx: &mut TestAppContext) {
    let opened = with(cx, marked(400, vec![mark(100, 3, "added"), mark(250, 1, "removed"), mark(300, 1, "hit")]));
    settle(cx, &opened);
    assert_eq!(label(cx, &opened, ("code-mark", 0usize)).as_deref(), Some("A change at line 100"));
    assert_eq!(label(cx, &opened, ("code-mark", 2usize)).as_deref(), Some("A match at line 300"));
    click_id(cx, &opened, ("code-mark", 1usize));
    assert_eq!(sent(&opened), ["Code { gesture: Click { target: Go { line: 250 } } }"]);
}

/// CODE-43: a mark stands where its rows are in the whole file, and is as tall as the run is long.
#[gpui_kit::test]
fn a_mark_stands_where_its_rows_are_in_the_whole_file(cx: &mut TestAppContext) {
    let opened = with(cx, marked(400, vec![mark(101, 40, "added"), mark(201, 1, "added")]));
    settle(cx, &opened);
    let track = bounds_of(cx, &opened, "code-ruler");
    let (run, one) = (bounds_of(cx, &opened, ("code-mark", 0usize)), bounds_of(cx, &opened, ("code-mark", 1usize)));
    let fraction = |at: gpui_kit::Pixels| (at - track.origin.y).as_f32() / track.size.height.as_f32();
    assert!((fraction(run.origin.y) - 100.0 / 400.0).abs() < 0.01, "line 101 is a quarter of the way down");
    assert!((run.size.height.as_f32() / track.size.height.as_f32() - 40.0 / 400.0).abs() < 0.01, "forty lines are a tenth");
    assert!((fraction(one.origin.y) - 200.0 / 400.0).abs() < 0.01);
    assert!(one.size.height.as_f32() >= 3.0, "never thinner than three pixels");
}

/// CODE-43: a place lines were removed above a mark moves it with the rows, not with the line count.
#[gpui_kit::test]
fn a_removal_marker_between_lines_moves_the_marks_below_it(cx: &mut TestAppContext) {
    let opened = with(cx, marked(100, vec![mark(60, 1, "added")]));
    settle(cx, &opened);
    let before = bounds_of(cx, &opened, ("code-mark", 0usize)).origin.y - bounds_of(cx, &opened, "code-ruler").origin.y;
    let mut page = marked(100, vec![mark(60, 1, "added")]);
    for at in 10..20 {
        page.lines[at].removed_before = 1;
    }
    says(cx, &opened, page);
    settle(cx, &opened);
    let after = bounds_of(cx, &opened, ("code-mark", 0usize)).origin.y - bounds_of(cx, &opened, "code-ruler").origin.y;
    assert!(after > before, "ten markers above are rows too: {before:?} then {after:?}");
}

/// CODE-43: the band is the part of the file on screen, and follows the scroll.
#[gpui_kit::test]
fn the_band_is_the_part_of_the_file_on_screen(cx: &mut TestAppContext) {
    let opened = with(cx, marked(400, Vec::new()));
    settle(cx, &opened);
    let (track, band) = (bounds_of(cx, &opened, "code-ruler"), bounds_of(cx, &opened, "code-view"));
    assert_eq!(band.origin.y, track.origin.y, "the top of the file is on screen");
    let share = band.size.height.as_f32() / track.size.height.as_f32();
    assert!(share > 0.02 && share < 0.5, "a screenful of four hundred lines is {share} of it");
    says(cx, &opened, CodePage { ruler: Vec::new(), ..reading(400, 399) });
    settle(cx, &opened);
    let band = bounds_of(cx, &opened, "code-view");
    assert!((band.origin.y + band.size.height - track.origin.y - track.size.height).abs() < px(2.0), "the end of the file is on screen");
}

/// CODE-44: a press elsewhere on the track centres the band there; dragging scrolls the file, and a
/// press on a mark is its own click, not a grab.
#[gpui_kit::test]
fn a_press_on_the_track_centres_the_band_and_a_drag_scrolls_the_file(cx: &mut TestAppContext) {
    let opened = with(cx, marked(400, vec![mark(10, 1, "added")]));
    settle(cx, &opened);
    let track = bounds_of(cx, &opened, "code-ruler");
    let at = point(track.center().x, track.origin.y + track.size.height * 0.6);
    cx.update_window(opened.handle, |_, window, cx| window.pointer_down(at, MouseButton::Left, cx)).unwrap();
    settle(cx, &opened);
    let band = bounds_of(cx, &opened, "code-view");
    let middle = (band.origin.y + band.size.height / 2.0 - at.y).abs();
    assert!(middle < px(3.0), "the band is centred on the pointer: {middle:?} off");
    let was = band.origin.y;
    let to = point(at.x, at.y - track.size.height * 0.3);
    cx.update_window(opened.handle, |_, window, cx| window.pointer_move(to, Some(MouseButton::Left), cx)).unwrap();
    settle(cx, &opened);
    let moved = bounds_of(cx, &opened, "code-view").origin.y;
    assert!(moved < was - track.size.height * 0.2, "dragging up scrolled the file up: {was:?} to {moved:?}");
    cx.update_window(opened.handle, |_, window, cx| window.pointer_up(to, MouseButton::Left, cx)).unwrap();
    settle(cx, &opened);
    assert_eq!(sent(&opened), Vec::<String>::new(), "scrolling sends nothing: the file's scroll is the window's");
}

/// CODE-44: a drag that goes under five pixels is no drag; a press on the band holds it where it is.
#[gpui_kit::test]
fn a_press_on_the_band_holds_it_where_it_is(cx: &mut TestAppContext) {
    let opened = with(cx, marked(400, Vec::new()));
    settle(cx, &opened);
    let band = bounds_of(cx, &opened, "code-view");
    let at = point(band.center().x, band.origin.y + band.size.height * 0.5);
    cx.update_window(opened.handle, |_, window, cx| window.pointer_down(at, MouseButton::Left, cx)).unwrap();
    cx.update_window(opened.handle, |_, window, cx| window.pointer_move(point(at.x, at.y + px(2.0)), Some(MouseButton::Left), cx)).unwrap();
    settle(cx, &opened);
    assert_eq!(bounds_of(cx, &opened, "code-view").origin.y, band.origin.y, "two pixels is not a drag");
    cx.update_window(opened.handle, |_, window, cx| window.pointer_up(at, MouseButton::Left, cx)).unwrap();
}

/// CODE-43: the ruler is drawn only for a file with lines.
#[gpui_kit::test]
fn a_file_with_no_lines_has_no_ruler(cx: &mut TestAppContext) {
    let opened = with(cx, marked(10, Vec::new()));
    assert!(present(cx, &opened, "code-ruler"));
    says(cx, &opened, CodePage { note: Some("binary file".into()), lines: Vec::new(), ..reading(0, 0) });
    assert!(!present(cx, &opened, "code-ruler"));
}
