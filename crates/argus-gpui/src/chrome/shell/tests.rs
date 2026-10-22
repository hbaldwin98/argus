use super::*;
use argus_desktop_core::chrome::strip::Says;
use argus_desktop_core::system::clock::Clock;
use crate::keys::root::tests::open;
use argus_desktop_core::connection::page::{Connection, Event, Frame, FrameCursor, Rail, Shown, Status};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{px, size, TestAppContext};

fn shown(title: &str, status: &'static str) -> Shown {
    Shown {
        pane: 1,
        repository: "argus".into(),
        checkout: "main".into(),
        branch: "main".into(),
        agent: true,
        browser: false,
        title: title.into(),
        status,
        resumed: None,
    }
}

fn clock() -> Clock {
    Clock::local()
}

fn title(heard: &Heard) -> String {
    Shell::strip(heard, &Says::default(), &clock()).title
}

#[test]
fn the_header_names_the_pane_on_screen_and_says_nothing_with_none() {
    let mut heard = Heard::new();
    assert_eq!(title(&heard), "", "the page's strip is empty with no pane");
    heard.hear(Event::Rail(Box::new(Rail { shown: Some(shown("Trim context", "working")), ..Rail::default() })));
    assert_eq!(title(&heard), "Trim context");
}

#[test]
fn git_is_named_by_its_title_alone() {
    let mut heard = Heard::new();
    heard.hear(Event::Rail(Box::new(Rail { shown: Some(shown("Trim context", "working")), ..Rail::default() })));
    let open = heard.open();
    heard.git.enter(open, None);
    let strip = Shell::strip(&heard, &Says::default(), &clock());
    assert_eq!((strip.title.as_str(), strip.crumb.is_empty(), strip.pane), ("Git", true, None));
}

fn grid(rows: u16, cols: u16) -> Event {
    Event::Frame(Frame {
        pane: 1,
        side: Side::Workspace,
        rows,
        cols,
        full: true,
        cursor: FrameCursor { row: 0, col: 0, visible: true, shape: "block" },
        alternate_screen: false,
        mouse: false,
        lines: Vec::new(),
        parked: None,
    })
}

fn size_said(cx: &mut TestAppContext, handle: gpui_kit::AnyWindowHandle) -> Option<String> {
    cx.update_window(handle, |_, window, _| window.try_find("head-size".to_owned()).and_then(|found| found.label().map(str::to_owned)))
        .unwrap()
}

fn hear(cx: &mut TestAppContext, opened: &crate::keys::root::tests::Opened, events: Vec<Event>) {
    opened.events.send(events).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn the_header_says_the_grid_just_after_a_resize_and_only_in_a_wide_window(cx: &mut TestAppContext) {
    let opened = open(cx);
    cx.simulate_window_resize(opened.handle, size(px(1300.0), px(700.0)));
    let status = Event::Status(Status { connection: Connection::Connected, detail: None, alarm: None, chime: true, host: None });
    let rail = Event::Rail(Box::new(Rail { loaded: true, shown: Some(shown("Trim context", "working")), ..Rail::default() }));
    hear(cx, &opened, vec![status, rail, grid(24, 80)]);
    assert_eq!(size_said(cx, opened.handle), None, "the first grid is nobody's resize");
    hear(cx, &opened, vec![grid(30, 100)]);
    assert_eq!(size_said(cx, opened.handle).as_deref(), Some("100 \u{d7} 30"));
    // `.head-size` is gone under 1100 px.
    cx.simulate_window_resize(opened.handle, size(px(1000.0), px(700.0)));
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.run_until_parked();
    assert_eq!(size_said(cx, opened.handle), None);
    // The page's `max-width: 1100px` hides it at exactly 1100 px too.
    cx.simulate_window_resize(opened.handle, size(px(1100.0), px(700.0)));
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.run_until_parked();
    assert_eq!(size_said(cx, opened.handle), None);
    cx.simulate_window_resize(opened.handle, size(px(1300.0), px(700.0)));
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.run_until_parked();
    // At rest the words go, and the header with them.
    cx.executor().advance_clock(crate::panes::size_label::SHOWN_FOR + std::time::Duration::from_millis(100));
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    assert_eq!(size_said(cx, opened.handle), None);
}

#[gpui_kit::test]
fn the_ended_bar_is_over_the_stage_while_the_shown_pane_has_ended(cx: &mut TestAppContext) {
    let opened = open(cx);
    let status = Event::Status(Status { connection: Connection::Connected, detail: None, alarm: None, chime: true, host: None });
    let ended = |status: &'static str| Event::Rail(Box::new(Rail { loaded: true, shown: Some(shown("Trim context", status)), ..Rail::default() }));
    let up = |cx: &mut TestAppContext| cx.update_window(opened.handle, |_, window, _| window.try_find("ended".to_owned()).is_some()).unwrap();
    hear(cx, &opened, vec![status, ended("working")]);
    assert!(!up(cx));
    hear(cx, &opened, vec![ended("exited")]);
    assert!(up(cx));
}

fn one_view() -> Event {
    use argus_desktop_core::connection::page::{PageView, ViewsPage};
    let view = PageView {
        pane: 1,
        from: "claude".into(),
        at: 0,
        refreshing: false,
        pressed: Vec::new(),
        subject: None,
        view: argus_protocol::AgentView { id: "trim".into(), title: "Trim".into(), ..Default::default() },
        diagrams: Vec::new(),
        changed: Vec::new(),
        unseen: false,
    };
    Event::Views(Box::new(ViewsPage { views: vec![view], open: None, popped: None, can_press: true }))
}

#[gpui_kit::test]
fn the_views_launcher_is_lit_while_a_dock_shows_the_views_panel(cx: &mut TestAppContext) {
    // AGENTVIEWS-2: aria-pressed follows whether the panel is out, as Rust's `head::out` says.
    let opened = open(cx);
    let status = Event::Status(Status { connection: Connection::Connected, detail: None, alarm: None, chime: true, host: None });
    let lit = |cx: &mut TestAppContext| cx.update_window(opened.handle, |_, window, _| window.find("views-tab".to_owned()).selected()).unwrap();
    let mut docks = crate::layout::work::tests::docks();
    docks.right.shown = None;
    hear(cx, &opened, vec![status, one_view(), Event::Docks(Box::new(docks))]);
    assert_eq!(lit(cx), Some(false), "the panel is not out");
    hear(cx, &opened, vec![Event::Docks(Box::new(crate::layout::work::tests::docks()))]);
    assert_eq!(lit(cx), Some(true), "a dock shows the views");
}
