//! The Browsers stage in the whole window: which tiles it draws, what a press asks, what it says
//! with none, and that a picture heard reaches the place that draws it.

use argus_desktop_core::browser::{BrowserShot, BrowsersPage, PageBrowser};
use argus_desktop_core::connection::page::{Connection, Event, Rail, Shown, Stage, Status};
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

fn top(cx: &mut TestAppContext, opened: &Opened, id: &str) -> f32 {
    cx.update_window(opened.handle, |_, window, _| f32::from(window.find(id.to_owned()).bounds().origin.y)).unwrap()
}

/// SHELL-BROWSER-14: one sub-row under the Browsers item for each browser, in the stage's order.
#[gpui_kit::test]
fn each_browser_is_a_sub_row_of_the_nav_in_the_stages_order_and_a_press_shows_its_pane(cx: &mut TestAppContext) {
    let opened = open(cx);
    let mut events = on_stage(Connection::Connected);
    events.push(two());
    hear(cx, &opened, events);
    assert!(has(cx, &opened, "nav-browsers-fold"));
    let (item, first, second) = (top(cx, &opened, "nav-browsers"), top(cx, &opened, "nav-browser-105"), top(cx, &opened, "nav-browser-106"));
    assert!(item < first && first < second, "{item} {first} {second}");
    let said = cx.update_window(opened.handle, |_, window, _| window.find("nav-browser-106".to_owned()).label().map(str::to_owned)).unwrap();
    assert_eq!(said.as_deref(), Some("screens \u{b7} https://screens.test/"));
    click(cx, &opened, "nav-browser-106");
    assert_eq!(asked(&opened), ["ShowBrowsers { pane: Some(106) }"]);
}

/// SHELL-STAGE-17: the chevron folds the list and does not press the item under it.
#[gpui_kit::test]
fn the_chevron_folds_the_open_browsers_and_presses_nothing(cx: &mut TestAppContext) {
    let opened = open(cx);
    let mut events = on_stage(Connection::Connected);
    events.push(two());
    hear(cx, &opened, events);
    click(cx, &opened, "nav-browsers-fold");
    hear(cx, &opened, Vec::new());
    assert!(!has(cx, &opened, "nav-browser-105"), "folded");
    assert!(asked(&opened).is_empty(), "the item was not pressed: {:?}", asked(&opened));
    click(cx, &opened, "nav-browsers-fold");
    hear(cx, &opened, Vec::new());
    assert!(has(cx, &opened, "nav-browser-105"), "unfolded again");
}

#[gpui_kit::test]
fn with_no_browser_the_nav_item_has_no_fold_and_no_rows(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, on_stage(Connection::Connected));
    assert!(!has(cx, &opened, "nav-browsers-fold"));
    assert!(!has(cx, &opened, "nav-browsers-list"));
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

fn shown_browser(pane: u64) -> Shown {
    Shown {
        pane,
        repository: "argus".into(),
        checkout: "main".into(),
        branch: "dev".into(),
        agent: true,
        browser: true,
        title: "screens".into(),
        status: "idle",
        resumed: None,
    }
}

fn lit(cx: &mut TestAppContext, opened: &Opened, id: &str) -> bool {
    cx.update_window(opened.handle, |_, window, _| window.find(id.to_owned()).selected()).unwrap().unwrap_or(false)
}

fn railing(stage: Stage, shown: Option<Shown>, beside: Vec<Option<Shown>>) -> Event {
    let mut rail = Rail { loaded: true, stage, shown, ..Rail::default() };
    rail.row.beside = beside;
    Event::Rail(Box::new(rail))
}

/// SHELL-BROWSER-14: a sub-row is lit while the Workspace is up and its pane is in the grid, the
/// shown pane or one beside it; on another stage none is lit.
#[gpui_kit::test]
fn a_sub_row_is_lit_only_on_the_workspace_and_only_for_a_pane_in_its_grid(cx: &mut TestAppContext) {
    let opened = open(cx);
    let connected = on_stage(Connection::Connected).remove(0);
    hear(cx, &opened, vec![connected.clone(), railing(Stage::Workspace, Some(shown_browser(106)), Vec::new()), two()]);
    assert!(lit(cx, &opened, "nav-browser-106"), "the shown pane");
    assert!(!lit(cx, &opened, "nav-browser-105"));
    hear(cx, &opened, vec![railing(Stage::Workspace, None, vec![Some(shown_browser(106))])]);
    assert!(lit(cx, &opened, "nav-browser-106"), "a pane beside");
    assert!(!lit(cx, &opened, "nav-browser-105"));
    hear(cx, &opened, vec![railing(Stage::Browsers, Some(shown_browser(106)), vec![Some(shown_browser(105))])]);
    assert!(!lit(cx, &opened, "nav-browser-106"), "the Browsers stage lights none");
    assert!(!lit(cx, &opened, "nav-browser-105"));
}

/// SHELL-BROWSER-14 and the page's `.nav-sub .ttl`: the name is `.grow` and the title keeps its words,
/// so a row too narrow for both loses the name first, and the title is cut at the row's edge.
#[gpui_kit::test]
fn a_narrow_sub_row_cuts_the_name_before_the_title_and_the_title_at_the_edge(cx: &mut TestAppContext) {
    let opened = open(cx);
    let mut short = browser(105, "trim", true);
    short.title = "Docs".into();
    let mut long = browser(106, "screens", true);
    long.title = "A page whose title is far too long to share the sidebar with anybody's name at all".into();
    let mut events = on_stage(Connection::Connected);
    events.push(Event::Browsers(Box::new(BrowsersPage { browsers: vec![short, long] })));
    hear(cx, &opened, events);
    let bounds = |cx: &mut TestAppContext, id: &str| cx.update_window(opened.handle, |_, window, _| window.find(id.to_owned()).bounds()).unwrap();
    let (row, name, meta) = (bounds(cx, "nav-browser-105"), bounds(cx, "nav-browser-105-name"), bounds(cx, "nav-browser-105-meta"));
    assert!(name.size.width > gpui_kit::px(10.0) && meta.right() <= row.right(), "room for both: the name takes what the title leaves");
    let (row, name, meta) = (bounds(cx, "nav-browser-106"), bounds(cx, "nav-browser-106-name"), bounds(cx, "nav-browser-106-meta"));
    assert!(meta.right() <= row.right(), "the title is cut at the row's edge, not past it");
    assert!(f32::from(name.size.width) < 1.0, "and the name went first, to nothing");
}
