//! The pointer on the screen as a window's own is on a page: hover, press and release apart, a drag
//! that holds its button, and leaving.

use gpui_kit::TestAppContext;

use super::*;

/// A cell with a picture heard, and the screen's middle and a point on the toolbar above it.
fn pictured(cx: &mut TestAppContext) -> (Opened, gpui_kit::Point<gpui_kit::Pixels>, gpui_kit::Point<gpui_kit::Pixels>) {
    let opened = opened(cx, 1, browser());
    hear(cx, &opened, vec![Event::BrowserShot(BrowserShot { pane: PANE, jpeg: png(), width: 1280, height: 800 })]);
    cx.run_until_parked();
    let centre = bounds(cx, &opened, "browser-screen-105").center();
    let off = bounds(cx, &opened, "browser-bar-105").center();
    opened.calls.borrow_mut().clear();
    (opened, centre, off)
}

fn moved(cx: &mut TestAppContext, opened: &Opened, at: gpui_kit::Point<gpui_kit::Pixels>, held: Option<MouseButton>) {
    cx.update_window(opened.handle, |_, window, cx| window.pointer_move(at, held, cx)).unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn a_press_and_its_release_are_sent_apart_at_the_point_in_the_pages_pixels(cx: &mut TestAppContext) {
    let (opened, centre, _) = pictured(cx);
    cx.update_window(opened.handle, |_, window, cx| window.pointer_down(centre, MouseButton::Left, cx)).unwrap();
    cx.update_window(opened.handle, |_, window, cx| window.pointer_up(centre, MouseButton::Left, cx)).unwrap();
    cx.run_until_parked();
    let calls: Vec<String> = said(&opened).into_iter().filter(|call| !call.contains("Move {")).collect();
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert!(calls[0].contains("Press { x: 640.0") && calls[0].contains("button: Left, count: 1"), "{calls:?}");
    assert!(calls[1].contains("Release { x: 640.0") && calls[1].contains("button: Left"), "{calls:?}");
}

#[gpui_kit::test]
fn the_pointer_hovers_the_page_once_per_point_and_leaving_it_says_so_once(cx: &mut TestAppContext) {
    let (opened, centre, off) = pictured(cx);
    moved(cx, &opened, centre, None);
    moved(cx, &opened, centre, None);
    let calls = said(&opened);
    assert!(calls.len() == 1 && calls[0].contains("Move { x: 640.0") && calls[0].contains("held: None"), "{calls:?}");
    moved(cx, &opened, off, None);
    moved(cx, &opened, off, None);
    assert_eq!(said(&opened), ["BrowserInput { pane: 105, input: Leave }"]);
}

#[gpui_kit::test]
fn a_drag_holds_its_button_and_a_release_past_the_screen_lets_it_go_where_it_last_was(cx: &mut TestAppContext) {
    let (opened, centre, off) = pictured(cx);
    cx.update_window(opened.handle, |_, window, cx| window.pointer_down(centre, MouseButton::Left, cx)).unwrap();
    let further = gpui_kit::point(centre.x + gpui_kit::px(20.0), centre.y);
    moved(cx, &opened, further, Some(MouseButton::Left));
    let dragged = said(&opened);
    assert!(dragged.iter().any(|call| call.contains("Move {") && call.contains("held: Some(Left)")), "{dragged:?}");
    moved(cx, &opened, off, Some(MouseButton::Left));
    assert!(said(&opened).is_empty(), "a drag off the page is not a leave: the page still holds the button");
    cx.update_window(opened.handle, |_, window, cx| window.pointer_up(off, MouseButton::Left, cx)).unwrap();
    cx.run_until_parked();
    let released = said(&opened);
    assert!(released.len() == 1 && released[0].contains("Release {") && !released[0].contains("x: 640.0"), "{released:?}");
}

#[gpui_kit::test]
fn a_release_the_page_was_not_pressed_for_sends_nothing(cx: &mut TestAppContext) {
    let (opened, centre, _) = pictured(cx);
    cx.update_window(opened.handle, |_, window, cx| window.pointer_up(centre, MouseButton::Left, cx)).unwrap();
    cx.run_until_parked();
    assert!(!said(&opened).iter().any(|call| call.contains("Release")));
}

#[gpui_kit::test]
fn the_toolbar_offers_a_copy_of_the_picture(cx: &mut TestAppContext) {
    let opened = opened(cx, 1, browser());
    assert!(has(cx, &opened, "browser-copy-105"));
    assert_eq!(label(cx, &opened, "browser-copy-105").as_deref(), Some(crate::browser::pane::shot::COPY_TIP));
}
