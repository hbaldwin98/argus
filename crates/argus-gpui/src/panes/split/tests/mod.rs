//! The grid in a test window: the whole shell, with the cells, gutters and drags as a person
//! meets them. The harness here is shared by the tests of each question the grid answers.

use argus_desktop_core::connection::page::{
    Call, Checkout, Connection, Event, Pane, Project, ProjectChoice, Rail, Repository, Shown, Side, Stage, Status,
};
use argus_desktop_core::panes::shown::workspace_grid::GridPage;
use gpui_kit::test::{TestEventExt as _, TestWindowExt as _};
use gpui_kit::AppContext as _;
use gpui_kit::{point, px, Bounds, MouseButton, Pixels, Point, TestAppContext};

use crate::keys::root::tests::{open, Opened};

mod carry;
mod cells;
mod gutters;
mod keys;
mod panels;

pub(super) fn shown(pane: u64, title: &str) -> Shown {
    Shown {
        pane,
        repository: "argus".into(),
        checkout: "main".into(),
        branch: "main".into(),
        agent: false,
        browser: false,
        title: title.into(),
        status: "idle",
        resumed: None,
    }
}

/// The grid of three panes: the Workspace's and one beside it above, one under both.
pub(super) fn three() -> GridPage {
    GridPage {
        beside: vec![Some(shown(2, "two")), Some(shown(3, "three"))],
        panels: vec![None, None],
        grid: vec![vec![0, 1], vec![2]],
        heights: vec![500, 500],
        widths: vec![vec![500, 500], vec![1000]],
        keyed: Some(0),
        min_cols: 20,
        min_rows: 6,
    }
}

/// One pane alone.
pub(super) fn alone() -> GridPage {
    GridPage { beside: Vec::new(), panels: Vec::new(), grid: vec![vec![0]], heights: vec![1000], widths: vec![vec![1000]], keyed: Some(0), min_cols: 20, min_rows: 6 }
}

fn pane(id: u64, title: &str, on: bool) -> Pane {
    Pane { id, agent: false, title: title.into(), status: "idle", note: None, on, views: 0 }
}

fn checkout(id: u64, name: &str, panes: Vec<Pane>) -> Checkout {
    Checkout {
        id,
        name: name.into(),
        branch: "main".into(),
        kind: "main",
        git: None,
        clean: true,
        shared: None,
        open: true,
        count: panes.len(),
        status: None,
        panes,
    }
}

/// A rail whose Workspace shows pane 1 with `row` as its grid, listing panes 1 to 3 in checkout 110
/// and a second checkout, 112, to move them to.
pub(super) fn rail(row: GridPage) -> Event {
    let repository = Repository {
        id: 11,
        name: "argus".into(),
        quiet: false,
        divider: false,
        open: true,
        panes: 3,
        status: None,
        used: None,
        checkouts: vec![
            checkout(110, "main", vec![pane(1, "one", true), pane(2, "two", false), pane(3, "three", false)]),
            checkout(112, "linker", vec![pane(4, "four", false)]),
        ],
    };
    Event::Rail(Box::new(Rail {
        projects: vec![ProjectChoice { id: 1, name: "src".into(), needs_you: 0, working: 0 }],
        project: Some(Project { id: 1, name: "src".into(), needs_you: 0, repositories: vec![repository], running: Vec::new() }),
        loaded: true,
        repository: Some(11),
        stage: Stage::Workspace,
        workspace_shown: true,
        shown: Some(shown(1, "one")),
        row,
        ..Rail::default()
    }))
}

pub(super) fn connected() -> Event {
    Event::Status(Status { connection: Connection::Connected, detail: None, alarm: None, chime: true, host: None })
}

/// The shell, connected, whose rail says `row`.
pub(super) fn opened(cx: &mut TestAppContext, row: GridPage) -> Opened {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), rail(row)]);
    opened.calls.borrow_mut().clear();
    opened
}

pub(super) fn hear(cx: &mut TestAppContext, opened: &Opened, events: Vec<Event>) {
    opened.events.send(events).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

pub(super) fn has(cx: &mut TestAppContext, opened: &Opened, id: &str) -> bool {
    cx.update_window(opened.handle, |_, window, _| window.try_find(id.to_owned()).is_some()).unwrap()
}

pub(super) fn bounds(cx: &mut TestAppContext, opened: &Opened, id: &str) -> Bounds<Pixels> {
    cx.update_window(opened.handle, |_, window, _| window.find(id.to_owned()).bounds()).unwrap()
}

pub(super) fn centre(cx: &mut TestAppContext, opened: &Opened, id: &str) -> Point<Pixels> {
    bounds(cx, opened, id).center()
}

pub(super) fn label(cx: &mut TestAppContext, opened: &Opened, id: &str) -> Option<String> {
    cx.update_window(opened.handle, |_, window, _| window.try_find(id.to_owned()).and_then(|found| found.label().map(str::to_owned)))
        .unwrap()
}

pub(super) fn by(at: Point<Pixels>, dx: f32, dy: f32) -> Point<Pixels> {
    point(at.x + px(dx), at.y + px(dy))
}

pub(super) fn down(cx: &mut TestAppContext, opened: &Opened, at: Point<Pixels>) {
    cx.update_window(opened.handle, |_, window, cx| window.pointer_down(at, MouseButton::Left, cx)).unwrap();
    cx.run_until_parked();
}

pub(super) fn drag_to(cx: &mut TestAppContext, opened: &Opened, at: Point<Pixels>) {
    cx.update_window(opened.handle, |_, window, cx| window.pointer_move(at, Some(MouseButton::Left), cx)).unwrap();
    cx.run_until_parked();
}

pub(super) fn up(cx: &mut TestAppContext, opened: &Opened, at: Point<Pixels>) {
    cx.update_window(opened.handle, |_, window, cx| window.pointer_up(at, MouseButton::Left, cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

pub(super) fn escape(cx: &mut TestAppContext, opened: &Opened) {
    cx.update_window(opened.handle, |_, window, cx| window.press("escape", cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

/// What the grid asked of the session, in words.
pub(super) fn asked(opened: &Opened) -> Vec<String> {
    opened
        .calls
        .borrow()
        .iter()
        .filter(|call| !matches!(call, Call::Focus { .. } | Call::Resize { .. } | Call::Key { .. }))
        .map(|call| format!("{call:?}"))
        .collect()
}

/// The rooms the screens reported, by side.
pub(super) fn rooms(opened: &Opened) -> Vec<(Side, u16, u16)> {
    opened
        .calls
        .borrow()
        .iter()
        .filter_map(|call| match call {
            Call::Resize { side, rows, cols } => Some((*side, *rows, *cols)),
            _ => None,
        })
        .collect()
}

