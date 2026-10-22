//! A Markdown in a test window: the leaves it draws, the text a drag selects, what a click on a
//! link asks for and what copies.

use std::cell::RefCell;
use std::rc::Rc;

use argus_desktop_core::connection::page::Call;
use gpui_kit::prelude::*;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{point, px, size, AnyWindowHandle, Bounds, Entity, Pixels, Point, TestAppContext, WindowBounds, WindowOptions};

use super::Markdown;
use crate::connection::caller::Caller;

mod draw;
mod links;
mod selection;

pub(super) struct Opened {
    pub(super) handle: AnyWindowHandle,
    pub(super) view: Entity<Markdown>,
    pub(super) calls: Rc<RefCell<Vec<Call>>>,
}

pub(super) fn open(cx: &mut TestAppContext, text: &str) -> Opened {
    cx.update(crate::window::init);
    let calls: Rc<RefCell<Vec<Call>>> = Rc::default();
    let sink = calls.clone();
    let mut built = None;
    let (handle, _) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(Default::default(), size(px(600.0), px(500.0))))),
                ..Default::default()
            },
            cx,
            |_, cx| {
                let caller = Caller::new(move |call| sink.borrow_mut().push(call));
                let view = cx.new(|cx| Markdown::new("md", text, caller, cx));
                built = Some(view.clone());
                view
            },
        )
        .expect("open test window")
    });
    let opened = Opened { handle, view: built.unwrap(), calls };
    settle(cx, &opened);
    opened
}

pub(super) fn settle(cx: &mut TestAppContext, opened: &Opened) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    }
}

/// The point at the middle of the line of `leaf`'s text where its byte `at` is.
pub(super) fn at(cx: &mut TestAppContext, opened: &Opened, leaf: usize, at: usize) -> Point<Pixels> {
    cx.update(|cx| {
        let view = opened.view.read(cx);
        let layouts = view.layouts.borrow();
        let layout = layouts.get(&leaf).unwrap_or_else(|| panic!("leaf {leaf} was not drawn: {:?}", layouts.keys().collect::<Vec<_>>()));
        let place = layout.position_for_index(at).unwrap_or_else(|| layout.bounds().origin);
        point(place.x + px(0.5), place.y + layout.line_height() / 2.0)
    })
}

pub(super) fn drag(cx: &mut TestAppContext, opened: &Opened, from: Point<Pixels>, to: Point<Pixels>) {
    cx.update_window(opened.handle, |_, window, cx| window.drag(from, to, cx)).unwrap();
    settle(cx, opened);
}

pub(super) fn selected(cx: &mut TestAppContext, opened: &Opened) -> Option<String> {
    cx.update(|cx| opened.view.read(cx).selection_text())
}

pub(super) fn click(cx: &mut TestAppContext, opened: &Opened, place: Point<Pixels>) {
    use gpui_kit::test::TestEventExt as _;
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(place, gpui_kit::MouseButton::Left, cx);
        window.pointer_up(place, gpui_kit::MouseButton::Left, cx);
    })
    .unwrap();
    settle(cx, opened);
}

/// A click at byte `byte` of leaf `leaf`'s text.
pub(super) fn click_in(cx: &mut TestAppContext, opened: &Opened, leaf: usize, byte: usize) {
    let place = at(cx, opened, leaf, byte);
    click(cx, opened, place);
}
