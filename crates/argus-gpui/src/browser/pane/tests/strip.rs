//! The tab strip with more tabs than it has room for (SHELL-BROWSER-6): the tabs scroll in their own
//! box, the buttons after them stay in reach, and the box's right edge fades as the page's does.

use gpui_kit::TestAppContext;

use super::*;

fn crowded() -> PageBrowser {
    let mut crowded = browser();
    crowded.tabs = (0..14).map(|n| tab(&format!("t{n}"), &format!("A long tab title number {n}"), "")).collect();
    crowded.shown = Some("t0".into());
    crowded
}

fn bounds_of(cx: &mut TestAppContext, opened: &Opened, id: &str) -> gpui_kit::Bounds<gpui_kit::Pixels> {
    cx.update_window(opened.handle, |_, window, _| window.find(id.to_owned()).bounds()).unwrap()
}

#[gpui_kit::test]
fn a_long_strip_scrolls_in_its_own_box_and_keeps_the_new_tab_and_hold_buttons_in_reach(cx: &mut TestAppContext) {
    let opened = opened(cx, 0, crowded());
    let (strip, scroll, plus, hold) = (
        bounds_of(cx, &opened, "browser-tabs-105"),
        bounds_of(cx, &opened, "browser-tabs-scroll-105"),
        bounds_of(cx, &opened, "browser-new-tab-105"),
        bounds_of(cx, &opened, "browser-hold-105"),
    );
    assert!(scroll.right() <= plus.origin.x && hold.right() <= strip.right(), "the scrolling box gives way to the buttons");
    assert!(plus.size.width > gpui_kit::px(0.0), "and the buttons keep their size");
}

#[gpui_kit::test]
fn the_scrolling_boxs_right_edge_fades_into_the_bar_over_the_last_24_px_always(cx: &mut TestAppContext) {
    // `.tabs-scroll`'s mask: the page fades the edge whether or not the tabs reach it.
    for browser in [crowded(), browser()] {
        let opened = opened(cx, 0, browser);
        let (scroll, fade) = (bounds_of(cx, &opened, "browser-tabs-scroll-105"), bounds_of(cx, &opened, "browser-tabs-fade-105"));
        assert_eq!(fade.right(), scroll.right(), "at the box's right edge");
        assert!((f32::from(fade.size.width) - 24.0).abs() < 1.0, "{:?}", fade.size);
        assert_eq!(fade.size.height, scroll.size.height);
    }
}
