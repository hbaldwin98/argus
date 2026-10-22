use std::cell::RefCell;

use super::*;
use argus_desktop_core::connection::page::{Event, Frame, FrameCursor, Rail, Segment, Shown};
use gpui_kit::test::{TestInput, TestWindowExt as _};
use gpui_kit::{
    size, AnyWindowHandle, Bounds, ElementInputHandler, Keystroke, TestAppContext, WindowBounds, WindowOptions,
};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

const WIDTH: f32 = 1000.0;
const HEIGHT: f32 = 700.0;

type Calls = Rc<RefCell<Vec<Call>>>;

struct Opened {
    handle: AnyWindowHandle,
    terminal: Entity<Terminal>,
    heard: Entity<Heard>,
    events: UnboundedSender<Vec<Event>>,
    calls: Calls,
}

fn shown(pane: u64) -> Shown {
    Shown {
        pane,
        repository: "argus".into(),
        checkout: "main".into(),
        branch: "main".into(),
        agent: false,
        browser: false,
        title: "zsh".into(),
        status: "idle",
        resumed: None,
    }
}

fn rail(pane: u64) -> Event {
    Event::Rail(Box::new(Rail { loaded: true, shown: Some(shown(pane)), ..Rail::default() }))
}

fn segment(col: u16, text: &str) -> Segment {
    Segment(col, text.chars().map(|c| c.to_string()).collect(), 0, 0, 0)
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
        lines: vec![(0, vec![segment(0, row0)])],
        parked: None,
    })
}

fn open(cx: &mut TestAppContext) -> Opened {
    cx.update(crate::window::init);
    let calls: Calls = Rc::default();
    let sink = calls.clone();
    let caller = Caller::new(move |call| sink.borrow_mut().push(call));
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
                let terminal = cx.new(|cx| Terminal::new(heard.clone(), caller, Side::Workspace, window, cx));
                built = Some((heard, terminal.clone()));
                terminal
            },
        )
        .expect("open test window")
    });
    let (heard, terminal) = built.unwrap();
    let opened = Opened { handle, terminal, heard, events, calls };
    settle(cx, &opened);
    opened
}

fn settle(cx: &mut TestAppContext, opened: &Opened) {
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.run_until_parked();
}

fn hear(cx: &mut TestAppContext, opened: &Opened, events: Vec<Event>) {
    opened.events.send(events).unwrap();
    cx.run_until_parked();
}

fn paints(cx: &mut TestAppContext, opened: &Opened) -> u64 {
    cx.update(|cx| opened.terminal.read(cx).paints())
}

/// The calls the pane made about its keys and its room, not the window's
/// focus.
fn taken(opened: &Opened) -> Vec<Call> {
    opened.calls.borrow().iter().filter(|call| !matches!(call, Call::Focus { .. })).cloned().collect()
}

fn type_keys(cx: &mut TestAppContext, opened: &Opened, keys: &str) {
    for key in keys.split(' ') {
        let keystroke = Keystroke::parse(key).unwrap();
        cx.update_window(opened.handle, |_, window, cx| window.dispatch_keystroke(keystroke, cx)).unwrap();
    }
    cx.run_until_parked();
}

fn resizes(opened: &Opened) -> Vec<(Side, u16, u16)> {
    taken(opened)
        .into_iter()
        .filter_map(|call| match call {
            Call::Resize { side, rows, cols } => Some((side, rows, cols)),
            _ => None,
        })
        .collect()
}

#[gpui_kit::test]
fn a_frame_for_the_shown_pane_is_held_and_repaints_the_stage(cx: &mut TestAppContext) {
    let opened = open(cx);
    let before = paints(cx, &opened);
    assert!(before > 0, "the stage painted once on open");
    hear(cx, &opened, vec![rail(7)]);
    let railed = paints(cx, &opened);
    assert!(railed > before, "the rail moving the shown pane repaints");
    hear(cx, &opened, vec![frame(7, Side::Workspace, "hello from pane 7")]);
    assert!(paints(cx, &opened) > railed, "a frame repaints without anything asking for a frame");
    cx.update(|cx| {
        let heard = opened.heard.read(cx);
        assert_eq!(heard.screen.pane(), Some(7));
        assert_eq!(heard.screen.row_text(0), "hello from pane 7");
    });
}

#[gpui_kit::test]
fn nothing_repaints_the_stage_when_nothing_arrives(cx: &mut TestAppContext) {
    let opened = open(cx);
    let before = paints(cx, &opened);
    cx.executor().advance_clock(SETTLE * 10);
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(5));
    cx.run_until_parked();
    assert_eq!(paints(cx, &opened), before, "no timer is driving the stage");
}

#[gpui_kit::test]
fn a_frame_for_another_side_is_not_drawn_here(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![rail(7)]);
    let before = paints(cx, &opened);
    hear(cx, &opened, vec![frame(7, Side::First, "the console's")]);
    assert_eq!(paints(cx, &opened), before);
    cx.update(|cx| assert_eq!(opened.heard.read(cx).screen.pane(), None));
}

#[gpui_kit::test]
fn typing_in_the_pane_sends_each_key_the_way_the_page_does_and_never_again_as_text(cx: &mut TestAppContext) {
    let opened = open(cx);
    type_keys(cx, &opened, "a shift-a enter ctrl-c up f5 alt-f");
    let keys: Vec<(String, bool, bool)> = taken(&opened)
        .into_iter()
        .map(|call| match call {
            Call::Key { key } => (key.key, key.ctrl, key.alt),
            other => panic!("typing sent {other:?}"),
        })
        .collect();
    let expected: Vec<(String, bool, bool)> = [
        ("a", false, false),
        ("A", false, false),
        ("Enter", false, false),
        ("c", true, false),
        ("ArrowUp", false, false),
        ("F5", false, false),
        ("f", false, true),
    ]
    .into_iter()
    .map(|(key, ctrl, alt)| (key.to_owned(), ctrl, alt))
    .collect();
    assert_eq!(keys, expected);
}

#[gpui_kit::test]
fn text_an_input_method_commits_is_sent_as_text(cx: &mut TestAppContext) {
    let opened = open(cx);
    let terminal = opened.terminal.clone();
    cx.update_window(opened.handle, |_, window, cx| {
        let bounds = Bounds::new(Default::default(), size(px(100.0), px(100.0)));
        let mut input = TestInput::new(ElementInputHandler::new(bounds, terminal), window);
        input.compose_text(None, "にほ", None, cx);
        input.commit_text("日本", cx);
    })
    .unwrap();
    let calls = taken(&opened);
    assert_eq!(calls.len(), 1, "the preedit is shown nowhere and sent never: {calls:?}");
    assert!(matches!(&calls[0], Call::Text { text } if text == "日本"), "{calls:?}");
}

#[gpui_kit::test]
fn the_room_the_stage_has_is_reported_once_it_holds_still(cx: &mut TestAppContext) {
    let opened = open(cx);
    assert!(resizes(&opened).is_empty(), "nothing is reported before the size has settled");
    cx.executor().advance_clock(SETTLE * 2);
    cx.run_until_parked();
    let first = resizes(&opened);
    assert_eq!(first.len(), 1, "{first:?}");
    // The element is the window less the stage's own padding, in whole 20 px rows.
    let (side, rows, cols) = first[0];
    assert_eq!(side, Side::Workspace);
    assert_eq!(rows, ((HEIGHT - 16.0) / 20.0).floor() as u16);
    assert!(cols > 20 && cols < 200, "{cols} columns in {WIDTH} px");
    let (cell_width, _) = cx.update(|cx| opened.terminal.read(cx).cell.get());
    assert_eq!(cols, ((WIDTH - 16.0) / f32::from(cell_width)).floor() as u16);
    // The same size again is not a change.
    settle(cx, &opened);
    cx.executor().advance_clock(SETTLE * 2);
    cx.run_until_parked();
    assert_eq!(resizes(&opened).len(), 1);
}

#[gpui_kit::test]
fn a_resized_window_reports_the_new_room(cx: &mut TestAppContext) {
    let opened = open(cx);
    cx.executor().advance_clock(SETTLE * 2);
    cx.run_until_parked();
    cx.simulate_window_resize(opened.handle, size(px(WIDTH), px(HEIGHT / 2.0)));
    settle(cx, &opened);
    cx.executor().advance_clock(SETTLE * 2);
    cx.run_until_parked();
    let sent = resizes(&opened);
    assert_eq!(sent.len(), 2, "{sent:?}");
    assert_eq!(sent[1].1, (((HEIGHT / 2.0) - 16.0) / 20.0).floor() as u16);
}

#[gpui_kit::test]
fn a_size_that_changes_again_inside_the_settle_time_is_reported_once(cx: &mut TestAppContext) {
    let opened = open(cx);
    cx.executor().advance_clock(SETTLE * 2);
    cx.run_until_parked();
    for height in [600.0, 500.0, 400.0] {
        cx.simulate_window_resize(opened.handle, size(px(WIDTH), px(height)));
        settle(cx, &opened);
        cx.executor().advance_clock(SETTLE / 5);
    }
    cx.executor().advance_clock(SETTLE * 2);
    cx.run_until_parked();
    let sent = resizes(&opened);
    assert_eq!(sent.len(), 2, "the opening size and the last, not every step: {sent:?}");
    assert_eq!(sent[1].1, ((400.0_f32 - 16.0) / 20.0).floor() as u16);
}

#[test]
fn picking_another_pane_draws_nothing_until_its_first_frame_arrives() {
    let blank = Arc::new(Screen::new());
    let mut heard = Heard::new();
    assert!(drawn(&heard, Side::Workspace, &blank).pane().is_none(), "no pane on screen");
    heard.hear(rail(7));
    heard.hear(frame(7, Side::Workspace, "seven"));
    assert_eq!(drawn(&heard, Side::Workspace, &blank).row_text(0), "seven");
    heard.hear(rail(8));
    assert!(drawn(&heard, Side::Workspace, &blank).pane().is_none(), "the rail moved; pane 7's rows are not pane 8's");
    heard.hear(frame(8, Side::Workspace, "eight"));
    assert_eq!(drawn(&heard, Side::Workspace, &blank).row_text(0), "eight");
}

#[test]
fn a_console_side_draws_its_own_sides_rows_only_while_they_are_of_the_console_it_shows() {
    use argus_desktop_core::panes::console::ConsolePage;
    let blank = Arc::new(Screen::new());
    let mut heard = Heard::new();
    heard.hear(frame(5, Side::First, "first"));
    heard.hear(frame(6, Side::Second, "second"));
    assert!(drawn(&heard, Side::First, &blank).pane().is_none(), "the console shows nothing yet");
    let page = ConsolePage { showing: Some(5), beside: Some(6), ..ConsolePage::default() };
    heard.hear(Event::Console(Box::new(page)));
    assert_eq!(drawn(&heard, Side::First, &blank).row_text(0), "first");
    assert_eq!(drawn(&heard, Side::Second, &blank).row_text(0), "second");
    assert!(drawn(&heard, Side::Workspace, &blank).pane().is_none(), "the Workspace's rows are its own");
    let page = ConsolePage { showing: Some(6), ..ConsolePage::default() };
    heard.hear(Event::Console(Box::new(page)));
    assert!(drawn(&heard, Side::First, &blank).pane().is_none(), "another console is shown: pane 5's rows are not its");
}

#[test]
fn a_pane_taken_off_the_screen_leaves_the_stage_empty() {
    let blank = Arc::new(Screen::new());
    let mut heard = Heard::new();
    heard.hear(rail(7));
    heard.hear(frame(7, Side::Workspace, "seven"));
    heard.hear(Event::Rail(Box::new(Rail { loaded: true, ..Rail::default() })));
    assert!(drawn(&heard, Side::Workspace, &blank).pane().is_none());
}

#[gpui_kit::test]
fn no_room_is_reported_while_a_dock_sash_is_held_and_the_last_is_when_it_is_let_go(cx: &mut TestAppContext) {
    let opened = open(cx);
    cx.executor().advance_clock(SETTLE * 2);
    cx.run_until_parked();
    assert_eq!(resizes(&opened).len(), 1);
    let resizing = |cx: &mut TestAppContext, on: bool| {
        cx.update(|cx| {
            opened.heard.update(cx, |heard, cx| {
                if on {
                    heard.hold_resize(crate::layout::holders::Holder::Rail);
                } else {
                    heard.release_resize(crate::layout::holders::Holder::Rail);
                }
                cx.notify();
            })
        });
        cx.run_until_parked();
    };

    // SHELL-EDGE-1: the page's panes wait for a held sash to settle.
    resizing(cx, true);
    cx.simulate_window_resize(opened.handle, size(px(WIDTH), px(HEIGHT / 2.0)));
    settle(cx, &opened);
    cx.executor().advance_clock(SETTLE * 2);
    cx.run_until_parked();
    assert_eq!(resizes(&opened).len(), 1, "a size was reported mid-drag: {:?}", resizes(&opened));

    resizing(cx, false);
    let sent = resizes(&opened);
    assert_eq!(sent.len(), 2, "the size the gesture settled on is reported once it ends: {sent:?}");
    assert_eq!(sent[1].1, (((HEIGHT / 2.0) - 16.0) / 20.0).floor() as u16);
}
