//! A browser's cell in the whole window: what it draws of what Rust says, what each control
//! asks, where a press on the picture lands in the page, the keys it takes and the room it reports.

use argus_desktop_core::browser::{BrowserShot, BrowsersPage, PageBrowser};
use argus_desktop_core::connection::page::{Connection, Event, Rail, Shown, Stage, Status};
use argus_desktop_core::panes::shown::workspace_grid::GridPage;
use argus_protocol::{BrowserDriver, BrowserTab};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use gpui_kit::prelude::*;
use gpui_kit::test::{TestEventExt as _, TestWindowExt as _};
use gpui_kit::{point, px, MouseButton, ScrollDelta, TestAppContext};
use image::{codecs::png::PngEncoder, ExtendedColorType, ImageEncoder as _};

mod hand;
mod reconnect;
mod strip;

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use argus_desktop_core::connection::page::Call;

use crate::connection::caller::Caller;
use crate::keys::root::tests::{asked, hear, open, open_on, press, type_text, Calls, Opened};

pub(super) const PANE: u64 = 105;

pub(super) fn shown(pane: u64, browser: bool) -> Shown {
    Shown {
        pane,
        repository: "argus".into(),
        checkout: "main".into(),
        branch: "main".into(),
        agent: false,
        browser,
        title: if browser { "Browser".into() } else { "shell".into() },
        status: "idle",
        resumed: None,
    }
}

pub(super) fn tab(id: &str, title: &str, url: &str) -> BrowserTab {
    BrowserTab { id: id.into(), url: url.into(), title: title.into() }
}

pub(super) fn browser() -> PageBrowser {
    PageBrowser {
        pane: PANE,
        owner: 1,
        agent: "claude".into(),
        url: "https://a.test/x".into(),
        title: "A page".into(),
        tabs: vec![tab("t1", "Docs", "https://docs.test/"), tab("t2", "", "https://a.test/x"), tab("t3", "", "")],
        shown: Some("t2".into()),
        following: true,
        held: false,
        driver: None,
    }
}

pub(super) fn page(browser: PageBrowser) -> Event {
    Event::Browsers(Box::new(BrowsersPage { browsers: vec![browser] }))
}

/// A Workspace whose first cell is a shell and whose second is the browser, `keyed` having the keys.
pub(super) fn workspace(keyed: u8) -> Vec<Event> {
    let row = GridPage {
        beside: vec![Some(shown(PANE, true))],
        grid: vec![vec![0, 1]],
        heights: vec![1000],
        widths: vec![vec![500, 500]],
        keyed: Some(keyed),
        min_cols: 20,
        min_rows: 6,
    };
    vec![
        Event::Status(Status { connection: Connection::Connected, detail: None, alarm: None, chime: true, host: None }),
        Event::Rail(Box::new(Rail { loaded: true, stage: Stage::Workspace, workspace_shown: true, shown: Some(shown(1, false)), row, ..Rail::default() })),
    ]
}

pub(super) fn opened(cx: &mut TestAppContext, keyed: u8, browser: PageBrowser) -> Opened {
    let opened = open(cx);
    let mut events = workspace(keyed);
    events.push(page(browser));
    hear(cx, &opened, events);
    opened
}

pub(super) fn has(cx: &mut TestAppContext, opened: &Opened, id: &str) -> bool {
    cx.update_window(opened.handle, |_, window, _| window.try_find(id.to_owned()).is_some()).unwrap()
}

pub(super) fn click(cx: &mut TestAppContext, opened: &Opened, id: &str) {
    cx.update_window(opened.handle, |_, window, cx| window.click(id.to_owned(), cx)).unwrap();
    cx.run_until_parked();
}

pub(super) fn label(cx: &mut TestAppContext, opened: &Opened, id: &str) -> Option<String> {
    cx.update_window(opened.handle, |_, window, _| window.find(id.to_owned()).label().map(str::to_owned)).unwrap()
}

/// What the cell sent of the browser, in words, with the calls so far cleared.
pub(super) fn said(opened: &Opened) -> Vec<String> {
    let calls = asked(opened).into_iter().filter(|call| call.starts_with("BrowserInput") || call.starts_with("CloseBrowser")).collect();
    opened.calls.borrow_mut().clear();
    calls
}

pub(super) fn png() -> String {
    let mut out = Vec::new();
    PngEncoder::new(&mut out).write_image(&[40u8, 90, 200, 255].repeat(8), 4, 2, ExtendedColorType::Rgba8).unwrap();
    STANDARD.encode(out)
}

#[gpui_kit::test]
fn a_browser_pane_beside_the_first_is_drawn_as_a_browser_not_a_terminal(cx: &mut TestAppContext) {
    let opened = opened(cx, 0, browser());
    for id in ["browser-105", "browser-tabs-105", "browser-bar-105", "browser-screen-105", "browser-address-105"] {
        assert!(has(cx, &opened, id), "{id}");
    }
    assert!(!has(cx, &opened, "cell-reserved-1"));
    assert!(has(cx, &opened, "cell-head-1"), "it keeps the split header");
}

#[gpui_kit::test]
fn each_control_sends_the_browsers_input_and_the_close_the_browsers_close(cx: &mut TestAppContext) {
    let opened = opened(cx, 0, browser());
    opened.calls.borrow_mut().clear();
    // First: a tab under the pointer opens its close, which moves the tabs after it.
    for (id, want) in [
        ("browser-tab-close-105-t2", "input: CloseTab { id: \"t2\" }"),
        ("browser-back-105", "input: Back"),
        ("browser-forward-105", "input: Forward"),
        ("browser-reload-105", "input: Reload"),
        ("browser-new-tab-105", "input: NewTab"),
        ("browser-tab-105-t1", "input: ShowTab { id: \"t1\" }"),
        ("browser-hold-105", "input: Hold { held: true }"),
    ] {
        click(cx, &opened, id);
        let calls = said(&opened);
        assert!(calls.len() == 1 && calls[0].contains(want) && calls[0].contains("pane: 105"), "{id}: {calls:?}");
    }
    click(cx, &opened, "browser-close-105");
    assert_eq!(said(&opened), ["CloseBrowser { pane: 105 }"]);
}

#[gpui_kit::test]
fn a_tab_is_named_by_its_title_else_its_address_else_new_tab(cx: &mut TestAppContext) {
    let opened = opened(cx, 0, browser());
    assert_eq!(label(cx, &opened, "browser-tab-105-t1").as_deref(), Some("Docs"));
    assert_eq!(label(cx, &opened, "browser-tab-105-t2").as_deref(), Some("https://a.test/x"));
    assert_eq!(label(cx, &opened, "browser-tab-105-t3").as_deref(), Some("New tab"));
    let lit = |cx: &mut TestAppContext, id: &str| cx.update_window(opened.handle, |_, window, _| window.find(id.to_owned()).selected()).unwrap();
    assert_eq!(lit(cx, "browser-tab-105-t2"), Some(true), "the shown tab is lit");
    assert_eq!(lit(cx, "browser-tab-105-t1"), Some(false));
}

#[gpui_kit::test]
fn who_has_the_browser_is_core_s_word_and_follow_agent_shows_only_while_not_following(cx: &mut TestAppContext) {
    let opened = opened(cx, 0, browser());
    assert!(!has(cx, &opened, "browser-status-agent") && !has(cx, &opened, "browser-follow-105"));
    let mut driven = browser();
    driven.driver = Some(BrowserDriver::Agent);
    driven.following = false;
    hear(cx, &opened, vec![page(driven.clone())]);
    assert!(has(cx, &opened, "browser-follow-105"));
    assert_eq!(label(cx, &opened, "browser-status-agent").as_deref(), Some("Agent driving"));
    opened.calls.borrow_mut().clear();
    click(cx, &opened, "browser-follow-105");
    assert_eq!(said(&opened), ["BrowserInput { pane: 105, input: FollowAgent }"]);
    driven.held = true;
    hear(cx, &opened, vec![page(driven)]);
    assert!(has(cx, &opened, "browser-status-held") && !has(cx, &opened, "browser-status-agent"), "held wins the ladder");
    opened.calls.borrow_mut().clear();
    click(cx, &opened, "browser-hold-105");
    assert_eq!(said(&opened), ["BrowserInput { pane: 105, input: Hold { held: false } }"], "held, the button hands it back");
}

#[gpui_kit::test]
fn with_no_tab_the_screen_says_so_and_offers_a_new_one(cx: &mut TestAppContext) {
    let mut tabless = browser();
    tabless.shown = None;
    tabless.tabs.clear();
    let opened = opened(cx, 0, tabless);
    assert!(has(cx, &opened, "browser-blank-105"));
    opened.calls.borrow_mut().clear();
    click(cx, &opened, "browser-blank-new-tab-105");
    assert_eq!(said(&opened), ["BrowserInput { pane: 105, input: NewTab }"]);
    hear(cx, &opened, vec![page(browser())]);
    assert!(!has(cx, &opened, "browser-blank-105"), "a tab came: the picture is drawn in its place");
}

pub(super) fn address(cx: &mut TestAppContext, opened: &Opened) -> String {
    cx.update_window(opened.handle, |_, window, _| window.find("browser-address-105".to_owned()).value().map(str::to_owned)).unwrap().unwrap_or_default()
}

#[gpui_kit::test]
fn the_address_shows_the_url_and_an_update_never_overwrites_one_being_typed(cx: &mut TestAppContext) {
    let opened = opened(cx, 0, browser());
    assert_eq!(address(cx, &opened), "https://a.test/x");
    let mut moved = browser();
    moved.url = "https://b.test/".into();
    hear(cx, &opened, vec![page(moved)]);
    assert_eq!(address(cx, &opened), "https://b.test/", "an idle field follows the browser");
    click(cx, &opened, "browser-address-105");
    type_text(cx, &opened, "zz");
    let mut again = browser();
    again.url = "https://c.test/".into();
    hear(cx, &opened, vec![page(again)]);
    let typed = address(cx, &opened);
    assert!(typed.contains("zz") && !typed.contains("c.test"), "{typed}");
}

#[gpui_kit::test]
fn enter_navigates_to_the_typed_address_and_escape_puts_the_url_back(cx: &mut TestAppContext) {
    let opened = opened(cx, 0, browser());
    opened.calls.borrow_mut().clear();
    click(cx, &opened, "browser-address-105");
    type_text(cx, &opened, "zz");
    press(cx, &opened, "escape");
    assert!(said(&opened).is_empty(), "Escape sends nothing");
    assert_eq!(address(cx, &opened), "https://a.test/x", "and puts the url back");
    click(cx, &opened, "browser-address-105");
    type_text(cx, &opened, "q.test");
    press(cx, &opened, "enter");
    let calls = said(&opened);
    assert!(calls.len() == 1 && calls[0].contains("Navigate { url: \"https://") && calls[0].contains("q.test"), "{calls:?}");
}

#[gpui_kit::test]
fn a_press_on_the_picture_clicks_where_it_was_aimed_in_the_pages_pixels(cx: &mut TestAppContext) {
    let opened = opened(cx, 0, browser());
    hear(cx, &opened, vec![Event::BrowserShot(BrowserShot { pane: PANE, jpeg: png(), width: 1280, height: 800 })]);
    cx.run_until_parked();
    let centre = cx.update_window(opened.handle, |_, window, _| window.find("browser-screen-105".to_owned()).bounds().center()).unwrap();
    opened.calls.borrow_mut().clear();
    cx.update_window(opened.handle, |_, window, cx| window.pointer_down(centre, MouseButton::Left, cx)).unwrap();
    cx.run_until_parked();
    let calls = said(&opened);
    let Some(at) = calls.iter().find_map(|call| call.strip_prefix("BrowserInput { pane: 105, input: Click { x: ")) else { panic!("{calls:?}") };
    let x: f64 = at.split(',').next().unwrap().parse().unwrap();
    assert!((x - 640.0).abs() < 2.0, "the middle of the box is the middle of the page: {calls:?}");
}

#[gpui_kit::test]
fn a_press_on_a_cell_that_lacks_the_keys_asks_for_them_and_one_that_has_them_asks_nothing(cx: &mut TestAppContext) {
    let opened = opened(cx, 0, browser());
    opened.calls.borrow_mut().clear();
    click(cx, &opened, "browser-reload-105");
    let calls = asked(&opened);
    assert!(calls.iter().any(|call| call == "FocusView { pane: 105 }"), "{calls:?}");
    let keyed = opened_with_keys_on_the_browser(cx);
    keyed.calls.borrow_mut().clear();
    click(cx, &keyed, "browser-reload-105");
    assert!(!asked(&keyed).iter().any(|call| call.starts_with("FocusView") || call.starts_with("Select")), "it has them already");
}

fn opened_with_keys_on_the_browser(cx: &mut TestAppContext) -> Opened {
    opened(cx, 1, browser())
}

#[gpui_kit::test]
fn the_cell_reports_the_room_it_has_once_laid_out_and_the_same_box_says_nothing_more(cx: &mut TestAppContext) {
    let opened = open(cx);
    let mut events = workspace(0);
    events.push(page(browser()));
    hear(cx, &opened, events);
    let told = |opened: &Opened| opened.calls.borrow().iter().map(|call| format!("{call:?}")).filter(|call| call.starts_with("BrowserViewport")).collect::<Vec<_>>();
    let first = told(&opened);
    assert!(!first.is_empty() && first[0].contains("pane: 105"), "{first:?}");
    opened.calls.borrow_mut().clear();
    hear(cx, &opened, vec![page(browser())]);
    assert!(told(&opened).is_empty(), "the same box says nothing again");
}

#[gpui_kit::test]
fn typed_on_the_screen_a_digit_is_the_pages_text_and_a_named_key_is_the_pages_key(cx: &mut TestAppContext) {
    let opened = opened(cx, 1, browser());
    click(cx, &opened, "browser-screen-105");
    opened.calls.borrow_mut().clear();
    press(cx, &opened, "6");
    press(cx, &opened, "enter");
    press(cx, &opened, "pagedown");
    let all = asked(&opened);
    assert!(!all.iter().any(|call| call.contains("Browsers")), "the Browsers stage is not asked for: {all:?}");
    assert_eq!(
        said(&opened),
        [
            "BrowserInput { pane: 105, input: Text { text: \"6\" } }",
            "BrowserInput { pane: 105, input: Key { key: Enter } }",
            "BrowserInput { pane: 105, input: Key { key: PageDown } }",
        ]
    );
}

#[gpui_kit::test]
fn a_key_the_page_has_no_word_for_goes_to_the_session_as_a_key_and_nothing_to_the_page(cx: &mut TestAppContext) {
    let opened = opened(cx, 1, browser());
    click(cx, &opened, "browser-screen-105");
    opened.calls.borrow_mut().clear();
    press(cx, &opened, "f9");
    let calls = asked(&opened);
    assert!(calls.iter().any(|call| call.starts_with("Key {")), "{calls:?}");
    assert!(said(&opened).is_empty());
}

#[gpui_kit::test]
fn a_reconnect_tells_the_new_daemon_the_size_with_no_box_changing(cx: &mut TestAppContext) {
    // SHELL-BROWSER-11, -18: core parks the last size again on a new connection and says when it
    // may go; the cell must ask then, though its box is the one it already reported.
    let calls: Calls = Rc::default();
    let sink = calls.clone();
    let due: Rc<Cell<Option<Duration>>> = Rc::default();
    let (answered, asking) = (due.clone(), due.clone());
    let caller = Caller::new(move |call| {
        if matches!(call, Call::BrowserViewportsDue { .. }) {
            answered.set(None);
        }
        sink.borrow_mut().push(call);
    })
    .asking_viewport(move || asking.get());
    let (events, listening) = tokio::sync::mpsc::unbounded_channel();
    let (handle, heard, shell) = open_on(cx, caller, listening);
    let opened = Opened { handle, heard, shell, calls, events };
    let mut events = workspace(0);
    events.push(page(browser()));
    hear(cx, &opened, events);
    let told = |opened: &Opened, word: &str| opened.calls.borrow().iter().filter(|call| format!("{call:?}").starts_with(word)).count();
    assert_eq!(told(&opened, "BrowserViewport {"), 1);
    opened.calls.borrow_mut().clear();
    // The connection came back: core holds the size again, and the daemon replays the browser.
    due.set(Some(Duration::ZERO));
    hear(cx, &opened, vec![page(browser())]);
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    assert_eq!(told(&opened, "BrowserViewportsDue"), 1, "{:?}", asked(&opened));
}

fn bounds(cx: &mut TestAppContext, opened: &Opened, id: &str) -> gpui_kit::Bounds<gpui_kit::Pixels> {
    cx.update_window(opened.handle, |_, window, _| window.find(id.to_owned()).bounds()).unwrap()
}

#[gpui_kit::test]
fn the_wheel_over_the_picture_scrolls_the_page_the_way_the_page_does_and_not_the_window(cx: &mut TestAppContext) {
    let opened = opened(cx, 1, browser());
    hear(cx, &opened, vec![Event::BrowserShot(BrowserShot { pane: PANE, jpeg: png(), width: 1280, height: 800 })]);
    opened.calls.borrow_mut().clear();
    // GPUI's deltas are positive upward and leftward; the page's deltaX and deltaY are the reverse.
    cx.update_window(opened.handle, |_, window, cx| window.scroll("browser-screen-105".to_owned(), ScrollDelta::Pixels(point(px(-10.0), px(-30.0))), cx)).unwrap();
    cx.run_until_parked();
    let calls = said(&opened);
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert!(calls[0].starts_with("BrowserInput { pane: 105, input: Wheel {") && calls[0].contains("dx: 10.0") && calls[0].contains("dy: 30.0"), "{calls:?}");
}

#[gpui_kit::test]
fn a_middle_click_on_a_tab_closes_it(cx: &mut TestAppContext) {
    let opened = opened(cx, 0, browser());
    opened.calls.borrow_mut().clear();
    let centre = bounds(cx, &opened, "browser-tab-105-t1").center();
    cx.update_window(opened.handle, |_, window, cx| window.pointer_up(centre, MouseButton::Middle, cx)).unwrap();
    cx.run_until_parked();
    assert_eq!(said(&opened), ["BrowserInput { pane: 105, input: CloseTab { id: \"t1\" } }"]);
}

#[gpui_kit::test]
fn the_tabs_stay_scrolled_where_they_were_when_the_browser_is_heard_again(cx: &mut TestAppContext) {
    let mut many = browser();
    many.tabs = (0..40).map(|n| tab(&format!("n{n}"), &format!("A tab number {n}"), "")).collect();
    many.shown = Some("n0".into());
    let opened = opened(cx, 0, many.clone());
    let left = |cx: &mut TestAppContext| bounds(cx, &opened, "browser-tab-105-n0").origin.x;
    let before = left(cx);
    cx.update_window(opened.handle, |_, window, cx| window.scroll("browser-tabs-scroll-105".to_owned(), ScrollDelta::Pixels(point(px(-300.0), px(0.0))), cx)).unwrap();
    let scrolled = left(cx);
    assert!(scrolled < before, "the strip scrolled: {before:?} then {scrolled:?}");
    many.title = "Another title".into();
    many.url = "https://d.test/".into();
    hear(cx, &opened, vec![page(many)]);
    assert_eq!(left(cx), scrolled, "an update does not reset it");
}

#[gpui_kit::test]
fn the_keyed_cell_shows_its_ring_only_while_the_keys_are_on_it_and_its_screen_has_the_focus(cx: &mut TestAppContext) {
    let opened = opened(cx, 1, browser());
    click(cx, &opened, "browser-screen-105");
    assert!(has(cx, &opened, "browser-ring-105"));
    let other = opened_unkeyed(cx);
    click(cx, &other, "browser-screen-105");
    assert!(!has(cx, &other, "browser-ring-105"), "focused, but the keys are the terminal's");
}

fn opened_unkeyed(cx: &mut TestAppContext) -> Opened {
    opened(cx, 0, browser())
}
