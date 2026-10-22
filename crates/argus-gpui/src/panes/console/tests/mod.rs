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

fn rename(cx: &mut TestAppContext, opened: &Opened) {
    cx.update_window(opened.handle, |_, window, cx| {
        rename::open(6, "cargo test".into(), opened.caller.clone(), opened.heard.clone(), window, cx);
    })
    .unwrap();
    settle(cx, opened);
}

mod keys_and_menus;
mod rename_dialog;
mod strip;
