use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use argus_desktop_core::connection::page::{Connection, Event, Frame, FrameCursor, Rail, Segment, Shown, Status};
use argus_desktop_core::panes::console::{ConsolePage, ConsolePlace, ConsoleTab};
use gpui_kit::test::{TestEventExt as _, TestQueryExt as _, TestWindowExt as _};
use gpui_kit::{
    size, AnyWindowHandle, Bounds, ElementId, Keystroke, MouseButton, Pixels, Point, Render, TestAppContext, WindowBounds, WindowOptions,
};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

const WIDTH: f32 = 1000.0;
const HEIGHT: f32 = 600.0;

type Calls = Rc<RefCell<Vec<Call>>>;

/// The Workspace's pane, standing as the stage with its own keys, beside the console.
struct Host {
    console: Entity<ConsoleView>,
    workspace: Entity<Terminal>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .child(div().h(px(100.0)).child(self.workspace.clone()))
            .child(div().flex_1().min_h_0().child(self.console.clone()))
    }
}

struct Opened {
    handle: AnyWindowHandle,
    heard: Entity<Heard>,
    console: Entity<ConsoleView>,
    stage: FocusHandle,
    workspace: Entity<Terminal>,
    events: UnboundedSender<Vec<Event>>,
    caller: Caller,
    calls: Calls,
}

fn open(cx: &mut TestAppContext) -> Opened {
    cx.update(crate::window::init);
    let calls: Calls = Rc::default();
    let sink = calls.clone();
    let caller = Caller::new(move |call| sink.borrow_mut().push(call));
    let caller_kept = caller.clone();
    let (events, listening) = unbounded_channel();
    let mut built = None;
    let (handle, _) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(Default::default(), size(px(WIDTH), px(HEIGHT))))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                let heard = cx.new(|_| Heard::new());
                heard.update(cx, |heard, cx| heard.listen(listening, cx));
                let workspace = cx.new(|cx| Terminal::new(heard.clone(), caller.clone(), Side::Workspace, window, cx));
                let stage = workspace.read(cx).focus_handle().clone();
                let console = cx.new(|cx| ConsoleView::new(heard.clone(), caller, stage.clone(), window, cx));
                built = Some((heard, console.clone(), stage.clone(), workspace.clone()));
                cx.new(|_| Host { console, workspace })
            },
        )
        .expect("open test window")
    });
    let (heard, console, stage, workspace) = built.unwrap();
    let opened = Opened { handle, heard, console, stage, workspace, events, caller: caller_kept, calls };
    settle(cx, &opened);
    opened
}

fn settle(cx: &mut TestAppContext, opened: &Opened) {
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

fn hear(cx: &mut TestAppContext, opened: &Opened, events: Vec<Event>) {
    opened.events.send(events).unwrap();
    settle(cx, opened);
}

/// Events heard with no frame forced after them, so a paint counted is one a change asked for.
fn hear_parked(cx: &mut TestAppContext, opened: &Opened, events: Vec<Event>) {
    opened.events.send(events).unwrap();
    cx.run_until_parked();
}

fn tab(pane: u64, title: &str, status: &'static str, on: bool) -> ConsoleTab {
    ConsoleTab { pane, title: title.into(), status, on }
}

fn connected() -> Event {
    Event::Status(Status { connection: Connection::Connected, detail: None, alarm: None, chime: true, host: None })
}

fn page(tabs: Vec<ConsoleTab>, showing: Option<u64>, beside: Option<u64>, typing: Option<u64>) -> Event {
    Event::Console(Box::new(ConsolePage { place: ConsolePlace::Tab, tabs, showing, beside, typing, can_spawn: true }))
}

/// Two consoles side by side, the second typed in.
fn split() -> Event {
    page(vec![tab(5, "pwsh", "idle", true), tab(6, "cargo test", "working", true), tab(7, "node", "idle", false)], Some(5), Some(6), Some(6))
}

/// One console, the only tab.
fn single() -> Event {
    page(vec![tab(5, "pwsh", "idle", true), tab(7, "node", "idle", false)], Some(5), None, Some(5))
}

fn frame(pane: u64, side: Side, row0: &str) -> Event {
    Event::Frame(Frame {
        pane,
        side,
        rows: 24,
        cols: 80,
        full: true,
        cursor: FrameCursor { row: 0, col: 0, visible: true, shape: "block" },
        alternate_screen: false,
        mouse: false,
        lines: vec![(0, vec![Segment(0, row0.chars().map(|c| c.to_string()).collect(), 0, 0, 0)])],
        parked: None,
    })
}

fn taken(opened: &Opened) -> Vec<String> {
    opened.calls.borrow().iter().filter(|call| !matches!(call, Call::Focus { .. } | Call::Resize { .. })).map(|call| format!("{call:?}")).collect()
}

fn centre(cx: &mut TestAppContext, opened: &Opened, id: &str) -> Point<Pixels> {
    cx.update_window(opened.handle, |_, window, _| window.find(id.to_owned()).bounds().center()).unwrap()
}

fn present(cx: &mut TestAppContext, opened: &Opened, id: &str) -> bool {
    cx.update_window(opened.handle, |_, window, _| window.try_find(id.to_owned()).is_some()).unwrap()
}

fn selected(cx: &mut TestAppContext, opened: &Opened, id: &str) -> Option<bool> {
    cx.update_window(opened.handle, |_, window, _| window.find(id.to_owned()).selected()).unwrap()
}

fn click(cx: &mut TestAppContext, opened: &Opened, id: &str) {
    cx.update_window(opened.handle, |_, window, cx| window.click(id.to_owned(), cx)).unwrap();
    settle(cx, opened);
}

fn click_with(cx: &mut TestAppContext, opened: &Opened, id: &str, button: MouseButton) {
    let at = centre(cx, opened, id);
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(at, button, cx);
        window.pointer_up(at, button, cx);
    })
    .unwrap();
    settle(cx, opened);
}

/// Clicks the `nth` item of the open menu: the kit gives its items no accessible
/// label, only their place in the list (headings and rules count).
fn click_menu_item(cx: &mut TestAppContext, opened: &Opened, nth: u64) {
    let at = cx
        .update_window(opened.handle, |_, window, _| {
            let item = window.elements().into_iter().find(|element| {
                let path = element.path();
                path.iter().any(|id| matches!(id, ElementId::Name(name) if name.as_ref() == "popup-menu"))
                    && matches!(path.last(), Some(ElementId::Integer(n)) if *n == nth)
            });
            item.expect("the menu has that item").bounds().center()
        })
        .unwrap();
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(at, MouseButton::Left, cx);
        window.pointer_up(at, MouseButton::Left, cx);
    })
    .unwrap();
    settle(cx, opened);
}

fn press(cx: &mut TestAppContext, opened: &Opened, key: &str) {
    let keystroke = Keystroke::parse(key).unwrap();
    cx.update_window(opened.handle, |_, window, cx| window.dispatch_keystroke(keystroke, cx)).unwrap();
    settle(cx, opened);
}

fn has_keys(cx: &mut TestAppContext, opened: &Opened, which: &str) -> bool {
    cx.update_window(opened.handle, |_, window, cx| {
        let console = opened.console.read(cx);
        let handle = match which {
            "first" => console.first.read(cx).focus_handle().clone(),
            "second" => console.second.read(cx).focus_handle().clone(),
            _ => opened.stage.clone(),
        };
        handle.is_focused(window)
    })
    .unwrap()
}

fn paints(cx: &mut TestAppContext, opened: &Opened, side: &str) -> u64 {
    cx.update(|cx| {
        let console = opened.console.read(cx);
        let terminal = if side == "first" { &console.first } else { &console.second };
        terminal.read(cx).paints()
    })
}

#[gpui_kit::test]
fn a_console_snapshot_draws_its_tabs_and_screens(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split()]);

    for id in ["console-tab-5", "console-tab-6", "console-tab-7"] {
        assert!(present(cx, &opened, id), "{id}");
    }
    assert_eq!(selected(cx, &opened, "console-tab-5"), Some(true), "chosen for the first side");
    assert_eq!(selected(cx, &opened, "console-tab-6"), Some(true), "chosen for the second");
    assert_eq!(selected(cx, &opened, "console-tab-7"), Some(false));
    // Two screens at once, each under its own header while split; the one with the keys marked.
    assert!(present(cx, &opened, "terminal-first") && present(cx, &opened, "terminal-second"));
    assert_eq!(selected(cx, &opened, "console-side-head-6"), Some(true));
    assert_eq!(selected(cx, &opened, "console-side-head-5"), Some(false));

    let (first, second) = (paints(cx, &opened, "first"), paints(cx, &opened, "second"));
    hear_parked(cx, &opened, vec![frame(5, Side::First, "first screen"), frame(6, Side::Second, "second screen")]);
    cx.update(|cx| {
        let heard = opened.heard.read(cx);
        assert_eq!(heard.first.row_text(0), "first screen");
        assert_eq!(heard.second.row_text(0), "second screen");
    });
    assert!(paints(cx, &opened, "first") > first, "a frame for the first side repaints it");
    assert!(paints(cx, &opened, "second") > second, "and the second");
}

#[gpui_kit::test]
fn a_console_that_is_not_split_draws_one_screen_and_no_side_headers(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), single()]);
    assert!(present(cx, &opened, "terminal-first"));
    assert!(!present(cx, &opened, "terminal-second"));
    assert!(!present(cx, &opened, "console-side-head-5"));
}

#[gpui_kit::test]
fn a_tab_that_asks_the_daemon_changes_nothing_until_the_console_event_says_so(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), single()]);
    click(cx, &opened, "console-tab-pick-7");
    assert_eq!(taken(&opened), ["Console { gesture: Select { pane: 7 } }"]);
    assert_eq!(selected(cx, &opened, "console-tab-7"), Some(false), "nothing predicted: the daemon has not answered");
    assert_eq!(selected(cx, &opened, "console-tab-5"), Some(true));
    hear(cx, &opened, vec![page(vec![tab(5, "pwsh", "idle", false), tab(7, "node", "idle", true)], Some(7), None, Some(7))]);
    assert_eq!(selected(cx, &opened, "console-tab-7"), Some(true), "the event moved it");
}

#[gpui_kit::test]
fn a_middle_click_and_the_tabs_x_both_close_the_console(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), single()]);
    click_with(cx, &opened, "console-tab-pick-7", MouseButton::Middle);
    assert_eq!(taken(&opened), ["Console { gesture: Close { pane: 7 } }"], "a middle press selects nothing");
    opened.calls.borrow_mut().clear();
    click(cx, &opened, "console-tab-close-7");
    assert_eq!(taken(&opened), ["Console { gesture: Close { pane: 7 } }"]);
    assert!(present(cx, &opened, "console-tab-7"), "the tab goes when the daemon says it ended");
}

#[gpui_kit::test]
fn the_strips_buttons_send_theirs(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), single()]);
    click(cx, &opened, "console-new");
    click(cx, &opened, "console-hide");
    assert_eq!(taken(&opened), ["Console { gesture: Spawn }", "Console { gesture: Hide }"]);
}

#[gpui_kit::test]
fn an_older_daemon_offers_no_new_console_button(cx: &mut TestAppContext) {
    let opened = open(cx);
    let mut older = ConsolePage { place: ConsolePlace::Tab, tabs: vec![tab(5, "pwsh", "idle", true)], showing: Some(5), ..ConsolePage::default() };
    older.can_spawn = false;
    hear(cx, &opened, vec![connected(), Event::Console(Box::new(older))]);
    assert!(!present(cx, &opened, "console-new"));
    assert!(present(cx, &opened, "console-hide"));
}

#[gpui_kit::test]
fn the_more_menu_sends_what_its_item_says(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), single()]);
    click(cx, &opened, "console-menu");
    click_menu_item(cx, &opened, 0);
    assert_eq!(taken(&opened), ["Console { gesture: Split }"]);
}

#[gpui_kit::test]
fn a_split_sides_header_selects_and_its_close_leaves_the_split(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split()]);
    click(cx, &opened, "console-side-pick-5");
    click(cx, &opened, "console-side-close-6");
    assert_eq!(taken(&opened), ["Console { gesture: Select { pane: 5 } }", "Console { gesture: CloseSide { pane: 6 } }"]);
}

#[gpui_kit::test]
fn with_no_console_on_screen_the_body_says_how_to_get_one(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), page(Vec::new(), None, None, None)]);
    assert!(present(cx, &opened, "console-empty"));
    click(cx, &opened, "console-start");
    assert_eq!(taken(&opened), ["Console { gesture: Spawn }"]);

    opened.calls.borrow_mut().clear();
    hear(cx, &opened, vec![page(vec![tab(5, "pwsh", "idle", false), tab(6, "node", "idle", true)], None, None, None)]);
    click(cx, &opened, "console-type");
    assert_eq!(taken(&opened), ["Console { gesture: Select { pane: 6 } }"], "the chosen tab");
}

#[gpui_kit::test]
fn a_stopped_daemon_leaves_the_console_answering_no_press(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), single()]);
    hear(cx, &opened, vec![Event::Status(Status { connection: Connection::Stopped, detail: None, alarm: None, chime: true, host: None })]);
    click(cx, &opened, "console-tab-pick-7");
    click(cx, &opened, "console-hide");
    assert!(taken(&opened).is_empty(), "{:?}", taken(&opened));
}

#[gpui_kit::test]
fn the_keys_move_to_the_screen_the_console_says_is_typed_in(cx: &mut TestAppContext) {
    let opened = open(cx);
    assert!(has_keys(cx, &opened, "stage"), "the Workspace's pane has them to begin with");
    hear(cx, &opened, vec![connected(), single()]);
    assert!(has_keys(cx, &opened, "first"), "a console shown takes them");
    hear(cx, &opened, vec![split()]);
    assert!(has_keys(cx, &opened, "second"));
    hear(cx, &opened, vec![page(vec![tab(5, "pwsh", "idle", true)], None, None, None)]);
    assert!(has_keys(cx, &opened, "stage"), "the console letting its panes go gives them back");
}

#[gpui_kit::test]
fn a_press_on_a_screen_without_the_keys_asks_for_them_and_waits_for_the_answer(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split(), page(vec![tab(5, "pwsh", "idle", true), tab(6, "cargo test", "working", true)], Some(5), Some(6), Some(5))]);
    assert!(has_keys(cx, &opened, "first"));
    let at = centre(cx, &opened, "terminal-second");
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(at, MouseButton::Left, cx);
        window.pointer_up(at, MouseButton::Left, cx);
    })
    .unwrap();
    settle(cx, &opened);
    assert_eq!(taken(&opened), ["Console { gesture: Select { pane: 6 } }"]);
    assert!(has_keys(cx, &opened, "first"), "nothing predicted: the keys are where the console says");
    hear(cx, &opened, vec![split()]);
    assert!(has_keys(cx, &opened, "second"), "the event moved them");

    opened.calls.borrow_mut().clear();
    let at = centre(cx, &opened, "terminal-second");
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(at, MouseButton::Left, cx);
        window.pointer_up(at, MouseButton::Left, cx);
    })
    .unwrap();
    settle(cx, &opened);
    assert!(taken(&opened).is_empty(), "a screen that has the keys asks for nothing");
}

#[gpui_kit::test]
fn a_key_typed_into_the_second_screen_is_sent_once_and_from_that_screen_only(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split()]);
    assert!(has_keys(cx, &opened, "second") && !has_keys(cx, &opened, "first"));
    press(cx, &opened, "enter");
    assert_eq!(taken(&opened).len(), 1, "one key, one call: {:?}", taken(&opened));
    assert!(taken(&opened)[0].starts_with("Key {"), "{:?}", taken(&opened));

    opened.calls.borrow_mut().clear();
    hear(cx, &opened, vec![single()]);
    assert!(has_keys(cx, &opened, "first"));
    press(cx, &opened, "enter");
    assert_eq!(taken(&opened).len(), 1, "typed into the first now, still once");
}

fn rename(cx: &mut TestAppContext, opened: &Opened) {
    cx.update_window(opened.handle, |_, window, cx| {
        rename::open(6, "cargo test".into(), opened.caller.clone(), opened.heard.clone(), window, cx);
    })
    .unwrap();
    settle(cx, opened);
}

#[gpui_kit::test]
fn the_rename_dialog_asks_for_the_new_name_on_enter_and_gives_the_keys_back(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split()]);
    assert!(has_keys(cx, &opened, "second"));
    rename(cx, &opened);
    assert!(!has_keys(cx, &opened, "second"), "the dialog has the keys while it is open");
    press(cx, &opened, "b");
    press(cx, &opened, "u");
    press(cx, &opened, "enter");
    assert_eq!(taken(&opened), ["RenamePane { pane: 6, title: \"bu\" }"], "the old name was selected, so typing replaced it");
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    settle(cx, &opened);
    assert!(has_keys(cx, &opened, "second"), "closing it gives the keys back to what had them");
    assert!(!cx.update_window(opened.handle, |_, window, cx| gpui_kit::component::WindowExt::has_active_dialog(window, cx)).unwrap());
}

#[gpui_kit::test]
fn escape_closes_the_rename_dialog_asking_nothing_and_gives_the_keys_back(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split()]);
    rename(cx, &opened);
    press(cx, &opened, "x");
    press(cx, &opened, "escape");
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    settle(cx, &opened);
    assert!(taken(&opened).is_empty(), "{:?}", taken(&opened));
    assert!(has_keys(cx, &opened, "second"));
    assert!(!cx.update_window(opened.handle, |_, window, cx| gpui_kit::component::WindowExt::has_active_dialog(window, cx)).unwrap());
}

#[gpui_kit::test]
fn the_rename_dialog_is_modal_and_tab_stays_inside_it(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split()]);
    rename(cx, &opened);
    for _ in 0..6 {
        press(cx, &opened, "tab");
        assert!(!has_keys(cx, &opened, "first") && !has_keys(cx, &opened, "second") && !has_keys(cx, &opened, "stage"), "Tab left the dialog");
    }
    assert!(taken(&opened).is_empty(), "keys in the dialog reach no pane: {:?}", taken(&opened));
}

#[gpui_kit::test]
fn the_rename_button_asks_and_cancel_does_not(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split()]);
    rename(cx, &opened);
    press(cx, &opened, "z");
    click(cx, &opened, "rename-cancel");
    assert!(taken(&opened).is_empty());

    rename(cx, &opened);
    press(cx, &opened, "z");
    click(cx, &opened, "rename-ok");
    assert_eq!(taken(&opened), ["RenamePane { pane: 6, title: \"z\" }"]);
}

#[gpui_kit::test]
fn the_tab_menu_offers_rename_and_close_for_that_tab(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), single()]);
    let at = centre(cx, &opened, "console-tab-pick-7");
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(at, MouseButton::Right, cx);
        window.pointer_up(at, MouseButton::Right, cx);
    })
    .unwrap();
    settle(cx, &opened);
    click_menu_item(cx, &opened, 3);
    assert_eq!(taken(&opened), ["Console { gesture: Close { pane: 7 } }"]);
}

#[gpui_kit::test]
fn each_screen_reports_its_room_for_its_own_side(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split()]);
    cx.executor().advance_clock(std::time::Duration::from_millis(200));
    cx.run_until_parked();
    let mut sides: Vec<String> = opened
        .calls
        .borrow()
        .iter()
        .filter_map(|call| match call {
            // The host's own Workspace screen reports its room too; it is not the console's.
            Call::Resize { side, .. } if *side != Side::Workspace => Some(format!("{side:?}")),
            _ => None,
        })
        .collect();
    sides.sort();
    assert_eq!(sides, ["First", "Second"], "one report each, and none from the console for the Workspace's side");
}

#[gpui_kit::test]
fn a_press_on_the_workspace_while_a_console_has_the_keys_asks_the_session_and_waits(cx: &mut TestAppContext) {
    let opened = open(cx);
    let shown = Shown {
        pane: 9,
        repository: "argus".into(),
        checkout: "main".into(),
        branch: "main".into(),
        agent: false,
        browser: false,
        title: "shell".into(),
        status: "idle",
        resumed: None,
    };
    hear(cx, &opened, vec![connected(), Event::Rail(Box::new(Rail { shown: Some(shown), ..Rail::default() })), single()]);
    assert!(has_keys(cx, &opened, "first"));
    let at = centre(cx, &opened, "terminal");
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(at, MouseButton::Left, cx);
        window.pointer_up(at, MouseButton::Left, cx);
    })
    .unwrap();
    settle(cx, &opened);
    assert_eq!(taken(&opened), ["Select { pane: 9 }"]);
    assert!(has_keys(cx, &opened, "first"), "nothing predicted: the keys stay where the session routes them");
    let workspace_focused = cx.update_window(opened.handle, |_, window, cx| opened.workspace.read(cx).focus_handle().is_focused(window)).unwrap();
    assert!(!workspace_focused);
}

#[gpui_kit::test]
fn the_menus_rename_item_opens_the_dialog_that_asks_on_enter(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split()]);
    click_with(cx, &opened, "console-tab-pick-7", MouseButton::Right);
    click_menu_item(cx, &opened, 1);
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    settle(cx, &opened);
    assert!(cx.update_window(opened.handle, |_, window, cx| gpui_kit::component::WindowExt::has_active_dialog(window, cx)).unwrap());
    press(cx, &opened, "x");
    press(cx, &opened, "enter");
    assert_eq!(taken(&opened), ["RenamePane { pane: 7, title: \"x\" }"]);
}

#[gpui_kit::test]
fn a_right_or_middle_press_on_a_screen_without_the_keys_asks_for_them_too(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), page(vec![tab(5, "pwsh", "idle", true), tab(6, "cargo test", "working", true)], Some(5), Some(6), Some(5))]);
    // SHELL-INPUT-5, -9: any button asks, none moves the keys before the event says so.
    for button in [MouseButton::Right, MouseButton::Middle] {
        opened.calls.borrow_mut().clear();
        click_with(cx, &opened, "terminal-second", button);
        assert_eq!(taken(&opened), ["Console { gesture: Select { pane: 6 } }"], "{button:?}");
        assert!(has_keys(cx, &opened, "first"), "{button:?}");
    }
}

#[gpui_kit::test]
fn a_field_that_has_the_keys_keeps_them_when_the_console_moves_them(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split()]);
    rename(cx, &opened);
    assert!(!has_keys(cx, &opened, "second"));
    // The session moves the keys to the first side while the dialog's field has GPUI's focus.
    hear(cx, &opened, vec![page(vec![tab(5, "pwsh", "idle", true), tab(6, "cargo test", "working", true)], Some(5), Some(6), Some(5))]);
    assert!(
        !has_keys(cx, &opened, "first") && !has_keys(cx, &opened, "second") && !has_keys(cx, &opened, "stage"),
        "a dialog's field keeps the keys"
    );
    press(cx, &opened, "q");
    press(cx, &opened, "enter");
    assert_eq!(taken(&opened), ["RenamePane { pane: 6, title: \"q\" }"], "the field still took the typing");
}

#[gpui_kit::test]
fn the_menus_rename_dialog_gives_the_keys_back_to_what_had_them_before_the_menu(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split()]);
    assert!(has_keys(cx, &opened, "second"));
    click_with(cx, &opened, "console-tab-pick-7", MouseButton::Right);
    click_menu_item(cx, &opened, 1);
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    settle(cx, &opened);
    press(cx, &opened, "escape");
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    settle(cx, &opened);
    assert!(has_keys(cx, &opened, "second"), "not the menu's handle, which is gone");
}

#[gpui_kit::test]
fn the_more_menus_other_items_send_theirs(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), single()]);
    // Place Tab: 0 Split, 1 dock, 2 Fullscreen, 3 rule, then the dock moves.
    for (nth, call) in [(1, "Console { gesture: Dock }"), (2, "Console { gesture: Fullscreen }")] {
        opened.calls.borrow_mut().clear();
        click(cx, &opened, "console-menu");
        click_menu_item(cx, &opened, nth);
        assert_eq!(taken(&opened), [call], "item {nth}");
    }
    opened.calls.borrow_mut().clear();
    click(cx, &opened, "console-menu");
    click_menu_item(cx, &opened, 4);
    assert!(taken(&opened)[0].starts_with("Docks { gesture: Move { panel: Console"), "{:?}", taken(&opened));
}

#[gpui_kit::test]
fn the_tab_menus_close_the_others_closes_every_other_tab(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), single()]);
    click_with(cx, &opened, "console-tab-pick-7", MouseButton::Right);
    click_menu_item(cx, &opened, 4);
    assert_eq!(taken(&opened), ["Console { gesture: Close { pane: 5 } }"]);
}

#[gpui_kit::test]
fn a_split_sides_menu_takes_it_out_of_the_split(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split()]);
    click_with(cx, &opened, "console-side-pick-6", MouseButton::Right);
    click_menu_item(cx, &opened, 2);
    assert_eq!(taken(&opened), ["Console { gesture: CloseSide { pane: 6 } }"]);
}

/// SHELL-CONSOLE-11/12: the keys follow the screen that is typed in, not only
/// the pane id, so an unsplit or a trade of sides moves them too.
#[gpui_kit::test]
fn an_unsplit_that_keeps_the_second_side_moves_the_keys_to_the_first_screen(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split()]);
    assert!(has_keys(cx, &opened, "second"));
    hear(cx, &opened, vec![page(vec![tab(6, "cargo test", "working", true), tab(7, "node", "idle", false)], Some(6), None, Some(6))]);
    assert!(has_keys(cx, &opened, "first"), "the typed console is the first screen now");
    opened.calls.borrow_mut().clear();
    press(cx, &opened, "enter");
    assert_eq!(taken(&opened).len(), 1, "{:?}", taken(&opened));
    assert!(taken(&opened)[0].starts_with("Key {"), "{:?}", taken(&opened));
}

#[gpui_kit::test]
fn a_trade_of_sides_moves_the_keys_with_the_typed_console(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split()]);
    assert!(has_keys(cx, &opened, "second"));
    let tabs = || vec![tab(5, "pwsh", "idle", true), tab(6, "cargo test", "working", true)];
    hear(cx, &opened, vec![page(tabs(), Some(6), Some(5), Some(6))]);
    assert!(has_keys(cx, &opened, "first"), "pane 6 is the first side now");
    hear(cx, &opened, vec![page(tabs(), Some(5), Some(6), Some(6))]);
    assert!(has_keys(cx, &opened, "second"), "and back");
}

#[gpui_kit::test]
fn the_tab_menus_close_the_others_sends_a_close_for_each_of_the_others(cx: &mut TestAppContext) {
    let opened = open(cx);
    let tabs = vec![tab(5, "pwsh", "idle", true), tab(7, "node", "idle", false), tab(8, "vim", "idle", false)];
    hear(cx, &opened, vec![connected(), page(tabs, Some(5), None, Some(5))]);
    click_with(cx, &opened, "console-tab-pick-7", MouseButton::Right);
    click_menu_item(cx, &opened, 4);
    assert_eq!(taken(&opened), ["Console { gesture: Close { pane: 5 } }", "Console { gesture: Close { pane: 8 } }"]);
}

#[gpui_kit::test]
fn only_a_console_that_is_doing_something_wears_a_status_dot(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split()]);
    assert!(present(cx, &opened, "console-tab-dot-6"), "a working console wears one");
    assert!(!present(cx, &opened, "console-tab-dot-5"), "an idle one wears none");
    assert!(present(cx, &opened, "console-side-dot-6"));
    assert!(!present(cx, &opened, "console-side-dot-5"));
}

/// A press elsewhere released over a console tab is not a click on it, as the
/// page's `auxclick` is not.
#[gpui_kit::test]
fn a_middle_press_that_began_elsewhere_and_ended_on_a_tab_closes_nothing(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), single()]);
    let (away, over) = (centre(cx, &opened, "terminal-first"), centre(cx, &opened, "console-tab-pick-7"));
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(away, MouseButton::Middle, cx);
        window.pointer_up(over, MouseButton::Middle, cx);
    })
    .unwrap();
    settle(cx, &opened);
    assert!(taken(&opened).is_empty(), "{:?}", taken(&opened));
}

/// The hide button's tooltip and name are one text: the keymap's key for the
/// action once it is heard, none before.
#[gpui_kit::test]
fn the_hide_button_names_the_key_the_session_binds(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), single()]);
    let named = |cx: &mut TestAppContext| {
        cx.update_window(opened.handle, |_, window, _| window.find("console-hide-label".to_owned()).label().map(str::to_owned)).unwrap()
    };
    assert_eq!(named(cx).as_deref(), Some("Hide the console"));
    hear(cx, &opened, vec![Event::Keymap { groups: argus_desktop_core::keys::keymap::groups(), commands: Vec::new(), layers: &[] }]);
    assert_eq!(named(cx).as_deref(), Some("Hide the console (`)"));
}
