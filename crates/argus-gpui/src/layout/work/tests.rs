use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::*;
use gpui_kit::px;
use argus_desktop_core::connection::page::{Connection, Event, Status};
use argus_desktop_core::panes::console::{ConsolePage, ConsolePlace};
use argus_desktop_core::layout::docks::{DockPage, DockTab, DocksPage, PanelKind, PresentPanel};
use gpui_kit::test::{ClickOptions, TestEventExt as _, TestQueryExt as _, TestWindowExt as _};
use gpui_kit::{
    point, size, AnyWindowHandle, Bounds, MouseButton, Render, TestAppContext, WindowBounds, WindowOptions,
};

const WIDTH: f32 = 1000.0;
const HEIGHT: f32 = 700.0;
const WIDE: f32 = 1400.0;

pub(crate) type Calls = Rc<RefCell<Vec<Call>>>;

/// A stage with keys of its own, standing for the pane.
pub(crate) struct Stub {
    focus: FocusHandle,
    /// The releases that reached the stage, as they would reach a button on the pane.
    ups: Rc<Cell<u32>>,
}

impl Render for Stub {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let ups = self.ups.clone();
        div().id("stub").track_focus(&self.focus).size_full().on_mouse_up(MouseButton::Left, move |_, _, _| ups.set(ups.get() + 1))
    }
}

pub(crate) struct Opened {
    pub(crate) handle: AnyWindowHandle,
    pub(crate) heard: Entity<Heard>,
    pub(crate) stub: Entity<Stub>,
    pub(crate) ups: Rc<Cell<u32>>,
    pub(crate) calls: Calls,
}

pub(crate) fn tab(panel: &Panel, title: &str) -> DockTab {
    DockTab { panel: panel.clone(), title: title.into() }
}

pub(crate) fn present(panel: &Panel, kind: PanelKind, stage: bool) -> PresentPanel {
    PresentPanel { panel: panel.clone(), kind, floats: false, stage, closes: false, cell: false }
}

pub(crate) fn images() -> Panel {
    Panel::extension("docker", "images")
}

/// The views panel and an extension's view share the right dock, 400 wide,
/// with the views shown; the console is alone in the bottom dock, 200 tall;
/// the left dock holds nothing and is closed.
pub(crate) fn docks() -> DocksPage {
    let (console, views, images) = (Panel::Console, Panel::Views, images());
    DocksPage {
        left: DockPage::default(),
        right: DockPage {
            tabs: vec![tab(&views, "Views"), tab(&images, "Images")],
            shown: Some(views.clone()),
            size: 400,
        },
        bottom: DockPage { tabs: vec![tab(&console, "Console")], shown: Some(console.clone()), size: 200 },
        floating: Vec::new(),
        present: vec![
            present(&console, PanelKind::Console, true),
            present(&views, PanelKind::Views, false),
            present(&images, PanelKind::Extension, true),
        ],
    }
}

pub(crate) fn status(connection: Connection) -> Event {
    Event::Status(Status { connection, detail: None, alarm: None, chime: true, host: None })
}

pub(crate) fn console(place: ConsolePlace) -> Event {
    Event::Console(Box::new(ConsolePage { place, ..ConsolePage::default() }))
}

pub(crate) fn open(cx: &mut TestAppContext, events: Vec<Event>) -> Opened {
    open_wide(cx, events, WIDTH)
}

/// The work area in a window `width` wide: 1200 and under is narrow.
pub(crate) fn open_wide(cx: &mut TestAppContext, events: Vec<Event>, width: f32) -> Opened {
    cx.update(crate::window::init);
    let calls: Calls = Rc::default();
    let sink = calls.clone();
    let caller = Caller::new(move |call| sink.borrow_mut().push(call));
    let mut built = None;
    let ups: Rc<Cell<u32>> = Rc::default();
    let stage_ups = ups.clone();
    let (handle, _) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(Default::default(), size(px(width), px(HEIGHT))))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                let heard = cx.new(|_| Heard::new());
                let stub = cx.new(|cx| Stub { focus: cx.focus_handle(), ups: stage_ups });
                let console = cx.new(|cx| Stub { focus: cx.focus_handle(), ups: Rc::default() });
                let work = cx.new(|cx| WorkView::new(heard.clone(), caller, stub.clone().into(), console.into(), cx));
                let focus = stub.read(cx).focus.clone();
                window.focus(&focus, cx);
                built = Some((heard, stub));
                work
            },
        )
        .expect("open test window")
    });
    let (heard, stub) = built.unwrap();
    let opened = Opened { handle, heard, stub, ups, calls };
    hear(cx, &opened, events);
    opened
}

pub(crate) fn hear(cx: &mut TestAppContext, opened: &Opened, events: Vec<Event>) {
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

pub(crate) fn docked(cx: &mut TestAppContext, page: DocksPage) -> Opened {
    open(cx, vec![status(Connection::Connected), console(ConsolePlace::Docked), Event::Docks(Box::new(page))])
}

pub(crate) fn taken(opened: &Opened) -> Vec<String> {
    opened.calls.borrow().iter().map(|call| format!("{call:?}")).collect()
}

pub(crate) fn centre(cx: &mut TestAppContext, opened: &Opened, id: &str) -> Point<Pixels> {
    cx.update_window(opened.handle, |_, window, _| window.find(id.to_owned()).bounds().center()).unwrap()
}

pub(crate) fn bounds(cx: &mut TestAppContext, opened: &Opened, id: &str) -> Bounds<Pixels> {
    cx.update_window(opened.handle, |_, window, _| window.find(id.to_owned()).bounds()).unwrap()
}

/// A side dock's size as Rust says it: the page's `.dock` is content-box, so its 1px border sits outside it.
pub(crate) fn drawn_width(cx: &mut TestAppContext, opened: &Opened, id: &str) -> Pixels {
    bounds(cx, opened, id).size.width - px(1.0)
}

pub(crate) fn label(cx: &mut TestAppContext, opened: &Opened, id: &str) -> Option<String> {
    cx.update_window(opened.handle, |_, window, _| {
        window.try_find(id.to_owned()).and_then(|found| found.label().map(str::to_owned))
    })
    .unwrap()
}

pub(crate) fn present_id(cx: &mut TestAppContext, opened: &Opened, id: &str) -> bool {
    cx.update_window(opened.handle, |_, window, _| window.try_find(id.to_owned()).is_some()).unwrap()
}

pub(crate) fn selected(cx: &mut TestAppContext, opened: &Opened, id: &str) -> Option<bool> {
    cx.update_window(opened.handle, |_, window, _| window.find(id.to_owned()).selected()).unwrap()
}

pub(crate) fn by(at: Point<Pixels>, dx: f32, dy: f32) -> Point<Pixels> {
    point(at.x + px(dx), at.y + px(dy))
}

pub(crate) fn down(cx: &mut TestAppContext, opened: &Opened, at: Point<Pixels>) {
    cx.update_window(opened.handle, |_, window, cx| window.pointer_down(at, MouseButton::Left, cx)).unwrap();
    cx.run_until_parked();
}

pub(crate) fn drag_to(cx: &mut TestAppContext, opened: &Opened, at: Point<Pixels>) {
    cx.update_window(opened.handle, |_, window, cx| window.pointer_move(at, Some(MouseButton::Left), cx)).unwrap();
    cx.run_until_parked();
}

pub(crate) fn up(cx: &mut TestAppContext, opened: &Opened, at: Point<Pixels>) {
    cx.update_window(opened.handle, |_, window, cx| window.pointer_up(at, MouseButton::Left, cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

pub(crate) fn escape(cx: &mut TestAppContext, opened: &Opened) {
    cx.update_window(opened.handle, |_, window, cx| window.press("escape", cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

fn keys_are_the_stages(cx: &mut TestAppContext, opened: &Opened) -> bool {
    cx.update_window(opened.handle, |_, window, cx| opened.stub.read(cx).focus.is_focused(window)).unwrap()
}

const VIEWS_TAB: &str = "dock-tab-views";
const IMAGES_TAB: &str = "dock-tab-extension:docker/images";

#[gpui_kit::test]
fn a_docks_snapshot_draws_each_dock_with_the_right_panel_shown(cx: &mut TestAppContext) {
    let opened = docked(cx, docks());

    // The right dock holds two panels: a tab strip, the views shown.
    assert!(present_id(cx, &opened, "dock-right-tabs"));
    assert_eq!(label(cx, &opened, "dock-right-panel").as_deref(), Some("Views"));
    assert_eq!(selected(cx, &opened, VIEWS_TAB), Some(true));
    assert_eq!(selected(cx, &opened, IMAGES_TAB), Some(false));
    assert_eq!(drawn_width(cx, &opened, "dock-right"), px(400.0), "Rust's size, as the page draws it");

    // The bottom dock holds one, the console, whose own strip is its head: neither here.
    assert!(!present_id(cx, &opened, "dock-bottom-head"));
    assert!(!present_id(cx, &opened, "dock-bottom-tabs"));
    assert_eq!(label(cx, &opened, "dock-bottom-panel").as_deref(), Some("Console"));
    assert_eq!(bounds(cx, &opened, "dock-bottom").size.height - px(1.0), px(200.0), "Rust's height, the top border outside it");

    // The left dock shows nothing and takes no room.
    assert!(!present_id(cx, &opened, "dock-left"));
    assert_eq!(bounds(cx, &opened, "work-stage").origin.x, px(0.0));
}

#[gpui_kit::test]
fn the_stage_gets_what_the_docks_leave(cx: &mut TestAppContext) {
    // A window wide enough for the dock to have a column of its own.
    let opened = docked_wide(cx, docks(), WIDE);
    let stage = bounds(cx, &opened, "work-stage");
    assert_eq!(stage.size.width, px(WIDE - 400.0 - 1.0), "the dock's border is outside its size");
    assert_eq!(stage.size.height, px(HEIGHT - 200.0 - 1.0), "the bottom dock's border is outside its height too");
}

#[gpui_kit::test]
fn another_tab_shown_is_drawn_when_the_docks_event_says_so(cx: &mut TestAppContext) {
    let opened = docked(cx, docks());
    let mut page = docks();
    page.right.shown = Some(images());
    hear(cx, &opened, vec![Event::Docks(Box::new(page))]);
    assert_eq!(label(cx, &opened, "dock-right-panel").as_deref(), Some("Images"));
    assert_eq!(selected(cx, &opened, IMAGES_TAB), Some(true));
}

#[gpui_kit::test]
fn a_tab_click_sends_the_show_call_and_the_dock_shows_what_rust_says(cx: &mut TestAppContext) {
    let opened = docked(cx, docks());
    let at = centre(cx, &opened, IMAGES_TAB);
    down(cx, &opened, at);
    up(cx, &opened, at);

    assert_eq!(taken(&opened), ["Docks { gesture: Show { panel: Extension(\"docker/images\") } }"]);
    assert_eq!(label(cx, &opened, "dock-right-panel").as_deref(), Some("Views"), "nothing changes before the event");
    assert_eq!(selected(cx, &opened, IMAGES_TAB), Some(false));
}

#[gpui_kit::test]
fn a_sash_drag_sends_the_size_call_and_the_dock_does_not_resize_until_the_next_docks_event(cx: &mut TestAppContext) {
    let opened = docked(cx, docks());
    let grab = centre(cx, &opened, "dock-right-sash");
    down(cx, &opened, grab);
    drag_to(cx, &opened, by(grab, -100.0, 0.0));

    // Followed while held, as a gesture in progress.
    assert_eq!(drawn_width(cx, &opened, "dock-right"), px(500.0));
    assert_eq!(taken(&opened), Vec::<String>::new(), "nothing is sent while the sash is held");

    up(cx, &opened, by(grab, -100.0, 0.0));
    assert_eq!(taken(&opened), ["Docks { gesture: Size { dock: Right, size: 500 } }"]);
    assert_eq!(drawn_width(cx, &opened, "dock-right"), px(400.0), "Rust's size until Rust answers");

    let mut page = docks();
    page.right.size = 500;
    hear(cx, &opened, vec![Event::Docks(Box::new(page))]);
    assert_eq!(drawn_width(cx, &opened, "dock-right"), px(500.0));
}

#[gpui_kit::test]
fn a_sash_let_go_where_it_began_still_asks_and_the_dock_stays_rusts_size(cx: &mut TestAppContext) {
    // SHELL-DOCK-13: every release of a dock sash that was dragged reports its size, so Rust
    // always has the word; the dock is as big as Rust says whether or not it answers.
    let opened = docked(cx, docks());
    let grab = centre(cx, &opened, "dock-right-sash");
    down(cx, &opened, grab);
    drag_to(cx, &opened, by(grab, -80.0, 0.0));
    drag_to(cx, &opened, grab);
    up(cx, &opened, grab);
    assert_eq!(taken(&opened), ["Docks { gesture: Size { dock: Right, size: 400 } }"]);
    assert_eq!(drawn_width(cx, &opened, "dock-right"), px(400.0));
}

#[gpui_kit::test]
fn a_sash_dragged_past_rusts_bounds_is_sent_and_drawn_within_them(cx: &mut TestAppContext) {
    let opened = docked(cx, docks());
    let grab = centre(cx, &opened, "dock-right-sash");
    down(cx, &opened, grab);
    drag_to(cx, &opened, by(grab, 300.0, 0.0));
    assert_eq!(drawn_width(cx, &opened, "dock-right"), px(280.0), "no narrower than a side dock may be");
    up(cx, &opened, by(grab, 300.0, 0.0));
    assert_eq!(taken(&opened), ["Docks { gesture: Size { dock: Right, size: 280 } }"]);
}

#[gpui_kit::test]
fn the_bottom_sash_sizes_the_height(cx: &mut TestAppContext) {
    let opened = docked(cx, docks());
    let grab = centre(cx, &opened, "dock-bottom-sash");
    down(cx, &opened, grab);
    drag_to(cx, &opened, by(grab, 0.0, -60.0));
    up(cx, &opened, by(grab, 0.0, -60.0));
    assert_eq!(taken(&opened), ["Docks { gesture: Size { dock: Bottom, size: 260 } }"]);
}

#[gpui_kit::test]
fn a_sash_press_that_stays_under_five_pixels_sends_nothing(cx: &mut TestAppContext) {
    let opened = docked(cx, docks());
    let grab = centre(cx, &opened, "dock-right-sash");
    down(cx, &opened, grab);
    drag_to(cx, &opened, by(grab, -3.0, 0.0));
    assert_eq!(drawn_width(cx, &opened, "dock-right"), px(400.0), "not yet a drag");
    up(cx, &opened, by(grab, -3.0, 0.0));
    assert!(taken(&opened).is_empty());
}

#[gpui_kit::test]
fn escape_during_a_sash_drag_restores_rusts_size_and_sends_nothing(cx: &mut TestAppContext) {
    let opened = docked(cx, docks());
    let grab = centre(cx, &opened, "dock-right-sash");
    down(cx, &opened, grab);
    drag_to(cx, &opened, by(grab, -100.0, 0.0));
    assert_eq!(drawn_width(cx, &opened, "dock-right"), px(500.0));
    escape(cx, &opened);
    up(cx, &opened, by(grab, -100.0, 0.0));
    assert_eq!(drawn_width(cx, &opened, "dock-right"), px(400.0));
    assert!(taken(&opened).is_empty());
}

#[gpui_kit::test]
fn a_drag_under_five_pixels_is_a_click_and_over_it_is_a_drag(cx: &mut TestAppContext) {
    let opened = docked(cx, docks());
    let at = centre(cx, &opened, IMAGES_TAB);

    // 4.2 px: the tab is clicked, not carried.
    down(cx, &opened, at);
    drag_to(cx, &opened, by(at, 3.0, 3.0));
    assert!(!present_id(cx, &opened, "drag-ghost"));
    up(cx, &opened, by(at, 3.0, 3.0));
    assert_eq!(taken(&opened), ["Docks { gesture: Show { panel: Extension(\"docker/images\") } }"]);

    // 5 px: carried; the release is a drop and never also a click.
    opened.calls.borrow_mut().clear();
    down(cx, &opened, at);
    drag_to(cx, &opened, by(at, -5.0, 0.0));
    assert!(present_id(cx, &opened, "drag-ghost"), "the panel's name follows the pointer");
    assert_eq!(label(cx, &opened, "drag-ghost").as_deref(), Some("Images"));
    up(cx, &opened, by(at, -5.0, 0.0));
    assert!(
        !taken(&opened).iter().any(|call| call.contains("Show")),
        "a drag ended on the tab it began on was heard as a click: {:?}",
        taken(&opened)
    );
    assert!(!present_id(cx, &opened, "drag-ghost"), "the ghost goes with the drag");
}

#[gpui_kit::test]
fn a_drag_previews_where_it_would_land_under_the_pointer(cx: &mut TestAppContext) {
    let opened = docked(cx, docks());
    let at = centre(cx, &opened, IMAGES_TAB);
    down(cx, &opened, at);
    drag_to(cx, &opened, point(px(60.0), px(300.0)));
    assert_eq!(label(cx, &opened, "drop-preview").as_deref(), Some("Dock on the left"));
    let preview = bounds(cx, &opened, "drop-preview");
    assert_eq!((preview.origin.x, preview.size.width), (px(0.0), px(350.0)), "35 % of the area, as the page does");

    drag_to(cx, &opened, point(px(940.0), px(300.0)));
    assert_eq!(label(cx, &opened, "drop-preview").as_deref(), Some("Dock on the right"));
    drag_to(cx, &opened, point(px(400.0), px(600.0)));
    assert_eq!(label(cx, &opened, "drop-preview").as_deref(), Some("Dock at the bottom"));
    drag_to(cx, &opened, point(px(400.0), px(200.0)));
    assert_eq!(label(cx, &opened, "drop-preview").as_deref(), Some("Open on the stage"), "an extension's view has a page");
}

#[gpui_kit::test]
fn a_panel_with_no_page_has_no_stage_to_land_on(cx: &mut TestAppContext) {
    let opened = docked(cx, docks());
    let at = centre(cx, &opened, VIEWS_TAB);
    down(cx, &opened, at);
    drag_to(cx, &opened, point(px(400.0), px(200.0)));
    assert!(!present_id(cx, &opened, "drop-preview"));
    up(cx, &opened, point(px(400.0), px(200.0)));
    assert!(taken(&opened).is_empty(), "a drop on nothing sends nothing: {:?}", taken(&opened));
}

#[gpui_kit::test]
fn a_drop_sends_the_move_call(cx: &mut TestAppContext) {
    let opened = docked(cx, docks());
    let at = centre(cx, &opened, IMAGES_TAB);
    down(cx, &opened, at);
    drag_to(cx, &opened, point(px(60.0), px(300.0)));
    up(cx, &opened, point(px(60.0), px(300.0)));
    assert_eq!(taken(&opened), ["Docks { gesture: Move { panel: Extension(\"docker/images\"), dock: Left } }"]);
    assert_eq!(label(cx, &opened, "dock-right-panel").as_deref(), Some("Views"), "the panel stays until the event");
    assert!(!present_id(cx, &opened, "dock-left"));
}

#[gpui_kit::test]
fn a_drop_on_the_stage_sends_the_stage_call(cx: &mut TestAppContext) {
    let opened = docked(cx, docks());
    let at = centre(cx, &opened, IMAGES_TAB);
    down(cx, &opened, at);
    drag_to(cx, &opened, point(px(400.0), px(200.0)));
    up(cx, &opened, point(px(400.0), px(200.0)));
    assert_eq!(taken(&opened), ["Docks { gesture: Stage { panel: Extension(\"docker/images\") } }"]);
}

#[gpui_kit::test]
fn the_consoles_strip_is_dragged_but_never_clicked(cx: &mut TestAppContext) {
    let opened = docked(cx, docks());
    // The strip is the console's own, in the top of its panel: a press there that no tab or
    // button took is the head's.
    let panel = bounds(cx, &opened, "dock-bottom-panel");
    let at = point(panel.origin.x + px(100.0), panel.origin.y + px(20.0));
    down(cx, &opened, at);
    up(cx, &opened, at);
    assert!(taken(&opened).is_empty(), "a click on a head does nothing");

    down(cx, &opened, at);
    drag_to(cx, &opened, point(px(60.0), px(300.0)));
    up(cx, &opened, point(px(60.0), px(300.0)));
    assert_eq!(taken(&opened), ["Docks { gesture: Move { panel: Console, dock: Left } }"]);
}

#[gpui_kit::test]
fn escape_cancels_a_drag_with_no_call(cx: &mut TestAppContext) {
    let opened = docked(cx, docks());
    let at = centre(cx, &opened, IMAGES_TAB);
    down(cx, &opened, at);
    drag_to(cx, &opened, point(px(60.0), px(300.0)));
    assert!(present_id(cx, &opened, "drop-preview"));

    escape(cx, &opened);
    assert!(!present_id(cx, &opened, "drop-preview"), "the preview goes with the drag");
    assert!(!present_id(cx, &opened, "drag-ghost"));
    up(cx, &opened, point(px(60.0), px(300.0)));
    assert!(taken(&opened).is_empty(), "a cancelled drag dropped: {:?}", taken(&opened));
}

#[gpui_kit::test]
fn a_drag_takes_the_keys_and_gives_them_back_when_it_ends_either_way(cx: &mut TestAppContext) {
    let opened = docked(cx, docks());
    assert!(keys_are_the_stages(cx, &opened));
    let at = centre(cx, &opened, IMAGES_TAB);

    down(cx, &opened, at);
    drag_to(cx, &opened, point(px(60.0), px(300.0)));
    assert!(!keys_are_the_stages(cx, &opened), "Escape must reach the drag, not the pane");
    escape(cx, &opened);
    assert!(keys_are_the_stages(cx, &opened), "cancelled: the pane has the keys again");

    down(cx, &opened, at);
    drag_to(cx, &opened, point(px(60.0), px(300.0)));
    up(cx, &opened, point(px(60.0), px(300.0)));
    assert!(keys_are_the_stages(cx, &opened), "dropped: the pane has the keys again");
}

#[gpui_kit::test]
fn a_release_that_went_elsewhere_ends_the_drag_with_no_call(cx: &mut TestAppContext) {
    let opened = docked(cx, docks());
    let at = centre(cx, &opened, IMAGES_TAB);
    down(cx, &opened, at);
    drag_to(cx, &opened, point(px(60.0), px(300.0)));
    // The button is already up when the pointer next moves.
    cx.update_window(opened.handle, |_, window, cx| window.pointer_move(point(px(70.0), px(310.0)), None, cx)).unwrap();
    cx.run_until_parked();
    assert!(!present_id(cx, &opened, "drop-preview"));
    assert!(taken(&opened).is_empty());
}

#[gpui_kit::test]
fn nothing_is_sent_while_the_daemon_is_gone(cx: &mut TestAppContext) {
    let opened = docked(cx, docks());
    let loaded = Event::Rail(Box::new(argus_desktop_core::connection::page::Rail { loaded: true, ..Default::default() }));
    hear(cx, &opened, vec![loaded, status(Connection::Connecting)]);

    let at = centre(cx, &opened, IMAGES_TAB);
    down(cx, &opened, at);
    up(cx, &opened, at);
    let grab = centre(cx, &opened, "dock-right-sash");
    down(cx, &opened, grab);
    drag_to(cx, &opened, by(grab, -100.0, 0.0));
    up(cx, &opened, by(grab, -100.0, 0.0));
    assert!(taken(&opened).is_empty(), "a press while reconnecting reached no daemon: {:?}", taken(&opened));
    assert_eq!(drawn_width(cx, &opened, "dock-right"), px(400.0));
}

#[gpui_kit::test]
fn a_tab_that_closes_sends_the_close_call_and_one_that_does_not_has_no_close_button(cx: &mut TestAppContext) {
    let mut page = docks();
    page.present[2].closes = true;
    let opened = docked(cx, page);
    assert!(!present_id(cx, &opened, "dock-tab-close-views"));
    cx.update_window(opened.handle, |_, window, cx| window.click("dock-tab-close-extension:docker/images".to_owned(), cx))
        .unwrap();
    cx.run_until_parked();
    let calls = taken(&opened);
    assert!(
        calls.iter().all(|call| call == "Docks { gesture: Close { panel: Extension(\"docker/images\") } }"),
        "only the close was sent, never a show: {calls:?}"
    );
    assert!(!calls.is_empty());
}

#[gpui_kit::test]
fn a_press_on_a_tab_or_a_dock_leaves_the_keys_with_the_pane(cx: &mut TestAppContext) {
    let opened = docked(cx, docks());
    let tab = centre(cx, &opened, IMAGES_TAB);
    down(cx, &opened, tab);
    up(cx, &opened, tab);
    assert!(keys_are_the_stages(cx, &opened), "a tab press took the keys from the pane");
    let body = centre(cx, &opened, "dock-right-panel");
    down(cx, &opened, body);
    up(cx, &opened, body);
    assert!(keys_are_the_stages(cx, &opened), "a press on a panel took the keys from the pane");
}

#[gpui_kit::test]
fn a_held_sash_says_so_until_it_is_let_go_or_cancelled(cx: &mut TestAppContext) {
    let opened = docked(cx, docks());
    let resizing = |cx: &mut TestAppContext| cx.update(|cx| opened.heard.read(cx).resizing());
    let grab = centre(cx, &opened, "dock-right-sash");
    down(cx, &opened, grab);
    assert!(!resizing(cx), "a press that has not travelled is not a resize");
    drag_to(cx, &opened, by(grab, -100.0, 0.0));
    assert!(resizing(cx), "panes wait for the size while a sash is held");
    up(cx, &opened, by(grab, -100.0, 0.0));
    assert!(!resizing(cx), "let go, the panes may report");

    down(cx, &opened, grab);
    drag_to(cx, &opened, by(grab, -100.0, 0.0));
    assert!(resizing(cx));
    escape(cx, &opened);
    assert!(!resizing(cx), "a cancelled drag is not a resize either");
}

/// SHELL-EDGE-1, CROSS-20: the hold is one set. A sash let go lifts only its
/// own; a gutter still held keeps the panes from reporting.
#[gpui_kit::test]
fn a_sash_let_go_does_not_lift_another_drags_hold(cx: &mut TestAppContext) {
    use crate::layout::holders::Holder;
    let opened = docked(cx, docks());
    let resizing = |cx: &mut TestAppContext| cx.update(|cx| opened.heard.read(cx).resizing());
    cx.update(|cx| opened.heard.update(cx, |heard, _| heard.hold_resize(Holder::Grid)));

    let grab = centre(cx, &opened, "dock-right-sash");
    down(cx, &opened, grab);
    drag_to(cx, &opened, by(grab, -100.0, 0.0));
    up(cx, &opened, by(grab, -100.0, 0.0));
    assert!(resizing(cx), "the sash's release lifted the grid's hold");

    down(cx, &opened, grab);
    drag_to(cx, &opened, by(grab, -100.0, 0.0));
    escape(cx, &opened);
    assert!(resizing(cx), "a cancelled sash lifted the grid's hold");

    cx.update(|cx| opened.heard.update(cx, |heard, _| heard.release_resize(Holder::Grid)));
    assert!(!resizing(cx));
}

#[gpui_kit::test]
fn a_middle_click_on_a_tab_that_closes_sends_the_close_call(cx: &mut TestAppContext) {
    let mut page = docks();
    page.present[2].closes = true;
    let opened = docked(cx, page);
    let middle = |cx: &mut TestAppContext, id: &str| {
        cx.update_window(opened.handle, |_, window, cx| {
            window.click_with_options(id.to_owned(), ClickOptions::new().with_button(MouseButton::Middle), cx)
        })
        .unwrap();
        cx.run_until_parked();
    };
    middle(cx, VIEWS_TAB);
    assert!(taken(&opened).is_empty(), "a tab that does not close ignores the middle button");
    middle(cx, IMAGES_TAB);
    assert_eq!(taken(&opened), ["Docks { gesture: Close { panel: Extension(\"docker/images\") } }"]);
}

/// Opens the menu a right click on `id` makes; `link` is heard once it is open
/// if `after` (a menu built before the daemon went away), else before.
fn pick_first_in_context_menu(cx: &mut TestAppContext, opened: &Opened, id: &str, link: Option<Event>, after: bool) {
    if let Some(link) = link.clone().filter(|_| !after) {
        hear(cx, opened, vec![link]);
    }
    cx.update_window(opened.handle, |_, window, cx| window.right_click(id.to_owned(), cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    if let Some(link) = link.filter(|_| after) {
        hear(cx, opened, vec![link]);
    }
    // The first press lands on the first item, not on the menu's name (SHELL-MENU-4).
    for key in ["down", "enter"] {
        cx.update_window(opened.handle, |_, window, cx| window.press(key, cx)).unwrap();
        cx.run_until_parked();
    }
}

#[gpui_kit::test]
fn a_tabs_context_menu_sends_the_move_it_is_given(cx: &mut TestAppContext) {
    let opened = docked(cx, docks());
    pick_first_in_context_menu(cx, &opened, IMAGES_TAB, None, false);
    assert_eq!(taken(&opened), ["Docks { gesture: Move { panel: Extension(\"docker/images\"), dock: Left } }"]);
}

#[gpui_kit::test]
fn a_tabs_context_menu_sends_nothing_while_the_daemon_is_gone(cx: &mut TestAppContext) {
    let loaded = Event::Rail(Box::new(argus_desktop_core::connection::page::Rail { loaded: true, ..Default::default() }));
    // Opened while the daemon is gone, and opened before it went.
    for after in [false, true] {
        let opened = docked(cx, docks());
        hear(cx, &opened, vec![loaded.clone()]);
        pick_first_in_context_menu(cx, &opened, IMAGES_TAB, Some(status(Connection::Connecting)), after);
        assert!(taken(&opened).is_empty(), "a menu item reached no daemon (opened after: {after}): {:?}", taken(&opened));
    }
}

/// SHELL-DOCK-9: the page puts the console in its dock only while it is
/// docked; Rust keeps `bottom.shown` as Console when the console goes to the
/// stage, so the dock must be drawn closed then.
#[gpui_kit::test]
fn a_dock_showing_the_console_is_closed_while_the_console_is_on_the_stage(cx: &mut TestAppContext) {
    let opened = docked(cx, docks());
    assert!(present_id(cx, &opened, "dock-bottom"));
    for place in [ConsolePlace::Tab, ConsolePlace::Fullscreen, ConsolePlace::Hidden] {
        hear(cx, &opened, vec![console(place)]);
        assert!(!present_id(cx, &opened, "dock-bottom"), "{place:?}: no dock, as the page hides it");
        assert_eq!(bounds(cx, &opened, "work-stage").size.height, px(HEIGHT));
    }
    hear(cx, &opened, vec![console(ConsolePlace::Docked)]);
    assert!(present_id(cx, &opened, "dock-bottom"));
}

#[gpui_kit::test]
fn the_release_that_ends_a_drag_does_not_press_what_is_under_it(cx: &mut TestAppContext) {
    let opened = docked(cx, docks());
    let stage = centre(cx, &opened, "work-stage");
    let tab = centre(cx, &opened, IMAGES_TAB);

    // A drop on the stage is the drag's alone.
    down(cx, &opened, tab);
    drag_to(cx, &opened, stage);
    up(cx, &opened, stage);
    assert_eq!(taken(&opened).len(), 1, "the drop sent its call");
    assert_eq!(opened.ups.get(), 0, "the release pressed the stage");

    // A press that never travelled is an ordinary click, and reaches it.
    down(cx, &opened, stage);
    up(cx, &opened, stage);
    assert_eq!(opened.ups.get(), 1);
}

/// A middle press elsewhere released over a closing tab is not a click on it,
/// as the page's `auxclick` is not.
#[gpui_kit::test]
fn a_middle_press_that_began_on_the_stage_and_ended_on_a_tab_closes_nothing(cx: &mut TestAppContext) {
    let mut page = docks();
    page.present[2].closes = true;
    let opened = docked(cx, page);
    let (stage, tab) = (centre(cx, &opened, "work-stage"), centre(cx, &opened, IMAGES_TAB));
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(stage, MouseButton::Middle, cx);
        window.pointer_up(tab, MouseButton::Middle, cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(taken(&opened).is_empty(), "{:?}", taken(&opened));
}

/// SHELL-DOCK-6, `docks.ts` `moveItems`: the dock a panel is in is marked
/// `here` and disabled, so picking it sends nothing, while the others send
/// their move. The kit's snapshot does not report a menu item's disabled state,
/// so what is disabled is what a pick does.
#[gpui_kit::test]
fn the_dock_menu_item_for_where_a_panel_is_sends_nothing_and_the_others_send_their_move(cx: &mut TestAppContext) {
    let pick = |cx: &mut TestAppContext, opened: &Opened, words: &str| {
        cx.update_window(opened.handle, |_, window, cx| window.right_click(IMAGES_TAB.to_owned(), cx)).unwrap();
        cx.run_until_parked();
        cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
        let at = cx.update_window(opened.handle, |_, window, _| window.find_by_label(words).bounds().center()).unwrap();
        cx.update_window(opened.handle, |_, window, cx| {
            window.pointer_down(at, MouseButton::Left, cx);
            window.pointer_up(at, MouseButton::Left, cx);
        })
        .unwrap();
        cx.run_until_parked();
    };
    let opened = docked(cx, docks());
    pick(cx, &opened, "Dock on the right");
    assert!(taken(&opened).is_empty(), "the dock it is in: {:?}", taken(&opened));
    let opened = docked(cx, docks());
    pick(cx, &opened, "Dock at the bottom");
    assert_eq!(taken(&opened), ["Docks { gesture: Move { panel: Extension(\"docker/images\"), dock: Bottom } }"]);
}

/// While the daemon is gone every item of a menu is disabled: each one picked,
/// even from a menu that was opened while it was there, sends nothing.
#[gpui_kit::test]
fn every_dock_menu_item_sends_nothing_while_the_daemon_is_gone(cx: &mut TestAppContext) {
    let loaded = Event::Rail(Box::new(argus_desktop_core::connection::page::Rail { loaded: true, ..Default::default() }));
    let opened = docked(cx, docks());
    hear(cx, &opened, vec![loaded, status(Connection::Connecting)]);
    cx.update_window(opened.handle, |_, window, cx| window.right_click(IMAGES_TAB.to_owned(), cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    for words in ["Dock on the left", "Dock at the bottom", "Hide"] {
        let at = cx.update_window(opened.handle, |_, window, _| window.find_by_label(words).bounds().center()).unwrap();
        cx.update_window(opened.handle, |_, window, cx| {
            window.pointer_down(at, MouseButton::Left, cx);
            window.pointer_up(at, MouseButton::Left, cx);
        })
        .unwrap();
        cx.run_until_parked();
        assert!(taken(&opened).is_empty(), "{words}: {:?}", taken(&opened));
        cx.update_window(opened.handle, |_, window, cx| window.right_click(IMAGES_TAB.to_owned(), cx)).unwrap();
        cx.run_until_parked();
        cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    }
}

/// The page caps a side dock at `100% - --dock-floor` and the bottom dock at
/// `100% - 120px`: the stage keeps that much however big the size asked.
#[gpui_kit::test]
fn a_dock_sized_past_the_window_leaves_the_stage_its_floor(cx: &mut TestAppContext) {
    let mut page = docks();
    page.right.size = 5000;
    page.bottom.size = 5000;
    let opened = docked(cx, page);
    let stage = bounds(cx, &opened, "work-stage");
    assert_eq!(stage.size.height, px(120.0));
    assert!(stage.size.width >= px(240.0), "{:?}", stage.size);
}

fn docked_wide(cx: &mut TestAppContext, page: DocksPage, width: f32) -> Opened {
    open_wide(cx, vec![status(Connection::Connected), console(ConsolePlace::Docked), Event::Docks(Box::new(page))], width)
}

#[gpui_kit::test]
fn under_1200_px_the_right_dock_is_a_drawer_over_the_stage_and_wider_it_squeezes_the_stage(cx: &mut TestAppContext) {
    // SHELL-STAGE-15, SHELL-DOCK-10: the terminal keeps its columns in a small window.
    let narrow = docked_wide(cx, docks(), 1200.0);
    assert!(present_id(cx, &narrow, "drawer"));
    let (dock, stage) = (bounds(cx, &narrow, "dock-right"), bounds(cx, &narrow, "work-stage"));
    assert_eq!(dock.right(), px(1200.0), "it hangs from the work area's right edge");
    assert_eq!(dock.size.width - px(1.0), px(400.0), "as wide as it was dragged, its border outside");
    assert_eq!(stage.size.width, px(1200.0), "the stage keeps all its width");
    let wide = docked_wide(cx, docks(), 1201.0);
    assert!(!present_id(cx, &wide, "drawer"));
    assert_eq!(bounds(cx, &wide, "work-stage").size.width, px(1201.0 - 400.0 - 1.0), "a wide window gives the dock its own column, its border outside the size");
}

#[gpui_kit::test]
fn the_drawer_is_no_wider_than_the_drawer_maximum_or_what_leaves_the_stage_a_sliver(cx: &mut TestAppContext) {
    let mut page = docks();
    page.right.size = 900;
    let opened = docked_wide(cx, page.clone(), 1000.0);
    assert_eq!(bounds(cx, &opened, "drawer").size.width, px(560.0), "`--drawer-max`");
    let small = docked_wide(cx, page, 500.0);
    assert_eq!(bounds(cx, &small, "drawer").size.width, px(500.0 - 44.0), "a sliver of 44px is left to click back to");
}

#[gpui_kit::test]
fn above_1200_px_the_right_dock_is_no_wider_than_the_min_pane_unless_the_window_has_the_room(cx: &mut TestAppContext) {
    // style.css `.dock-right { max-width: min(100% - floor, max(440px, 100% - 800px)) }` above 1200px.
    let mut page = docks();
    page.right.size = 560;
    let tight = docked_wide(cx, page.clone(), 1240.0);
    assert_eq!(drawn_width(cx, &tight, "dock-right"), px(440.0), "1240 - 800 is under the min pane");
    let roomy = docked_wide(cx, page.clone(), 1440.0);
    assert_eq!(drawn_width(cx, &roomy, "dock-right"), px(560.0), "1440 - 800 = 640 leaves room for the size dragged");
    let mut wide = page;
    wide.right.size = 5000;
    let capped = docked_wide(cx, wide, 1440.0);
    assert_eq!(drawn_width(cx, &capped, "dock-right"), px(640.0), "never past the room rule");
}

#[test]
fn a_side_dock_caps_against_the_floor_and_only_the_right_one_against_the_room() {
    let tokens = crate::theme::themes::ledger::tokens();
    use crate::layout::dock::capped;
    assert_eq!(capped(Dock::Left, 900.0, 1000.0, 1000.0, &tokens), 760.0);
    assert_eq!(capped(Dock::Left, 900.0, 1400.0, 1440.0, &tokens), 900.0, "the room rule is the right dock's");
    assert_eq!(capped(Dock::Right, 900.0, 1400.0, 1440.0, &tokens), 600.0);
    assert_eq!(capped(Dock::Right, 900.0, 1100.0, 1100.0, &tokens), 900.0, "the drawer has its own maximum");
    assert_eq!(capped(Dock::Right, 900.0, 0.0, 1440.0, &tokens), 900.0, "an area not laid out caps nothing");
}

#[gpui_kit::test]
fn a_dock_holding_only_the_views_panel_draws_the_panels_strip_and_no_head_over_it(cx: &mut TestAppContext) {
    // The views' tabs carry the dock button and take the head's press, as the console's do.
    let mut page = docks();
    page.right.tabs = vec![tab(&Panel::Views, "Views")];
    let opened = docked(cx, page);
    assert!(!present_id(cx, &opened, "dock-menu-right"), "a second head over the panel's strip");
    let mut with_images = docks();
    with_images.right.tabs = vec![tab(&images(), "Images")];
    with_images.right.shown = Some(images());
    let other = docked(cx, with_images);
    assert!(present_id(cx, &other, "dock-menu-right"), "any other single panel has the dock's head");
}
