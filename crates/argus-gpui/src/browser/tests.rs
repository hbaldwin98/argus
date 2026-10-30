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
        cursor: argus_protocol::BrowserCursor::Default,
        dialog: None,
        select: None,
        sized: None,
        context: None,
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


/// The sub-rows under the Browsers item are gone (the browsers are pane rows in the tree, wearing
/// the globe); the item says how many are open as a badge, and none as no badge.
#[gpui_kit::test]
fn the_browsers_item_counts_the_open_browsers_and_a_press_goes_to_their_stage(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, on_stage(Connection::Connected));
    assert!(!has(cx, &opened, "nav-browsers-badge"), "no browser, no badge");
    hear(cx, &opened, vec![two()]);
    assert!(has(cx, &opened, "nav-browsers-badge"));
    let said = cx.update_window(opened.handle, |_, window, _| window.find("nav-browsers".to_owned()).label().map(str::to_owned)).unwrap();
    assert_eq!(said.as_deref(), Some("Browsers, 2 open"));
    click(cx, &opened, "nav-browsers");
    assert_eq!(asked(&opened), ["ShowBrowsers { pane: None }"]);
}

/// SHELL-BROWSER-9: a tile says who has the browser, and nothing when nobody does.
#[gpui_kit::test]
fn a_tile_says_who_has_the_browser_and_says_nothing_when_nobody_does(cx: &mut TestAppContext) {
    let opened = open(cx);
    let mut events = on_stage(Connection::Connected);
    let mut driven = browser(106, "screens", true);
    driven.driver = Some(argus_protocol::BrowserDriver::Agent);
    events.push(Event::Browsers(Box::new(BrowsersPage { browsers: vec![browser(105, "trim", true), driven] })));
    hear(cx, &opened, events);
    assert!(!has(cx, &opened, "browser-tile-105-status"));
    assert!(has(cx, &opened, "browser-tile-106-status"));
}

fn browsers_docked() -> Event {
    use argus_desktop_core::layout::docks::{DockPage, DockTab, Panel, PanelKind, PresentPanel};
    let panel = Panel::Browsers;
    let mut docks = crate::layout::work::tests::docks();
    docks.right = DockPage { tabs: vec![DockTab { panel: panel.clone(), title: "Browsers".into() }], shown: Some(panel.clone()), size: 400 };
    docks.present.push(PresentPanel { panel, kind: PanelKind::Browsers, floats: true, stage: true, closes: false, cell: true });
    Event::Docks(Box::new(docks))
}

#[gpui_kit::test]
fn the_stage_carries_the_menu_that_docks_its_browsers(cx: &mut TestAppContext) {
    let opened = open(cx);
    let mut events = on_stage(Connection::Connected);
    events.push(two());
    hear(cx, &opened, events);
    assert!(has(cx, &opened, "browsers-dock"));
    assert!(!has(cx, &opened, "browsers-grid-panel"), "no dock shows them yet");
}

#[gpui_kit::test]
fn a_dock_showing_the_browsers_panel_draws_every_browser_beside_the_work(cx: &mut TestAppContext) {
    let opened = open(cx);
    let status = Event::Status(Status { connection: Connection::Connected, detail: None, alarm: None, chime: true, host: None });
    hear(cx, &opened, vec![status, two(), browsers_docked()]);
    assert!(has(cx, &opened, "browsers-grid-panel"));
    assert!(!has(cx, &opened, "browsers-grid"), "the stage is the workspace's, not the browsers'");
    assert!(!has(cx, &opened, "browsers-dock"), "the panel's own dock head moves it");
    click(cx, &opened, "browser-tile-106");
    assert_eq!(asked(&opened), ["ShowBrowsers { pane: Some(106) }"]);
}
