//! The diff view in a test window: the rows it draws, the sizes it measures, the pan and the
//! selection.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use argus_desktop_core::connection::page::{DiffRow, DiffSide};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{prelude::*, px, size, Bounds, Entity, TestAppContext, WindowBounds, WindowOptions};

use super::gutter::{MARKER, NUMBER};
use super::shape;
use super::{DiffEvent, DiffView};

mod measure;
mod pan;
mod rows;
mod selection;

pub(super) fn side(old: Option<u32>, new: Option<u32>, kind: &'static str, parts: &[(&str, &'static str)]) -> DiffSide {
    DiffSide {
        old,
        new,
        marker: "",
        kind,
        parts: parts.iter().map(|(text, kind)| ((*text).to_owned(), *kind)).collect(),
    }
}

pub(super) fn row(kind: &'static str) -> DiffRow {
    DiffRow { row: 0, kind, text: String::new(), hidden: None, step: None, left: None, right: None, cursor: false, picked: false }
}

pub(super) fn line(row: usize) -> DiffRow {
    let side = DiffSide { old: Some(row as u32), new: Some(row as u32), marker: "", kind: "context", parts: vec![("x".into(), "")] };
    DiffRow { row, kind: "line", text: String::new(), hidden: None, step: None, left: Some(side), right: None, cursor: false, picked: false }
}

pub(super) fn fold(row: usize) -> DiffRow {
    DiffRow { row, kind: "fold", text: "8 lines".into(), hidden: Some(8), step: None, left: None, right: None, cursor: false, picked: false }
}

pub(super) struct Opened {
    pub(super) handle: gpui_kit::AnyWindowHandle,
    pub(super) view: Entity<DiffView>,
    pub(super) pressed: Rc<RefCell<Vec<DiffEvent>>>,
    pub(super) notified: Rc<Cell<u32>>,
}

pub(super) fn open(cx: &mut TestAppContext, interactive: bool) -> Opened {
    open_picking(cx, interactive, false)
}

pub(super) fn open_picking(cx: &mut TestAppContext, interactive: bool, picks: bool) -> Opened {
    cx.update(crate::window::init);
    let pressed: Rc<RefCell<Vec<DiffEvent>>> = Rc::default();
    let notified: Rc<Cell<u32>> = Rc::default();
    let mut built = None;
    let (handle, _) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(Default::default(), size(px(600.0), px(400.0))))),
                ..Default::default()
            },
            cx,
            |_, cx| {
                let view = cx.new(|cx| DiffView::new("diff", interactive, picks, cx));
                let sink = pressed.clone();
                cx.subscribe(&view, move |_, event: &DiffEvent, _| sink.borrow_mut().push(*event)).detach();
                let count = notified.clone();
                cx.observe(&view, move |_, _| count.set(count.get() + 1)).detach();
                built = Some(view.clone());
                view
            },
        )
        .expect("open test window")
    });
    Opened { handle, view: built.unwrap(), pressed, notified }
}

pub(super) fn show(cx: &mut TestAppContext, opened: &Opened, rows: &[DiffRow]) {
    cx.update(|cx| opened.view.update(cx, |view, cx| view.show(rows, false, None, cx)));
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

pub(super) fn text_line(row: usize, text: &str) -> DiffRow {
    let mut line = line(row);
    line.left.as_mut().unwrap().parts = vec![(text.into(), "")];
    line
}

pub(super) fn pair(row: usize, left: &str, right: &str) -> DiffRow {
    let side = |text: &str, kind: &'static str| DiffSide { old: Some(row as u32), new: Some(row as u32), marker: "", kind, parts: vec![(text.into(), "")] };
    DiffRow {
        row,
        kind: "pair",
        text: String::new(),
        hidden: None,
        step: None,
        left: Some(side(left, "removed")),
        right: Some(side(right, "added")),
        cursor: false,
        picked: false,
    }
}

/// The rows drawn and settled: the pane's width is read a frame after it is laid out.
pub(super) fn settle(cx: &mut TestAppContext, opened: &Opened, rows: &[DiffRow], split: bool) {
    cx.update(|cx| opened.view.update(cx, |view, cx| view.show(rows, split, None, cx)));
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    }
}
