//! The rail in a test window: the rows it lists, what a press sends, who holds the keys, which row
//! is picked and what the foot says.

use std::cell::RefCell;
use std::rc::Rc;

use argus_desktop_core::connection::page::{Call, Checkout, Connection, Event, Pane, Project, ProjectChoice, Rail, Repository, Shown, Stage, Status};
use argus_desktop_core::panes::console::ConsolePlace;
use gpui_kit::test::{TestEventExt as _, TestWindowExt as _};
use gpui_kit::{point, prelude::*, px, size, AnyWindowHandle, Bounds, Entity, MouseButton, TestAppContext, WindowBounds, WindowOptions};

use super::picked::Picked;
use super::RailView;
use crate::chrome::nav::Nav;
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;

mod calls;
mod foot;
mod keys;
mod picked;
mod rows;

pub(super) fn pane(id: u64, agent: bool, title: &str, status: &'static str, on: bool) -> Pane {
    Pane { id, agent, title: title.into(), status, note: None, on, views: 0 }
}

pub(super) fn checkout_row(id: u64, name: &str, open: bool, panes: Vec<Pane>) -> Checkout {
    Checkout {
        id,
        name: name.into(),
        branch: "main".into(),
        kind: "main",
        git: None,
        clean: true,
        shared: None,
        open,
        count: panes.len(),
        status: None,
        panes,
    }
}

/// One project with two repositories: `argus` active, `dotfiles` quiet.
pub(super) fn busy() -> Rail {
    let argus = Repository {
        id: 11,
        name: "argus".into(),
        quiet: false,
        divider: false,
        open: true,
        panes: 3,
        status: Some("needs-review"),
        used: None,
        checkouts: vec![
            checkout_row(
                110,
                "main",
                true,
                vec![
                    pane(1, true, "Trim context", "working", true),
                    pane(2, false, "zsh", "idle", false),
                    pane(4, true, "Tracker", "needs-review", false),
                ],
            ),
            Checkout {
                git: Some("+1 !2".into()),
                count: 1,
                status: Some("waiting"),
                clean: false,
                shared: Some(2),
                ..checkout_row(112, "tracker-links", false, vec![pane(5, true, "Linker", "waiting", false)])
            },
        ],
    };
    let dotfiles = Repository {
        id: 10,
        name: "dotfiles".into(),
        quiet: true,
        divider: true,
        open: false,
        panes: 0,
        status: None,
        used: Some("16m".into()),
        // Given though the repository is shut: the view must not draw it.
        checkouts: vec![checkout_row(100, "main", true, vec![pane(6, false, "zsh", "idle", false)])],
    };
    Rail {
        projects: vec![
            ProjectChoice { id: 1, name: "src".into(), needs_you: 1, working: 1 },
            ProjectChoice { id: 2, name: "other".into(), needs_you: 0, working: 0 },
        ],
        project: Some(Project {
            id: 1,
            name: "src".into(),
            needs_you: 1,
            repositories: vec![argus, dotfiles],
            running: Vec::new(),
        }),
        needs_you: 1,
        working: 1,
        loaded: true,
        repository: Some(11),
        stage: Stage::Workspace,
        ..Rail::default()
    }
}

pub(super) fn status(connection: Connection) -> Event {
    Event::Status(Status { connection, detail: None, alarm: None, chime: true, host: None })
}

pub(super) type Calls = Rc<RefCell<Vec<Call>>>;

pub(super) struct Opened {
    pub(super) handle: AnyWindowHandle,
    pub(super) heard: Entity<Heard>,
    pub(super) rail: Entity<RailView>,
    pub(super) calls: Calls,
}

pub(super) fn open(cx: &mut TestAppContext, events: Vec<Event>) -> Opened {
    cx.update(crate::window::init);
    let calls: Calls = Rc::default();
    let sink = calls.clone();
    let caller = Caller::new(move |call| sink.borrow_mut().push(call));
    let mut built = None;
    let (handle, _) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    size(px(crate::theme::ledger::tokens().rail_w), px(700.0)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                let heard = cx.new(|_| Heard::new());
                let stage = cx.focus_handle();
                let rail = cx.new(|cx| RailView::new(heard.clone(), caller, stage, window, cx));
                built = Some((heard, rail.clone()));
                rail
            },
        )
        .expect("open test window")
    });
    let (heard, rail) = built.unwrap();
    let opened = Opened { handle, heard, rail, calls };
    hear(cx, &opened, events);
    opened
}

pub(super) fn hear(cx: &mut TestAppContext, opened: &Opened, events: Vec<Event>) {
    cx.update(|cx| {
        opened.heard.update(cx, |heard, cx| {
            for event in events {
                heard.hear(event);
            }
            cx.notify();
        })
    });
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

pub(super) fn click(cx: &mut TestAppContext, opened: &Opened, id: &str) {
    let at = cx
        .update_window(opened.handle, |_, window, _| window.find(id.to_owned()).bounds().center())
        .unwrap();
    press(cx, opened, at, MouseButton::Left);
}

pub(super) fn press(cx: &mut TestAppContext, opened: &Opened, at: gpui_kit::Point<gpui_kit::Pixels>, button: MouseButton) {
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(at, button, cx);
        window.pointer_up(at, button, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

pub(super) fn label(cx: &mut TestAppContext, opened: &Opened, id: &str) -> Option<String> {
    cx.update_window(opened.handle, |_, window, _| {
        window.try_find(id.to_owned()).and_then(|found| found.label().map(str::to_owned))
    })
    .unwrap()
}

pub(super) fn present(cx: &mut TestAppContext, opened: &Opened, id: &str) -> bool {
    cx.update_window(opened.handle, |_, window, _| window.try_find(id.to_owned()).is_some()).unwrap()
}

pub(super) fn selected(cx: &mut TestAppContext, opened: &Opened, id: &str) -> Option<bool> {
    cx.update_window(opened.handle, |_, window, _| window.find(id.to_owned()).selected()).unwrap()
}

pub(super) fn taken(opened: &Opened) -> Vec<String> {
    opened.calls.borrow().iter().map(|call| format!("{call:?}")).collect()
}

pub(super) fn connected_and_busy() -> Vec<Event> {
    vec![status(Connection::Connected), Event::Rail(Box::new(busy()))]
}

/// The whole window, rail and stage, where the keys can be told apart.
pub(super) fn open_shell(cx: &mut TestAppContext) -> Opened {
    cx.update(crate::window::init);
    let calls: Calls = Rc::default();
    let sink = calls.clone();
    let caller = Caller::new(move |call| sink.borrow_mut().push(call));
    let mut built = None;
    let (handle, _) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    size(px(1000.0), px(700.0)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                let heard = cx.new(|_| Heard::new());
                let shell = cx.new(|cx| crate::chrome::shell::Shell::new(heard.clone(), caller, window, cx));
                built = Some((heard, shell.clone()));
                shell
            },
        )
        .expect("open test window")
    });
    let (heard, shell) = built.unwrap();
    let rail = cx.update(|cx| shell.read(cx).rail().clone());
    let opened = Opened { handle, heard, rail, calls };
    // A test window starts inactive, and an inactive one is told of no
    // focus change, so no focus-out would reach the rail.
    cx.update_window(handle, |_, window, _| window.activate_window()).unwrap();
    cx.run_until_parked();
    hear(cx, &opened, connected_and_busy());
    opened
}

pub(super) fn type_key(cx: &mut TestAppContext, opened: &Opened, key: &str) {
    let keystroke = gpui_kit::Keystroke::parse(key).unwrap();
    cx.update_window(opened.handle, |_, window, cx| window.dispatch_keystroke(keystroke, cx)).unwrap();
    cx.run_until_parked();
}

pub(super) fn picked(cx: &mut TestAppContext, opened: &Opened) -> Option<Picked> {
    cx.update(|cx| opened.rail.read(cx).picked())
}
