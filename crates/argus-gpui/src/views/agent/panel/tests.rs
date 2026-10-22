use std::cell::RefCell;
use std::rc::Rc;

use argus_desktop_core::connection::page::{Call, Connection, Event, Link, PageView, Status, ViewRef, ViewsPage};
use argus_protocol::{AgentView as Shown, Chip, Tone};
use gpui_kit::px;
use gpui_kit::test::{ClickOptions, TestWindowExt as _};
use gpui_kit::MouseButton;
use gpui_kit::{size, AnyWindowHandle, Bounds, TestAppContext, WindowBounds, WindowOptions};

use super::*;

struct Opened {
    handle: AnyWindowHandle,
    heard: Entity<Heard>,
    view: Entity<AgentView>,
    calls: Rc<RefCell<Vec<Call>>>,
}

fn shown(pane: u64, id: &str, from: &str) -> PageView {
    PageView {
        pane,
        from: from.into(),
        at: 0,
        refreshing: false,
        pressed: Vec::new(),
        subject: None,
        view: Shown {
            id: id.into(),
            title: format!("{id} title"),
            status: Some(Chip { label: "in progress".into(), tone: Tone::Active }),
            ..Default::default()
        },
        diagrams: Vec::new(),
        changed: Vec::new(),
        unseen: false,
    }
}

fn page(views: Vec<PageView>, open: Option<(u64, &str)>, popped: Option<(u64, &str)>) -> Event {
    let at = |(pane, id): (u64, &str)| ViewRef { pane, id: id.into() };
    Event::Views(Box::new(ViewsPage { views, open: open.map(at), popped: popped.map(at), can_press: true }))
}

fn open(cx: &mut TestAppContext, place: Place, events: Vec<Event>) -> Opened {
    cx.update(crate::window::init);
    let calls: Rc<RefCell<Vec<Call>>> = Rc::default();
    let sink = calls.clone();
    let caller = Caller::new(move |call| sink.borrow_mut().push(call));
    let mut built = None;
    let (handle, _) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(Default::default(), size(px(460.0), px(600.0))))),
                ..Default::default()
            },
            cx,
            |_, cx| {
                let heard = cx.new(|_| Heard::new());
                heard.update(cx, |heard, _| {
                    heard.hear(Event::Status(Status { connection: Connection::Connected, detail: None, alarm: None, chime: true, host: None }));
                });
                let view = cx.new(|cx| AgentView::new(place, heard.clone(), caller, cx));
                built = Some((heard, view.clone()));
                view
            },
        )
        .expect("open test window")
    });
    let (heard, view) = built.unwrap();
    let opened = Opened { handle, heard, view, calls };
    hear(cx, &opened, events);
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
}

fn there(cx: &mut TestAppContext, opened: &Opened, id: &str) -> bool {
    cx.update_window(opened.handle, |_, window, _| window.try_find(id.to_owned()).is_some()).unwrap()
}

fn click(cx: &mut TestAppContext, opened: &Opened, id: &str) {
    cx.update_window(opened.handle, |_, window, cx| window.click(id.to_owned(), cx)).unwrap();
    cx.run_until_parked();
}

fn taken(opened: &Opened) -> Vec<String> {
    opened.calls.borrow().iter().map(|call| format!("{call:?}")).collect()
}

fn two() -> Event {
    page(vec![shown(2, "ci", "claude"), shown(1, "trim", "claude")], Some((1, "trim")), None)
}

#[gpui_kit::test]
fn the_panel_has_a_tab_per_view_newest_first_and_the_chosen_one_is_lit(cx: &mut TestAppContext) {
    let opened = open(cx, Place::Panel, vec![two()]);
    let (first, second) = cx
        .update_window(opened.handle, |_, window, _| {
            (window.find("view-tab-2/ci".to_owned()).bounds(), window.find("view-tab-1/trim".to_owned()).bounds())
        })
        .unwrap();
    assert!(first.origin.x < second.origin.x, "newest first");
    let lit = |cx: &mut TestAppContext, id: &str| cx.update_window(opened.handle, |_, window, _| window.find(id.to_owned()).selected()).unwrap();
    assert_eq!(lit(cx, "view-tab-1/trim"), Some(true));
    assert_eq!(lit(cx, "view-tab-2/ci"), Some(false));
    assert!(there(cx, &opened, "views-tabs") && there(cx, &opened, "view-head"));
}

#[gpui_kit::test]
fn a_tab_is_a_request_to_open_its_view_and_its_close_a_request_to_close_it(cx: &mut TestAppContext) {
    let opened = open(cx, Place::Panel, vec![two()]);
    click(cx, &opened, "view-tab-2/ci");
    click(cx, &opened, "view-tab-close-1/trim");
    click(cx, &opened, "views-hide");
    let calls = taken(&opened);
    assert_eq!(calls.len(), 3, "{calls:?}");
    assert!(calls[0].starts_with("OpenView") && calls[0].contains("\"ci\"") && calls[0].contains("pane: 2"));
    assert!(calls[1].starts_with("CloseView") && calls[1].contains("\"trim\""));
    assert_eq!(calls[2], "HideViews");
}

#[gpui_kit::test]
fn the_view_stays_until_the_daemon_says_it_closed(cx: &mut TestAppContext) {
    let opened = open(cx, Place::Panel, vec![two()]);
    click(cx, &opened, "view-tab-close-1/trim");
    assert!(there(cx, &opened, "view-tab-1/trim"), "a close is a request");
    hear(cx, &opened, vec![page(vec![shown(2, "ci", "claude")], None, None)]);
    assert!(!there(cx, &opened, "view-tab-1/trim"));
}

#[gpui_kit::test]
fn a_view_changed_unseen_wears_a_dot_and_a_seen_one_none(cx: &mut TestAppContext) {
    let mut changed = shown(2, "ci", "claude");
    changed.unseen = true;
    let opened = open(cx, Place::Panel, vec![page(vec![changed, shown(1, "trim", "claude")], Some((1, "trim")), None)]);
    let dots = cx.update_window(opened.handle, |_, window, _| window.find_all("view-unseen".to_owned()).len()).unwrap();
    assert_eq!(dots, 1);
}

#[gpui_kit::test]
fn the_subject_link_opens_through_rust(cx: &mut TestAppContext) {
    let mut view = shown(1, "trim", "claude");
    view.subject = Some(Link { tracker: "Linear".into(), key: "ARG-4".into(), url: "https://example.com/ARG-4".into(), role: String::new() });
    let opened = open(cx, Place::Panel, vec![page(vec![view], Some((1, "trim")), None)]);
    assert!(there(cx, &opened, "view-subject"));
    click(cx, &opened, "view-subject");
    assert_eq!(taken(&opened), ["OpenUrl { url: \"https://example.com/ARG-4\" }"]);
}

#[gpui_kit::test]
fn as_a_page_it_has_the_meta_and_the_body_but_no_tabs_or_title_row(cx: &mut TestAppContext) {
    let opened = open(cx, Place::Page, vec![page(vec![shown(1, "trim", "claude")], Some((1, "trim")), Some((1, "trim")))]);
    assert!(there(cx, &opened, "view-meta") && there(cx, &opened, "view-body"));
    assert!(!there(cx, &opened, "views-tabs") && !there(cx, &opened, "view-title"));
}

#[gpui_kit::test]
fn the_page_shows_nothing_while_no_view_is_popped_out(cx: &mut TestAppContext) {
    let opened = open(cx, Place::Page, vec![page(vec![shown(1, "trim", "claude")], Some((1, "trim")), None)]);
    assert!(!there(cx, &opened, "view-body"));
    hear(cx, &opened, vec![page(vec![shown(1, "trim", "claude")], None, Some((1, "trim")))]);
    assert!(there(cx, &opened, "view-body"));
}

#[gpui_kit::test]
fn the_age_is_drawn_again_on_a_timer_so_it_does_not_go_stale(cx: &mut TestAppContext) {
    let opened = open(cx, Place::Panel, vec![two()]);
    // Counted at the notify itself: a forced frame would redraw the root whether or not the timer asked.
    let notified = Rc::new(std::cell::Cell::new(0u32));
    let count = notified.clone();
    let _watching = cx.update(|cx| cx.observe(&opened.view, move |_, _| count.set(count.get() + 1)));
    cx.executor().advance_clock(AGE_EVERY + Duration::from_secs(1));
    cx.run_until_parked();
    assert!(notified.get() >= 1, "the timer asks for a new draw");
}

#[gpui_kit::test]
fn a_middle_click_on_a_tab_closes_its_view(cx: &mut TestAppContext) {
    let opened = open(cx, Place::Panel, vec![two()]);
    cx.update_window(opened.handle, |_, window, cx| {
        window.click_with_options("view-tab-2/ci".to_owned(), ClickOptions::new().with_button(MouseButton::Middle), cx)
    })
    .unwrap();
    cx.run_until_parked();
    let calls = taken(&opened);
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert!(calls[0].starts_with("CloseView") && calls[0].contains("\"ci\"") && calls[0].contains("pane: 2"), "{calls:?}");
}

#[gpui_kit::test]
fn the_readers_place_is_kept_for_the_same_view_and_starts_at_the_top_for_another(cx: &mut TestAppContext) {
    let opened = open(cx, Place::Panel, vec![two()]);
    cx.update(|cx| {
        opened.view.update(cx, |view, _| {
            view.keep_place(Some("1/trim".into()));
            view.scroll.set_offset(point(px(0.0), px(-120.0)));
            view.keep_place(Some("1/trim".into()));
            assert_eq!(view.scroll.offset().y, px(-120.0), "the same view coming again keeps the place");
            view.keep_place(Some("2/ci".into()));
            assert_eq!(view.scroll.offset().y, px(0.0), "another view starts at the top");
        })
    });
}
