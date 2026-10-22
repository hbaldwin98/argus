//! The change marks, and stepping between changes (EDITOR-GUTTER).

use gpui_kit::component::input::RopeExt as _;

use super::*;

/// A page of `lines` lines in which lines `added` were added and `removed` have lines removed before.
fn changed(lines: u32, added: &[u32], removed: &[u32]) -> CodePage {
    let mut page = editing(lines, 1);
    for row in &mut page.lines {
        row.added = added.contains(&row.no);
        row.removed_before = if removed.contains(&row.no) { 2 } else { 0 };
    }
    page
}

fn text_of(lines: u32) -> String {
    (1..=lines).map(|n| format!("line {n}\n")).collect()
}

/// EDITOR-GUTTER-1, -9: an added line and a line after removed lines are marked, and a line that
/// is both has only the added mark.
#[gpui_kit::test]
fn changed_lines_are_marked_and_a_line_that_is_both_has_only_the_added_mark(cx: &mut TestAppContext) {
    let opened = with(cx, changed(10, &[2, 3, 7], &[5, 7]));
    hear(cx, &opened, vec![opened_with("src/lib.rs", &text_of(10))]);
    settle(cx, &opened);
    assert_eq!(marked(cx, true), [2, 3, 7]);
    assert_eq!(marked(cx, false), [5]);
}

/// EDITOR-GUTTER-2: the marks move with what is typed until Rust reads the file again.
#[gpui_kit::test]
fn marks_move_with_the_text_typed_above_them(cx: &mut TestAppContext) {
    let opened = with(cx, changed(10, &[4], &[]));
    hear(cx, &opened, vec![opened_with("src/lib.rs", &text_of(10))]);
    settle(cx, &opened);
    sent(&opened);
    let state = state(cx);
    cx.update_window(opened.handle, |_, window, cx| {
        state.update(cx, |state, cx| {
            state.set_selected_range(0..0, cx);
            state.insert("new\n", window, cx);
        })
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(marked(cx, true), [5], "the line added is a line lower");
}

/// EDITOR-GUTTER-4, -8: stepping goes to the first line of the next run, and back from inside a run
/// goes to that run's first line; with nothing in that direction nothing happens.
#[gpui_kit::test]
fn stepping_goes_to_the_first_line_of_the_next_run_and_back_to_the_runs_first_line(cx: &mut TestAppContext) {
    let opened = with(cx, changed(30, &[4, 5, 6, 12, 13, 20], &[]));
    hear(cx, &opened, vec![opened_with("src/lib.rs", &text_of(30))]);
    settle(cx, &opened);
    let at = |cx: &mut TestAppContext| {
        let (state, editor) = (state(cx), editor());
        cx.update(|cx| {
            let _ = &editor;
            let state = state.read(cx);
            state.text().offset_to_point(state.cursor()).row + 1
        })
    };
    let step = |cx: &mut TestAppContext, forward: bool| {
        let editor = editor();
        cx.update(|cx| editor.update(cx, |editor, cx| editor.step(forward, cx)))
    };
    assert!(step(cx, true));
    assert_eq!(at(cx), 4);
    assert!(step(cx, true) && at(cx) == 12);
    assert!(step(cx, true) && at(cx) == 20);
    assert!(!step(cx, true), "nothing after the last change");
    assert_eq!(at(cx), 20);
    assert!(step(cx, false) && at(cx) == 12);
    // From the middle of a run, back is that run's first line.
    let state = state(cx);
    cx.update(|cx| {
        state.update(cx, |state, cx| {
            let into = state.text().line_start_offset(5);
            state.set_selected_range(into..into, cx);
        })
    });
    assert!(step(cx, false) && at(cx) == 4);
}

/// The bars have a gutter of their own, as the diff view's do (the person asked for the Code view to
/// match Git's): the text stops a bar's width short of the editor's right edge and its foot.
#[gpui_kit::test]
fn the_text_stops_short_of_the_scrollbars(cx: &mut TestAppContext) {
    let opened = with(cx, changed(200, &[], &[]));
    hear(cx, &opened, vec![opened_with("src/lib.rs", &text_of(200))]);
    settle(cx, &opened);
    let outer = crate::code::file::tests::bounds_of(cx, &opened, "code-editor");
    let state = state(cx);
    let text = cx.update(|cx| state.read(cx).input_bounds());
    let bar = gpui_kit::component::scroll::Scrollbar::width();
    assert!(text.right() <= outer.right() - bar, "the text reaches {:?}, the bar starts at {:?}", text.right(), outer.right() - bar);
    assert!(text.bottom() <= outer.bottom() - bar, "the text reaches {:?}, the bar starts at {:?}", text.bottom(), outer.bottom() - bar);
}
