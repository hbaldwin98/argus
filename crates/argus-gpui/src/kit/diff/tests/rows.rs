//! What the rows draw and report.

use super::*;

#[gpui_kit::test]
fn a_diff_with_no_clicks_says_its_folds_without_buttons_and_reports_no_press(cx: &mut TestAppContext) {
    let opened = open(cx, false);
    show(cx, &opened, &[line(0), fold(1), line(2)]);
    cx.update_window(opened.handle, |_, window, cx| {
        assert!(window.try_find(("diff-row", 1usize)).is_some(), "the fold is still said");
        window.click(("diff-row", 0usize), cx);
        window.click(("diff-row", 1usize), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(opened.pressed.borrow().is_empty(), "{:?}", opened.pressed.borrow());
    cx.update_window(opened.handle, |_, window, _| {
        assert!(window.try_find(("diff-fold", 1usize)).is_none(), "a view that takes no click offers no button");
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_diff_that_takes_clicks_reports_a_line_a_fold_and_a_shifted_line(cx: &mut TestAppContext) {
    let opened = open(cx, true);
    show(cx, &opened, &[line(0), fold(1), line(2)]);
    cx.update_window(opened.handle, |_, window, cx| {
        window.click(("diff-row", 0usize), cx);
        window.click(("diff-fold", 1usize), cx);
        window.click_with_modifiers(("diff-row", 2usize), gpui_kit::Modifiers::shift(), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(
        *opened.pressed.borrow(),
        [
            DiffEvent::Row { row: 0, extend: false },
            DiffEvent::Fold { row: 1, all: true },
            DiffEvent::Row { row: 2, extend: true },
        ]
    );
}

#[gpui_kit::test]
fn a_long_diff_builds_only_the_rows_the_list_asks_for(cx: &mut TestAppContext) {
    let opened = open(cx, true);
    let rows: Vec<DiffRow> = (0..20_000).map(line).collect();
    show(cx, &opened, &rows);
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.update(|cx| {
        let view = opened.view.read(cx);
        assert_eq!(view.len(), 20_000);
        assert!(view.built() > 0 && view.built() < 300, "built {} items of 20000", view.built());
    });
}

#[gpui_kit::test]
fn showing_what_is_drawn_already_redraws_nothing(cx: &mut TestAppContext) {
    let opened = open(cx, true);
    let rows = [line(0), line(1)];
    show(cx, &opened, &rows);
    let before = opened.notified.get();
    show(cx, &opened, &rows);
    assert_eq!(opened.notified.get(), before, "the same rows said again repainted the list");
    show(cx, &opened, &[line(0)]);
    assert_eq!(opened.notified.get(), before + 1);
    cx.update(|cx| assert_eq!(opened.view.read(cx).len(), 1));
}
