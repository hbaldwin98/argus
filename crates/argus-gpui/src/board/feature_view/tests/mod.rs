//! An open feature in the whole window: what each lens draws of the session's page, what each press and
//! key sends, and what a push does to the form, the legend and the expanded diagram. Rules cited are
//! `gpui/parity/board.md`'s.

use argus_desktop_core::connection::page::{Event, FeatureView, Features};
use gpui_kit::test::{TestQueryExt as _, TestWindowExt as _};
use gpui_kit::{AppContext as _, ElementId, InputEvent, MouseButton, MouseDownEvent, MouseUpEvent, TestAppContext};

use crate::board::snapshots;
use crate::keys::root::tests::{hear, Opened};

mod flows;
mod forms;
mod head;
mod record;
mod work;

pub(crate) fn on(cx: &mut TestAppContext, scene: &str) -> Opened {
    snapshots::on(cx, scene)
}

pub(crate) fn features(opened: &Opened, cx: &mut TestAppContext) -> Features {
    cx.update(|cx| opened.heard.read(cx).features.as_deref().cloned().expect("the Feature stage was sent"))
}

pub(crate) fn view(opened: &Opened, cx: &mut TestAppContext) -> FeatureView {
    features(opened, cx).open.expect("a feature is open")
}

/// The window hears `view` as the open feature, in the scene's page.
pub(crate) fn push(cx: &mut TestAppContext, opened: &Opened, view: FeatureView) {
    let mut page = features(opened, cx);
    page.open = Some(view);
    hear(cx, opened, vec![Event::Features(Box::new(page))]);
}

/// The width of the element `id`, in the window's pixels.
pub(crate) fn width(cx: &mut TestAppContext, opened: &Opened, id: impl Into<ElementId>) -> f32 {
    let id = id.into();
    cx.update_window(opened.handle, |_, window, _| window.find(id).bounds().size.width.as_f32()).unwrap()
}

/// The left edge of the element `id`, in the window's pixels.
pub(crate) fn left(cx: &mut TestAppContext, opened: &Opened, id: impl Into<ElementId>) -> f32 {
    let id = id.into();
    cx.update_window(opened.handle, |_, window, _| window.find(id).bounds().origin.x.as_f32()).unwrap()
}

/// Whether the element `id` says it is selected.
pub(crate) fn selected(cx: &mut TestAppContext, opened: &Opened, id: impl Into<ElementId>) -> Option<bool> {
    let id = id.into();
    cx.update_window(opened.handle, |_, window, _| window.find(id).selected()).unwrap()
}

/// What the field called `name` says now.
pub(crate) fn said(cx: &mut TestAppContext, opened: &Opened, name: &str) -> Option<String> {
    cx.update_window(opened.handle, |_, window, _| window.find_by_label(name).value().map(str::to_owned)).unwrap()
}

/// Presses the element `id` twice, the second press counted as a double click.
pub(crate) fn double_click(cx: &mut TestAppContext, opened: &Opened, id: impl Into<ElementId>) {
    let id = id.into();
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
