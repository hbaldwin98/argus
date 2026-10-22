//! What the tests of the stage find and press in the window by the name an element is drawn with.

use gpui_kit::test::{TestEventExt as _, TestQueryExt as _, TestWindowExt as _};
use gpui_kit::{AppContext as _, ElementId, InputEvent, MouseButton, MouseDownEvent, MouseUpEvent, TestAppContext};

use crate::keys::root::tests::Opened;

/// Presses the element named `id`, as a person's click on it does: for a name two elements share.
pub(crate) fn click_id(cx: &mut TestAppContext, opened: &Opened, id: impl Into<ElementId>) {
    let id = id.into();
    cx.update_window(opened.handle, |_, window, cx| window.click(id, cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.run_until_parked();
}

/// Presses the element `id` twice, the second press counted as a double click.
pub(crate) fn double_click_id(cx: &mut TestAppContext, opened: &Opened, id: &'static str) {
    let at = cx.update_window(opened.handle, |_, window, _| window.find(id).bounds().center()).unwrap();
    for count in [1, 2] {
        cx.update_window(opened.handle, |_, window, cx| {
            window.dispatch_event(InputEvent::to_platform_input(MouseDownEvent { button: MouseButton::Left, position: at, click_count: count, ..Default::default() }), cx);
            window.dispatch_event(InputEvent::to_platform_input(MouseUpEvent { button: MouseButton::Left, position: at, click_count: count, ..Default::default() }), cx);
        })
        .unwrap();
    }
    cx.run_until_parked();
}

/// Whether the window draws the element `id`.
pub(crate) fn present(cx: &mut TestAppContext, opened: &Opened, id: impl Into<ElementId>) -> bool {
    let id = id.into();
    cx.update_window(opened.handle, |_, window, _| window.try_find(id).is_some()).unwrap()
}

/// Whether the element `id` says it is selected.
pub(crate) fn selected(cx: &mut TestAppContext, opened: &Opened, id: &'static str) -> Option<bool> {
    cx.update_window(opened.handle, |_, window, _| window.find(id).selected()).unwrap()
}

/// Whether the element `id` says it is expanded.
pub(crate) fn expanded(cx: &mut TestAppContext, opened: &Opened, id: &'static str) -> Option<bool> {
    cx.update_window(opened.handle, |_, window, _| window.find(id).expanded()).unwrap()
}

/// What the element `id` is called to a reader that cannot see it.
pub(crate) fn label(cx: &mut TestAppContext, opened: &Opened, id: impl Into<ElementId>) -> Option<String> {
    let id = id.into();
    cx.update_window(opened.handle, |_, window, _| window.try_find(id).and_then(|found| found.label().map(str::to_owned))).unwrap()
}

/// Points at the element `id` as a person does before pressing a button that shows only while its
/// row is pointed at.
pub(crate) fn hover_id(cx: &mut TestAppContext, opened: &Opened, id: &'static str) {
    let at = cx.update_window(opened.handle, |_, window, _| window.find(id).bounds().center()).unwrap();
    cx.update_window(opened.handle, |_, window, cx| window.pointer_move(at, None, cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

/// Clicks the `nth` item of the open menu: the kit gives its items no accessible label, only their
/// place in the list (headings and rules count).
pub(crate) fn click_menu_item(cx: &mut TestAppContext, opened: &Opened, nth: u64) {
    let at = cx
        .update_window(opened.handle, |_, window, _| {
            let item = window.elements().into_iter().find(|element| {
                let path = element.path();
                path.iter().any(|id| matches!(id, ElementId::Name(name) if name.as_ref() == "popup-menu")) && matches!(path.last(), Some(ElementId::Integer(n)) if n == &nth)
            });
            item.expect("the menu has that item").bounds().center()
        })
        .unwrap();
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(at, MouseButton::Left, cx);
        window.pointer_up(at, MouseButton::Left, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

/// Presses the element `id` of a dialog, which slides in over its first frames on the clock of the
/// machine and not the test's: the pointer goes where the element is once a frame has left it there.
pub(crate) fn click_in_dialog(cx: &mut TestAppContext, opened: &Opened, id: &'static str) {
    let find = |cx: &mut TestAppContext| cx.update_window(opened.handle, |_, window, _| window.find(id).bounds().center()).unwrap();
    let mut at = find(cx);
    for _ in 0..60 {
        cx.update_window(opened.handle, |_, window, cx| window.pointer_move(at, None, cx)).unwrap();
        let next = find(cx);
        if next == at {
            break;
        }
        at = next;
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_move(at, None, cx);
        window.pointer_down(at, MouseButton::Left, cx);
        window.pointer_up(at, MouseButton::Left, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.run_until_parked();
}
