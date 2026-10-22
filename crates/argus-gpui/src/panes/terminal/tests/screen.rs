//! What a screen holds and shows around its rows: the skeleton, the palette, the size label and
//! the screens of the sides a view binds (SHELL-SCREEN-1, -10, -12, -13, -14).

use std::cell::RefCell;

use super::skeleton::DELAY;
use super::*;
use crate::panes::size_label::SHOWN_FOR;
use crate::theme::themes::ledger;
use crate::theme::kit;
use argus_desktop_core::connection::page::Parked;

const BARS: &str = "terminal-skeleton";

fn skeleton_up(cx: &mut TestAppContext, opened: &Opened) -> bool {
    cx.update_window(opened.handle, |_, window, _| window.try_find(BARS.to_owned()).is_some()).unwrap()
}

fn wait(cx: &mut TestAppContext, opened: &Opened, by: Duration) {
    cx.executor().advance_clock(by);
    settle(cx, opened);
}

#[gpui_kit::test]
fn bars_stand_in_for_a_pane_that_has_sent_nothing_after_250_ms_and_not_before(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![rail(8)]);
    wait(cx, &opened, DELAY - Duration::from_millis(10));
    assert!(!skeleton_up(cx, &opened), "{BARS} showed before its delay");
    wait(cx, &opened, Duration::from_millis(20));
    assert!(skeleton_up(cx, &opened), "no frame came and no bars stood in");
}

#[gpui_kit::test]
fn the_first_frame_takes_the_bars_away_and_one_inside_the_delay_never_shows_them(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![rail(8)]);
    wait(cx, &opened, DELAY + Duration::from_millis(10));
    assert!(skeleton_up(cx, &opened));
    hear(cx, &opened, vec![frame(8, Side::Workspace, "eight")]);
    settle(cx, &opened);
    assert!(!skeleton_up(cx, &opened), "the pane's first screen came and the bars stayed");

    // A live pane answers well inside the delay and the bars never flash.
    hear(cx, &opened, vec![rail(9)]);
    wait(cx, &opened, Duration::from_millis(200));
    hear(cx, &opened, vec![frame(9, Side::Workspace, "nine")]);
    wait(cx, &opened, Duration::from_secs(1));
    assert!(!skeleton_up(cx, &opened));
}

#[gpui_kit::test]
fn picking_another_pane_starts_the_wait_over_and_picking_none_ends_it(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![rail(8)]);
    wait(cx, &opened, Duration::from_millis(200));
    hear(cx, &opened, vec![rail(9)]);
    wait(cx, &opened, Duration::from_millis(100));
    assert!(!skeleton_up(cx, &opened), "pane 8's 200 ms counted toward pane 9's wait");
    wait(cx, &opened, Duration::from_millis(200));
    assert!(skeleton_up(cx, &opened));
    hear(cx, &opened, vec![Event::Rail(Box::new(Rail { loaded: true, ..Rail::default() }))]);
    settle(cx, &opened);
    assert!(!skeleton_up(cx, &opened), "nothing on screen, nothing to wait for");
}

#[gpui_kit::test]
fn a_theme_installed_over_the_screen_changes_its_palette_and_asks_for_no_new_room(cx: &mut TestAppContext) {
    let opened = open(cx);
    cx.executor().advance_clock(SETTLE * 2);
    cx.run_until_parked();
    let before = cx.update(|cx| opened.terminal.read(cx).palette().clone());
    let paints_before = paints(cx, &opened);
    let sent_before = resizes(&opened).len();
    cx.update(|cx| {
        let mut other = ledger::tokens();
        other.fg = other.ansi[1];
        other.bg_sunken = other.ansi[2];
        other.ansi[1] = other.ansi[4];
        kit::install(cx, other, 100);
    });
    settle(cx, &opened);
    let after = cx.update(|cx| opened.terminal.read(cx).palette().clone());
    assert_ne!(before.fg, after.fg);
    assert_ne!(before.bg, after.bg);
    assert_ne!(before.ansi[1], after.ansi[1]);
    assert!(paints(cx, &opened) > paints_before, "the screen was not repainted in the new colours");
    cx.executor().advance_clock(SETTLE * 2);
    cx.run_until_parked();
    assert_eq!(resizes(&opened).len(), sent_before, "a theme is not a new size");
}

fn labelled(cx: &mut TestAppContext, opened: &Opened) -> Rc<RefCell<Vec<String>>> {
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let sink = log.clone();
    cx.update(|cx| {
        cx.subscribe(&opened.terminal, move |_, Labelled(words): &Labelled, _| sink.borrow_mut().push(words.clone())).detach()
    });
    log
}

fn sized(pane: u64, rows: u16, cols: u16, parked: Option<Parked>) -> Event {
    Event::Frame(Frame {
        pane,
        side: Side::Workspace,
        rows,
        cols,
        full: true,
        cursor: FrameCursor { row: 0, col: 0, visible: true, shape: "block" },
        alternate_screen: false,
        mouse: false,
        lines: Vec::new(),
        parked,
    })
}

#[gpui_kit::test]
fn the_size_shows_for_a_second_after_a_resize_and_the_header_is_told_when_it_stops(cx: &mut TestAppContext) {
    let opened = open(cx);
    let log = labelled(cx, &opened);
    hear(cx, &opened, vec![rail(7), sized(7, 24, 80, None)]);
    assert!(log.borrow().is_empty(), "the first grid is nobody's resize");
    hear(cx, &opened, vec![sized(7, 30, 100, None)]);
    assert_eq!(*log.borrow(), ["100 \u{d7} 30"]);
    assert_eq!(cx.update(|cx| opened.terminal.read(cx).size_label(cx).to_owned()), "100 \u{d7} 30");
    wait(cx, &opened, SHOWN_FOR - Duration::from_millis(100));
    assert_eq!(log.borrow().len(), 1, "still showing");
    wait(cx, &opened, Duration::from_millis(200));
    assert_eq!(*log.borrow(), ["100 \u{d7} 30", ""], "at rest it says nothing");
}

#[gpui_kit::test]
fn a_parked_views_place_is_said_at_rest_and_the_size_follows_it_while_it_shows(cx: &mut TestAppContext) {
    let opened = open(cx);
    let log = labelled(cx, &opened);
    hear(cx, &opened, vec![rail(7), sized(7, 24, 80, Some(Parked { offset: 3, depth: 40 }))]);
    assert_eq!(*log.borrow(), ["\u{2191} 3/40"]);
    hear(cx, &opened, vec![sized(7, 30, 100, Some(Parked { offset: 3, depth: 40 }))]);
    assert_eq!(log.borrow().last().map(String::as_str), Some("\u{2191} 3/40 \u{b7} 100 \u{d7} 30"));
    wait(cx, &opened, SHOWN_FOR + Duration::from_millis(100));
    assert_eq!(log.borrow().last().map(String::as_str), Some("\u{2191} 3/40"));
}

#[gpui_kit::test]
fn the_header_says_nothing_of_a_grid_that_is_not_of_the_pane_on_screen(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![rail(7), sized(7, 24, 80, Some(Parked { offset: 1, depth: 2 }))]);
    assert_eq!(cx.update(|cx| opened.terminal.read(cx).size_label(cx).to_owned()), "\u{2191} 1/2");
    hear(cx, &opened, vec![rail(8)]);
    assert_eq!(cx.update(|cx| opened.terminal.read(cx).size_label(cx).to_owned()), "", "pane 7's place under pane 8");
}

#[gpui_kit::test]
fn waiting_and_labelling_keep_no_timer_going_at_rest(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![rail(7), sized(7, 24, 80, None), sized(7, 30, 100, None)]);
    wait(cx, &opened, SHOWN_FOR + Duration::from_millis(100));
    let before = paints(cx, &opened);
    cx.executor().advance_clock(Duration::from_secs(5));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(5));
    cx.run_until_parked();
    assert_eq!(paints(cx, &opened), before, "something is still driving the screen");
}

#[gpui_kit::test]
fn a_split_cells_screen_holds_frames_while_its_view_lives_and_none_after(cx: &mut TestAppContext) {
    let Opened { handle, terminal, heard, events, .. } = open_side(cx, Side::Beside(0));
    let hear = |cx: &mut TestAppContext, event: Event| {
        events.send(vec![event]).unwrap();
        cx.run_until_parked();
    };
    let pane_of = |cx: &mut TestAppContext, side: Side| cx.update(|cx| heard.read(cx).screen_of(side).pane());
    hear(cx, frame(4, Side::Beside(0), "beside"));
    assert_eq!(pane_of(cx, Side::Beside(0)), Some(4));
    hear(cx, frame(4, Side::Beside(1), "not bound"));
    assert_eq!(pane_of(cx, Side::Beside(1)), None, "no view for that side");
    // The window held the view and so did this test; both let go.
    drop(terminal);
    cx.update_window(handle, |_, window, _| window.remove_window()).unwrap();
    cx.run_until_parked();
    hear(cx, frame(4, Side::Beside(0), "after"));
    assert_eq!(pane_of(cx, Side::Beside(0)), None, "the view is gone, so is the screen");
}
