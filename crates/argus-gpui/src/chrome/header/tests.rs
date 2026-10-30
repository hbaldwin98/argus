use argus_desktop_core::chrome::strip::{Part, Says};
use argus_desktop_core::connection::page::{Connection, Event, Rail, Resumed, Shown, Stage, Status};
use gpui_kit::test::{TestEventExt as _, TestQueryExt as _, TestWindowExt as _};
use gpui_kit::{div, px, size, Context, ElementId, MouseButton, TestAppContext, Window};

use super::slots::{Offer, Slots};
use super::*;
use crate::keys::root::tests::{asked, open_sized, Opened};

/// The strip is tested in a window narrow enough to draw its status chip.
fn open(cx: &mut TestAppContext) -> Opened {
    open_sized(cx, 1000.0)
}

fn shown(status: &'static str, agent: bool) -> Shown {
    Shown {
        pane: 7,
        repository: "argus".into(),
        checkout: "main".into(),
        branch: "dev".into(),
        agent,
        browser: false,
        title: "Trim context".into(),
        status,
        resumed: None,
    }
}

fn connected() -> Event {
    Event::Status(Status { connection: Connection::Connected, detail: None, alarm: None, chime: true, host: None })
}

fn rail(shown: Option<Shown>, stage: Stage) -> Event {
    Event::Rail(Box::new(Rail { loaded: true, shown, stage, ..Rail::default() }))
}

fn hear(cx: &mut TestAppContext, opened: &Opened, events: Vec<Event>) {
    opened.events.send(events).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

fn has(cx: &mut TestAppContext, opened: &Opened, id: &'static str) -> bool {
    cx.update_window(opened.handle, |_, window, _| window.try_find(id.to_owned()).is_some()).unwrap()
}

fn said(cx: &mut TestAppContext, opened: &Opened, id: &'static str) -> Option<String> {
    cx.update_window(opened.handle, |_, window, _| window.try_find(id.to_owned()).and_then(|found| found.label().map(str::to_owned))).unwrap()
}

fn resize(cx: &mut TestAppContext, opened: &Opened, width: f32) {
    cx.simulate_window_resize(opened.handle, size(px(width), px(700.0)));
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

fn click(cx: &mut TestAppContext, opened: &Opened, id: impl Into<ElementId>) {
    let id = id.into();
    cx.update_window(opened.handle, |_, window, cx| window.click(id, cx)).unwrap();
    cx.run_until_parked();
}

/// Clicks the `nth` item of the open menu: the kit gives its items no accessible label, only their
/// place in the list (headings and rules count).
fn click_menu_item(cx: &mut TestAppContext, opened: &Opened, nth: u64) {
    let at = cx
        .update_window(opened.handle, |_, window, _| {
            let item = window.elements().into_iter().find(|element| {
                let path = element.path();
                path.iter().any(|id| matches!(id, ElementId::Name(name) if name.as_ref() == "popup-menu"))
                    && matches!(path.last(), Some(ElementId::Integer(n)) if n == &nth)
            });
            item.expect("the menu has that item").bounds().center()
        })
        .unwrap();
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(at, MouseButton::Left, cx);
        window.pointer_up(at, MouseButton::Left, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

#[gpui_kit::test]
fn the_pane_status_is_its_dot_at_every_width_and_never_a_word_that_takes_the_names_room(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), rail(Some(shown("working", true)), Stage::Workspace)]);
    for width in [860.0, 1200.0, 1201.0] {
        resize(cx, &opened, width);
        assert!(!has(cx, &opened, "head-status"), "{width}");
    }
}

#[gpui_kit::test]
fn a_resumed_pane_says_so_beside_its_title(cx: &mut TestAppContext) {
    let opened = open(cx);
    let resumed = Some(Resumed { at: None, turns: Some(3) });
    hear(cx, &opened, vec![connected(), rail(Some(Shown { resumed, ..shown("idle", true) }), Stage::Workspace)]);
    assert_eq!(said(cx, &opened, "head-resumed").as_deref(), Some("resumed \u{b7} 3 turns before"));
    hear(cx, &opened, vec![rail(Some(shown("idle", true)), Stage::Workspace)]);
    assert!(!has(cx, &opened, "head-resumed"));
}

#[gpui_kit::test]
fn a_pane_on_screen_has_a_more_menu_that_renames_and_closes_it(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), rail(Some(shown("idle", true)), Stage::Workspace)]);
    assert_eq!(said(cx, &opened, "head-more").as_deref(), Some("More about Trim context"));
    click(cx, &opened, "head-more-button");
    click_menu_item(cx, &opened, 3);
    assert_eq!(asked(&opened), ["Close { pane: 7 }"]);
    click(cx, &opened, "head-more-button");
    click_menu_item(cx, &opened, 1);
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    assert!(cx.update_window(opened.handle, |_, window, cx| gpui_kit::component::WindowExt::has_active_dialog(window, cx)).unwrap(), "the rename dialog is up");
}

#[gpui_kit::test]
fn the_more_menu_asks_nothing_once_the_daemon_is_gone(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), rail(Some(shown("idle", true)), Stage::Workspace)]);
    click(cx, &opened, "head-more-button");
    hear(cx, &opened, vec![Event::Status(Status { connection: Connection::Stopped, detail: None, alarm: None, chime: true, host: None })]);
    click_menu_item(cx, &opened, 3);
    assert!(asked(&opened).is_empty(), "{:?}", asked(&opened));
}

#[gpui_kit::test]
fn no_pane_on_screen_has_no_more_menu(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), rail(None, Stage::Workspace)]);
    assert!(!has(cx, &opened, "head-more"));
}

#[gpui_kit::test]
fn a_stage_puts_its_own_tools_in_the_strip_and_no_other_stage_shows_them(cx: &mut TestAppContext) {
    let opened = open(cx);
    cx.update(|cx| {
        Slots::of(cx).offer(Stage::Code, Offer::default().tools(|_, _| div().id("code-tool").test_support().into_any_element()));
    });
    hear(cx, &opened, vec![connected(), rail(None, Stage::Code)]);
    assert!(has(cx, &opened, "code-tool"));
    assert!(!has(cx, &opened, "head-more"), "a stage with no pane has no pane menu");
    hear(cx, &opened, vec![rail(None, Stage::Checkouts)]);
    assert!(!has(cx, &opened, "code-tool"));
}

#[gpui_kit::test]
fn a_stage_that_offers_nothing_has_the_name_it_is_given_and_no_tools(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let slots = Slots::of(cx);
        slots.offer(Stage::Feature, Offer::default().says(|_| Says { title: Some("Add retries".into()), trail: vec![Part::Mono("#12".into())], ..Says::default() }));
        assert_eq!(slots.says(Stage::Feature, cx).title.as_deref(), Some("Add retries"));
        assert_eq!(slots.says(Stage::Checkouts, cx), Says::default());
        // A second offer replaces the first.
        slots.offer(Stage::Feature, Offer::default());
        assert_eq!(slots.says(Stage::Feature, cx), Says::default());
    });
}

struct Launching {
    caller: crate::connection::caller::Caller,
    held: bool,
}

impl Render for Launching {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *crate::theme::Tokens::global(cx);
        div().children(launcher::launcher(3, true, Some(argus_desktop_core::connection::page::Call::HideViews), self.held, &self.caller, &tokens))
    }
}

#[gpui_kit::test]
fn the_views_launcher_sends_what_rust_says_unless_the_daemon_is_gone(cx: &mut TestAppContext) {
    use std::cell::RefCell;
    use std::rc::Rc;
    cx.update(crate::window::init);
    for held in [false, true] {
        let calls: Rc<RefCell<Vec<String>>> = Rc::default();
        let sink = calls.clone();
        let caller = crate::connection::caller::Caller::new(move |call| sink.borrow_mut().push(format!("{call:?}")));
        let (handle, _) = cx.update(|cx| {
            gpui_kit::open_window(Default::default(), cx, |_, cx| cx.new(|_| Launching { caller, held })).expect("open test window")
        });
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| window.render_frame(cx)).unwrap();
        cx.update_window(handle, |_, window, cx| window.click("views-tab", cx)).unwrap();
        cx.run_until_parked();
        assert_eq!(*calls.borrow(), if held { Vec::<String>::new() } else { vec!["HideViews".to_owned()] }, "held: {held}");
    }
}

#[gpui_kit::test]
fn a_stage_puts_its_tabs_after_the_crumb_and_a_press_only_calls(cx: &mut TestAppContext) {
    use std::cell::Cell;
    use std::rc::Rc;
    let opened = open(cx);
    let pressed: Rc<Cell<u32>> = Rc::default();
    let seen = pressed.clone();
    cx.update(|cx| {
        Slots::of(cx).offer(
            Stage::Code,
            Offer::default().tabs(move |_, _| {
                let seen = seen.clone();
                Some(tabs::Tabs::new(
                    "code-tabs",
                    "Which files",
                    vec![tabs::Tab::new("tab-a", "All files", true, |_, _| {}), tabs::Tab::new("tab-b", "Changed", false, move |_, _| seen.set(seen.get() + 1)).count(Some(4))],
                ))
            }),
        );
    });
    hear(cx, &opened, vec![connected(), rail(None, Stage::Code)]);
    let selected = |cx: &mut TestAppContext, id: &'static str| cx.update_window(opened.handle, |_, window, _| window.find(id.to_owned()).selected()).unwrap();
    assert_eq!((selected(cx, "tab-a"), selected(cx, "tab-b")), (Some(true), Some(false)));
    click(cx, &opened, "tab-b");
    assert_eq!(pressed.get(), 1);
    assert_eq!(selected(cx, "tab-b"), Some(false), "the bar never predicts: the stage says which tab is on");
    hear(cx, &opened, vec![rail(None, Stage::Checkouts)]);
    assert!(!has(cx, &opened, "tab-a"), "another stage shows none of Code's tabs");
}
