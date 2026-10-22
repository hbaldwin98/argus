//! The block renderer in a test window: a view's blocks as JSON, drawn, clicked and shown again, to
//! see what is on screen, what a press asks Rust for and what the window remembers.
//!
//! The harness is shared (`pub(crate)`): the buttons of `views/actions` are tested through the blocks
//! that hold them.

use std::cell::RefCell;
use std::rc::Rc;

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::views::saved::SavedViewsPage;
use argus_protocol::Block;
use gpui_kit::prelude::*;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{px, size, AnyWindowHandle, Bounds, Entity, Pixels, TestAppContext, WindowBounds, WindowOptions};

use super::Blocks;
use crate::connection::caller::Caller;
use crate::views::ctx::ViewCtx;
use crate::views::memory::{Memory, Shared};

mod actions;
mod containers;
mod changed;
mod drawn;
mod figure;
mod form;
mod leaves;
mod patch;
mod saved;
mod table;

pub(crate) struct Rig {
    pub(crate) handle: AnyWindowHandle,
    pub(crate) view: Entity<Blocks>,
    frame: Entity<Frame>,
    pub(crate) calls: Rc<RefCell<Vec<Call>>>,
    pub(crate) memory: Shared,
}

pub(crate) fn parse(json: serde_json::Value) -> Rc<Vec<Block>> {
    Rc::new(serde_json::from_value(json).expect("blocks"))
}

/// A window `width` wide holding the blocks `json` of the agent view `7/v`.
pub(crate) fn open(cx: &mut TestAppContext, width: f32, json: serde_json::Value) -> Rig {
    open_as(cx, width, json, ViewCtx { can_press: true, ..ViewCtx::agent(7, "v") })
}

pub(crate) fn open_as(cx: &mut TestAppContext, width: f32, json: serde_json::Value, ctx: ViewCtx) -> Rig {
    open_in(cx, width, json, ctx, None)
}

/// The same inside a page 400 px tall that scrolls, whose scroll is the handle returned: the box a
/// block's own scroll box has to hand the wheel to.
pub(crate) fn open_in_page(cx: &mut TestAppContext, width: f32, json: serde_json::Value) -> (Rig, gpui_kit::ScrollHandle) {
    let page = gpui_kit::ScrollHandle::new();
    let rig = open_in(cx, width, json, ViewCtx { can_press: true, ..ViewCtx::agent(7, "v") }, Some(page.clone()));
    (rig, page)
}

fn open_in(cx: &mut TestAppContext, width: f32, json: serde_json::Value, ctx: ViewCtx, outer: Option<gpui_kit::ScrollHandle>) -> Rig {
    cx.update(crate::window::init);
    let calls: Rc<RefCell<Vec<Call>>> = Rc::default();
    let memory = Memory::shared();
    let (sink, held) = (calls.clone(), memory.clone());
    let mut built = None;
    let blocks = parse(json);
    let (handle, _) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions { window_bounds: Some(WindowBounds::Windowed(Bounds::new(Default::default(), size(px(width), px(900.0))))), ..Default::default() },
            cx,
            |_, cx| {
                let caller = Caller::new(move |call| sink.borrow_mut().push(call));
                let view = cx.new(|cx| {
                    let mut view = Blocks::new(held, caller);
                    view.show(blocks, ctx, None, cx);
                    view
                });
                let frame = cx.new(|_| Frame { blocks: view.clone(), width: px(width), outer });
                built = Some((view, frame.clone()));
                frame
            },
        )
        .expect("open test window")
    });
    let (view, frame) = built.unwrap();
    let rig = Rig { handle, view, frame, calls, memory };
    settle(cx, &rig);
    rig
}

/// Shows `json` as the view `ctx` is, as the daemon does whenever its agent shows a view again.
pub(crate) fn show(cx: &mut TestAppContext, rig: &Rig, json: serde_json::Value, ctx: ViewCtx) {
    show_with(cx, rig, json, ctx, None);
}

/// The same, with the saved views the window holds.
pub(crate) fn show_with(cx: &mut TestAppContext, rig: &Rig, json: serde_json::Value, ctx: ViewCtx, saved: Option<Rc<SavedViewsPage>>) {
    let blocks = parse(json);
    cx.update(|cx| rig.view.update(cx, |view, cx| view.show(blocks, ctx, saved, cx)));
    settle(cx, rig);
}

pub(crate) fn settle(cx: &mut TestAppContext, rig: &Rig) {
    for _ in 0..4 {
        cx.run_until_parked();
        cx.update_window(rig.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    }
}

pub(crate) fn bounds(cx: &mut TestAppContext, rig: &Rig, id: &str) -> Option<Bounds<Pixels>> {
    cx.update_window(rig.handle, |_, window, _| window.try_find(id.to_owned()).map(|found| found.bounds())).unwrap()
}

pub(crate) fn exists(cx: &mut TestAppContext, rig: &Rig, id: &str) -> bool {
    bounds(cx, rig, id).is_some()
}

pub(crate) fn label(cx: &mut TestAppContext, rig: &Rig, id: &str) -> Option<String> {
    cx.update_window(rig.handle, |_, window, _| window.try_find(id.to_owned()).and_then(|found| found.label().map(str::to_owned))).unwrap()
}

pub(crate) fn click(cx: &mut TestAppContext, rig: &Rig, id: &str) {
    cx.update_window(rig.handle, |_, window, cx| window.click(id.to_owned(), cx)).unwrap();
    settle(cx, rig);
}

pub(crate) fn said(call: Call) -> String {
    format!("{call:?}")
}

/// What was asked of Rust, as `said` writes it (a `Call` has no equality).
pub(crate) fn calls(rig: &Rig) -> Vec<String> {
    rig.calls.borrow().iter().map(|call| said(call.clone())).collect()
}

/// What holds the blocks: a box whose width a test can change, as a dock or a column is dragged.
struct Frame {
    blocks: Entity<Blocks>,
    width: Pixels,
    outer: Option<gpui_kit::ScrollHandle>,
}

impl Render for Frame {
    fn render(&mut self, _: &mut gpui_kit::Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
        let frame = gpui_kit::div().w(self.width);
        match &self.outer {
            Some(page) => frame.id("page").h(px(400.0)).overflow_y_scroll().track_scroll(page).child(self.blocks.clone()).into_any_element(),
            None => frame.child(self.blocks.clone()).into_any_element(),
        }
    }
}

/// Makes the box the blocks are in `width` wide, which the window's own width does not change.
pub(crate) fn set_width(cx: &mut TestAppContext, rig: &Rig, width: f32) {
    cx.update(|cx| rig.frame.update(cx, |frame, cx| {
        frame.width = px(width);
        cx.notify();
    }));
    settle(cx, rig);
}
