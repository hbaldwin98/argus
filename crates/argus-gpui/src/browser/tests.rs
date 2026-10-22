//! The Browsers stage in the whole window: which tiles it draws, what a press asks, what it says
//! with none, and that a picture heard reaches the place that draws it.

use argus_desktop_core::browser::{BrowserShot, BrowsersPage, PageBrowser};
use argus_desktop_core::connection::page::{Connection, Event, Rail, Stage, Status};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::prelude::*;
use gpui_kit::TestAppContext;
use image::{codecs::png::PngEncoder, ExtendedColorType, ImageEncoder as _};

use crate::keys::root::tests::{asked, open, Opened};

fn browser(pane: u64, agent: &str, tab: bool) -> PageBrowser {
    PageBrowser {
        pane,
        owner: pane - 100,
        agent: agent.into(),
        url: format!("https://{agent}.test/"),
        title: String::new(),
        tabs: Vec::new(),
        shown: tab.then(|| "t1".into()),
        following: true,
        held: false,
        driver: None,
    }
}

fn on_stage(connection: Connection) -> Vec<Event> {
    vec![
        Event::Status(Status { connection, detail: None, alarm: None, chime: true, host: None }),
        Event::Rail(Box::new(Rail { loaded: true, stage: Stage::Browsers, ..Rail::default() })),
    ]
}

fn hear(cx: &mut TestAppContext, opened: &Opened, events: Vec<Event>) {
    opened.events.send(events).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

fn has(cx: &mut TestAppContext, opened: &Opened, id: &str) -> bool {
    cx.update_window(opened.handle, |_, window, _| window.try_find(id.to_owned()).is_some()).unwrap()
}

fn click(cx: &mut TestAppContext, opened: &Opened, id: &str) {
    cx.update_window(opened.handle, |_, window, cx| window.click(id.to_owned(), cx)).unwrap();
    cx.run_until_parked();
}

fn two() -> Event {
    Event::Browsers(Box::new(BrowsersPage { browsers: vec![browser(105, "trim", true), browser(106, "screens", true)] }))
}

fn png() -> String {
    let mut out = Vec::new();
    PngEncoder::new(&mut out).write_image(&[40u8, 90, 200, 255].repeat(8), 4, 2, ExtendedColorType::Rgba8).unwrap();
    STANDARD.encode(out)
}

#[gpui_kit::test]
fn with_no_browser_the_stage_says_how_one_is_opened(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, on_stage(Connection::Connected));
    assert!(has(cx, &opened, "browsers-empty"));
    assert!(!has(cx, &opened, "browsers-grid"));
}

#[gpui_kit::test]
fn each_browser_is_a_tile_in_the_order_rust_lists_them(cx: &mut TestAppContext) {
    let opened = open(cx);
    let mut events = on_stage(Connection::Connected);
    events.push(two());
    hear(cx, &opened, events);
    assert!(!has(cx, &opened, "browsers-empty"));
    let (first, second) = cx
        .update_window(opened.handle, |_, window, _| {
            (window.find("browser-tile-105".to_owned()).bounds().origin, window.find("browser-tile-106".to_owned()).bounds().origin)
        })
        .unwrap();
    assert!(first.x < second.x || first.y < second.y, "{first:?} then {second:?}");
    let said = cx.update_window(opened.handle, |_, window, _| window.find("browser-tile-106".to_owned()).label().map(str::to_owned)).unwrap();
    assert_eq!(said.as_deref(), Some("Open screens's browser"));
}

#[gpui_kit::test]
fn a_press_on_a_tile_asks_to_show_that_browsers_pane_and_the_last_browser_going_empties_the_stage(cx: &mut TestAppContext) {
    let opened = open(cx);
    let mut events = on_stage(Connection::Connected);
    events.push(two());
    hear(cx, &opened, events);
    click(cx, &opened, "browser-tile-106");
    assert_eq!(asked(&opened), ["ShowBrowsers { pane: Some(106) }"]);
    hear(cx, &opened, vec![Event::Browsers(Box::default())]);
    assert!(has(cx, &opened, "browsers-empty"), "nothing is predicted: the stage follows what the session says");
}

#[gpui_kit::test]
fn a_tile_pressed_while_the_daemon_is_gone_asks_nothing(cx: &mut TestAppContext) {
    let opened = open(cx);
    let mut events = on_stage(Connection::Connected);
    events.push(two());
    hear(cx, &opened, events);
    hear(cx, &opened, vec![Event::Status(Status { connection: Connection::Connecting, detail: None, alarm: None, chime: true, host: None })]);
    click(cx, &opened, "browser-tile-105");
    assert!(asked(&opened).is_empty());
}

#[gpui_kit::test]
fn a_picture_heard_is_held_for_every_place_and_goes_with_the_tab(cx: &mut TestAppContext) {
    let opened = open(cx);
    let mut events = on_stage(Connection::Connected);
    events.push(two());
    events.push(Event::BrowserShot(BrowserShot { pane: 105, jpeg: png(), width: 1280, height: 800 }));
    hear(cx, &opened, events);
    let decoded = |cx: &mut TestAppContext| cx.read_entity(&opened.heard, |heard, _| heard.pictures.of(105).map(|picture| (picture.width, picture.height)));
    assert_eq!(decoded(cx), Some((1280, 800)));
    hear(cx, &opened, vec![Event::Browsers(Box::new(BrowsersPage { browsers: vec![browser(105, "trim", false), browser(106, "screens", true)] }))]);
    assert_eq!(decoded(cx), None, "a browser with no tab shows nothing, not that tab's last page");
}
