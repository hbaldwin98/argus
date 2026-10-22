use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use crate::chrome::stage::StageView;
use crate::connection::heard::Heard;
use argus_desktop_core::connection::page::{Checkout, Connection, Event, Status, Pane, Project, Repository, Shown};
use gpui_kit::test::{TestEventExt as _, TestWindowExt as _};
use gpui_kit::{
    px, size, AnyWindowHandle, Bounds, Context, Entity, MouseButton, TestAppContext, Window, WindowBounds,
    WindowOptions,
};

fn pane(id: u64, agent: bool, note: Option<&str>) -> Pane {
    Pane { id, agent, title: "task".into(), status: "exited", note: note.map(str::to_owned), on: true, views: 0 }
}

fn checkout(id: u64, panes: Vec<Pane>) -> Checkout {
    Checkout {
        id,
        name: "main".into(),
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

fn shown(pane: u64, status: &'static str) -> Shown {
    Shown {
        pane,
        repository: "argus".into(),
        checkout: "main".into(),
        branch: "main".into(),
        agent: true,
        browser: false,
        title: "task".into(),
        status,
        resumed: None,
    }
}

/// A tree with `panes` in checkout 110, the pane `on` on screen with `status`, and `templates` offered.
fn rail(panes: Vec<Pane>, on: u64, status: &'static str, templates: &[&str]) -> Rail {
    let repository = Repository {
        id: 11,
        name: "argus".into(),
        quiet: false,
        divider: false,
        open: true,
        panes: panes.len(),
        status: None,
        used: None,
        checkouts: vec![checkout(110, panes)],
    };
    Rail {
        loaded: true,
        project: Some(Project { id: 1, name: "src".into(), needs_you: 0, repositories: vec![repository], running: Vec::new() }),
        shown: Some(shown(on, status)),
        templates: templates.iter().map(|name| (*name).to_owned()).collect(),
        ..Rail::default()
    }
}

#[test]
fn a_pane_that_has_not_ended_has_no_bar() {
    assert_eq!(of(&rail(vec![pane(7, true, None)], 7, "working", &["claude"])), None);
    assert_eq!(of(&Rail::default()), None, "nothing on screen");
}

#[test]
fn an_ended_agent_offers_the_first_template_and_its_note() {
    let ended = of(&rail(vec![pane(7, true, Some("exit 1"))], 7, "exited", &["codex", "claude"])).unwrap();
    assert_eq!(ended.pane, 7);
    assert_eq!(ended.note.as_deref(), Some("exit 1"));
    let start = ended.start.unwrap();
    assert_eq!(start, Start { checkout: 110, template: Some("codex".into()) });
    assert_eq!(start.words(), "Start codex here");
    assert!(matches!(start.call(), Call::SpawnAgent { checkout: 110, template } if template == "codex"));
}

#[test]
fn an_ended_shell_offers_a_shell_whatever_agents_the_daemon_offers() {
    let ended = of(&rail(vec![pane(7, false, None)], 7, "exited", &["claude"])).unwrap();
    let start = ended.start.unwrap();
    assert_eq!(start.words(), "New shell here");
    assert!(matches!(start.call(), Call::SpawnShell { checkout: 110 }));
}

#[test]
fn an_ended_agent_with_no_template_on_offer_starts_a_shell() {
    let start = of(&rail(vec![pane(7, true, None)], 7, "exited", &[])).unwrap().start.unwrap();
    assert_eq!(start.words(), "New shell here");
    assert!(matches!(start.call(), Call::SpawnShell { checkout: 110 }));
}

#[test]
fn a_pane_not_in_the_tree_can_only_be_closed() {
    let ended = of(&rail(vec![pane(8, true, Some("elsewhere"))], 7, "exited", &["claude"])).unwrap();
    assert_eq!(ended, Ended { pane: 7, note: None, start: None });
}

struct Blank;

impl Render for Blank {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui_kit::div()
    }
}

struct Opened {
    handle: AnyWindowHandle,
    heard: Entity<Heard>,
    calls: Rc<RefCell<Vec<Call>>>,
}

fn open(cx: &mut TestAppContext) -> Opened {
    cx.update(crate::window::init);
    let calls: Rc<RefCell<Vec<Call>>> = Rc::default();
    let sink = calls.clone();
    let caller = Caller::new(move |call| sink.borrow_mut().push(call));
    let mut heard_out = None;
    let (handle, _) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(Default::default(), size(px(800.0), px(400.0))))),
                ..Default::default()
            },
            cx,
            |_, cx| {
                let heard = cx.new(|_| Heard::new());
                heard_out = Some(heard.clone());
                let (pane, git, checkouts, code) = (cx.new(|_| Blank), cx.new(|_| Blank), cx.new(|_| Blank), cx.new(|_| Blank));
                cx.new(|cx| StageView::new(heard, caller, pane.into(), git.into(), checkouts.into(), code.into(), cx))
            },
        )
        .expect("open test window")
    });
    let opened = Opened { handle, heard: heard_out.unwrap(), calls };
    // A window that has heard a daemon: the sidebar is not held.
    hear(cx, &opened, vec![Event::Status(Status { connection: Connection::Connected, detail: None, alarm: None, chime: true, host: None })]);
    opened
}

fn hear(cx: &mut TestAppContext, opened: &Opened, events: Vec<Event>) {
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
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

fn press(cx: &mut TestAppContext, opened: &Opened, id: &'static str) {
    let at = cx.update_window(opened.handle, |_, window, _| window.find(id).bounds().center()).unwrap();
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(at, MouseButton::Left, cx);
        window.pointer_up(at, MouseButton::Left, cx);
    })
    .unwrap();
    cx.run_until_parked();
}

fn has(cx: &mut TestAppContext, opened: &Opened, id: &'static str) -> bool {
    cx.update_window(opened.handle, |_, window, _| window.try_find(id.to_owned()).is_some()).unwrap()
}

#[gpui_kit::test]
fn the_bar_is_up_while_the_shown_pane_has_ended_and_not_otherwise(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![Event::Rail(Box::new(rail(vec![pane(7, true, None)], 7, "working", &["claude"])))]);
    assert!(!has(cx, &opened, "ended"));
    hear(cx, &opened, vec![Event::Rail(Box::new(rail(vec![pane(7, true, None)], 7, "exited", &["claude"])))]);
    assert!(has(cx, &opened, "ended"));
    hear(cx, &opened, vec![Event::Rail(Box::new(rail(vec![pane(7, true, None)], 7, "idle", &["claude"])))]);
    assert!(!has(cx, &opened, "ended"));
}

#[gpui_kit::test]
fn start_asks_for_the_new_pane_and_close_for_the_end_of_this_one(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![Event::Rail(Box::new(rail(vec![pane(7, true, None)], 7, "exited", &["claude"])))]);
    press(cx, &opened, "ended-start");
    assert!(matches!(&opened.calls.borrow()[..], [Call::SpawnAgent { checkout: 110, template }] if template == "claude"), "{:?}", opened.calls.borrow());
    press(cx, &opened, "ended-close");
    assert!(matches!(opened.calls.borrow()[1], Call::Close { pane: 7 }), "{:?}", opened.calls.borrow());
}

#[gpui_kit::test]
fn a_pane_out_of_the_tree_offers_close_alone(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![Event::Rail(Box::new(rail(vec![pane(8, true, None)], 7, "exited", &["claude"])))]);
    assert!(has(cx, &opened, "ended"));
    assert!(!has(cx, &opened, "ended-start"));
    assert!(has(cx, &opened, "ended-close"));
}

#[gpui_kit::test]
fn nothing_is_asked_while_the_daemon_is_gone(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(
        cx,
        &opened,
        vec![
            Event::Status(Status { connection: Connection::Stopped, detail: None, alarm: None, chime: true, host: None }),
            Event::Rail(Box::new(rail(vec![pane(7, true, None)], 7, "exited", &["claude"]))),
        ],
    );
    press(cx, &opened, "ended-start");
    press(cx, &opened, "ended-close");
    assert!(opened.calls.borrow().is_empty(), "{:?}", opened.calls.borrow());
}
