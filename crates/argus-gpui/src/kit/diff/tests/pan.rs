//! The nudge and the middle button's drag.

use gpui_kit::{point, px, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, TestAppContext};

use super::*;

#[gpui_kit::test]
fn a_nudge_and_a_middle_drag_scroll_sideways_held_to_the_content(cx: &mut TestAppContext) {
    let opened = open(cx, true);
    settle(cx, &opened, &[line(0), text_line(1, &"x".repeat(400)), line(2)], false);
    let scroll = cx.update(|cx| opened.view.read(cx).scroll().clone());
    cx.update(|cx| opened.view.update(cx, |view, cx| view.nudge(4.0, cx)));
    assert!(scroll.offset().x < px(0.0), "a nudge right shows more of the line");
    cx.update(|cx| opened.view.update(cx, |view, cx| view.nudge(-1000.0, cx)));
    assert_eq!(scroll.offset().x, px(0.0), "a nudge stops at the start");
    cx.update_window(opened.handle, |_, window, cx| {
        let at = point(px(300.0), px(30.0));
        window.dispatch_event(gpui_kit::InputEvent::to_platform_input(MouseDownEvent { button: MouseButton::Middle, position: at, click_count: 1, ..Default::default() }), cx);
        window.dispatch_event(
            gpui_kit::InputEvent::to_platform_input(MouseMoveEvent { position: point(px(150.0), px(30.0)), pressed_button: Some(MouseButton::Middle), ..Default::default() }),
            cx,
        );
        window.dispatch_event(gpui_kit::InputEvent::to_platform_input(MouseUpEvent { button: MouseButton::Middle, position: point(px(150.0), px(30.0)), click_count: 1, ..Default::default() }), cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(scroll.offset().x, px(-150.0), "the diff follows the middle-dragged pointer");
    assert!(opened.pressed.borrow().is_empty(), "a middle drag presses no row");
}
