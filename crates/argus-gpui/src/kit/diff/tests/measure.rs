//! How wide and how tall the rows are, and what sideways scrolling is left to a long line.

use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::{point, px, size, TestAppContext};

use super::*;

#[gpui_kit::test]
fn a_line_longer_than_the_pane_scrolls_sideways_and_is_not_cut(cx: &mut TestAppContext) {
    let opened = open(cx, true);
    let rows = [line(0), text_line(1, &"x".repeat(400)), line(2)];
    settle(cx, &opened, &rows, false);
    let scroll = cx.update(|cx| opened.view.read(cx).scroll().clone());
    let max = scroll.max_offset().x;
    // 400 cells are far more than the 600 px pane, whatever the font.
    assert!(max > px(600.0), "the list has {max:?} to scroll sideways");
    let long = cx.update_window(opened.handle, |_, window, _| window.find(("diff-row", 1usize)).bounds()).unwrap();
    let short = cx.update_window(opened.handle, |_, window, _| window.find(("diff-row", 0usize)).bounds()).unwrap();
    assert_eq!(long.size.width, short.size.width, "every row carries the column's width, so each colour runs on");
    assert!(long.size.width > px(1000.0), "the row is {:?} wide, not cut at the pane", long.size.width);
    assert_eq!(scroll.offset().x, px(0.0));
    cx.update_window(opened.handle, |_, window, cx| {
        // Over the first screenful: the long row's centre is off the window.
        let wheel = gpui_kit::ScrollWheelEvent {
            position: point(px(100.0), px(30.0)),
            delta: gpui_kit::ScrollDelta::Pixels(point(px(-120.0), px(0.0))),
            ..Default::default()
        };
        window.dispatch_event(gpui_kit::InputEvent::to_platform_input(wheel), cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(scroll.offset().x, px(-120.0), "a sideways wheel moves the list sideways");
}

#[gpui_kit::test]
fn a_diff_of_short_lines_has_no_sideways_range_and_fills_the_pane(cx: &mut TestAppContext) {
    let opened = open(cx, true);
    settle(cx, &opened, &[line(0), text_line(1, "short"), line(2)], false);
    let scroll = cx.update(|cx| opened.view.read(cx).scroll().clone());
    assert_eq!(scroll.max_offset().x, px(0.0));
    let width = cx.update_window(opened.handle, |_, window, _| window.find(("diff-row", 1usize)).bounds().size.width).unwrap();
    assert_eq!(width, px(600.0) - Scrollbar::width(), "a row is stretched to the pane, short of the bar's gutter");
}

#[gpui_kit::test]
fn a_long_half_of_a_split_row_wraps_and_the_rows_around_it_keep_one_line(cx: &mut TestAppContext) {
    let opened = open(cx, true);
    let rows = [pair(0, "a", "b"), pair(1, &"y".repeat(200), "short"), pair(2, "c", "d")];
    settle(cx, &opened, &rows, true);
    let narrow = cx.update(|cx| opened.view.read(cx).heights.clone());
    assert_eq!(narrow[0], shape::LINE);
    assert_eq!(narrow[2], shape::LINE);
    assert!(narrow[1] > shape::LINE * 2.0, "200 cells in a half of a 600 px pane are several lines, got {}", narrow[1]);
    let drawn = cx.update_window(opened.handle, |_, window, _| window.find(("diff-row", 1usize)).bounds().size.height).unwrap();
    assert_eq!(drawn, px(narrow[1]), "the row is drawn as tall as the list was told");
    let scroll = cx.update(|cx| opened.view.read(cx).scroll().clone());
    assert_eq!(scroll.max_offset().x, px(0.0), "a split view wraps and never scrolls sideways");

    let before = opened.notified.get();
    cx.simulate_window_resize(opened.handle, size(px(1200.0), px(400.0)));
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.run_until_parked();
    assert!(opened.notified.get() > before, "a new pane width asks the view for another render, which reads it");
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    }
    let wide = cx.update(|cx| opened.view.read(cx).heights.clone());
    assert!(wide[1] > shape::LINE && wide[1] < narrow[1], "a wider pane wraps the half to fewer lines: {} then {}", narrow[1], wide[1]);
}
