//! A window that draws one kit piece for a test, so a piece's size and place can be read back
//! without the shell around it.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{div, prelude::*, px, size, AnyElement, AnyWindowHandle, Bounds, Context, Render, TestAppContext, Window, WindowBounds, WindowOptions};

use crate::theme::Tokens;

type Make = Box<dyn Fn(&Tokens) -> AnyElement>;

pub(crate) struct Host {
    make: Make,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        div().size_full().flex().flex_col().child((self.make)(&tokens))
    }
}

/// A window `width` css pixels wide with `make`'s element in it, drawn once.
pub(crate) fn open(cx: &mut TestAppContext, width: f32, make: impl Fn(&Tokens) -> AnyElement + 'static) -> AnyWindowHandle {
    cx.update(crate::window::init);
    let (handle, _) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions { window_bounds: Some(WindowBounds::Windowed(Bounds::new(Default::default(), size(px(width), px(400.0))))), ..Default::default() },
            cx,
            |_, cx| cx.new(|_| Host { make: Box::new(make) }),
        )
        .expect("open test window")
    });
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.render_frame(cx)).unwrap();
    handle
}

/// The bounds `(left, top, width, height)` of the element `id`, in pixels.
pub(crate) fn bounds(cx: &mut TestAppContext, handle: AnyWindowHandle, id: impl Into<gpui_kit::ElementId>) -> (f32, f32, f32, f32) {
    let id = id.into();
    cx.update_window(handle, |_, window, _| {
        let b = window.find(id).bounds();
        (f32::from(b.origin.x), f32::from(b.origin.y), f32::from(b.size.width), f32::from(b.size.height))
    })
    .unwrap()
}
