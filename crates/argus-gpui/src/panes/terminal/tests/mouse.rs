//! The mouse over a screen: the cell under it, a drag that selects and copies on release, the
//! child's own reports, the press that claims the keys, and the wheel (SHELL-INPUT-5 to -10, -13).

use super::*;
use argus_desktop_core::connection::page::Parked;
use argus_desktop_core::panes::console::ConsolePage;
use crate::connection::screens::Marked;
use crate::panes::selection::Selection;
use gpui_kit::test::TestEventExt as _;
use gpui_kit::{point, Modifiers, MouseButton, Pixels, Point, ScrollDelta};

fn grid_frame(pane: u64, side: Side, mouse: bool, parked: Option<Parked>) -> Event {
    Event::Frame(Frame {
        pane,
        side,
        rows: 24,
        cols: 80,
        full: true,
        cursor: FrameCursor { row: 0, col: 0, visible: true, shape: "block" },
        alternate_screen: false,
        mouse,
        lines: vec![(0, vec![segment(0, "hello")])],
        parked,
    })
}

/// A window showing pane 7 with a grid, the child asking for the mouse or not.
fn showing(cx: &mut TestAppContext, mouse: bool) -> Opened {
    let opened = open(cx);
    hear(cx, &opened, vec![rail(7), grid_frame(7, Side::Workspace, mouse, None)]);
    settle(cx, &opened);
    opened
}

/// The middle of a cell, in window pixels.
fn middle(cx: &mut TestAppContext, opened: &Opened, row: u16, col: u16) -> Point<Pixels> {
    cx.update(|cx| {
        let terminal = opened.terminal.read(cx);
        let ((width, height), origin) = (terminal.cell.get(), terminal.origin.get());
        point(origin.x + width * (f32::from(col) + 0.5), origin.y + height * (f32::from(row) + 0.5))
    })
}

fn down(cx: &mut TestAppContext, opened: &Opened, at: Point<Pixels>, button: MouseButton) {
    cx.update_window(opened.handle, |_, window, cx| window.pointer_down(at, button, cx)).unwrap();
    cx.run_until_parked();
}

fn drag(cx: &mut TestAppContext, opened: &Opened, at: Point<Pixels>, button: MouseButton) {
    cx.update_window(opened.handle, |_, window, cx| window.pointer_move(at, Some(button), cx)).unwrap();
    cx.run_until_parked();
}

fn hover(cx: &mut TestAppContext, opened: &Opened, at: Point<Pixels>) {
    cx.update_window(opened.handle, |_, window, cx| window.pointer_move(at, None, cx)).unwrap();
    cx.run_until_parked();
}

fn hover_cell(cx: &mut TestAppContext, opened: &Opened, row: u16, col: u16) {
    let at = middle(cx, opened, row, col);
    hover(cx, opened, at);
}

fn drag_cell(cx: &mut TestAppContext, opened: &Opened, row: u16, col: u16) {
    let at = middle(cx, opened, row, col);
    drag(cx, opened, at, MouseButton::Left);
}

fn up(cx: &mut TestAppContext, opened: &Opened, at: Point<Pixels>, button: MouseButton) {
    cx.update_window(opened.handle, |_, window, cx| window.pointer_up(at, button, cx)).unwrap();
    cx.run_until_parked();
}

/// The calls the pointer made, as the words the session reads them in; the room and the window's
/// focus are not the pointer's.
fn said(opened: &Opened) -> Vec<String> {
    taken(opened)
        .into_iter()
        .filter(|call| !matches!(call, Call::Resize { .. }))
        .map(|call| format!("{call:?}"))
        .collect()
}

fn selection(cx: &mut TestAppContext, opened: &Opened) -> Option<Selection> {
    cx.update(|cx| opened.heard.read(cx).screens.selection_of(Side::Workspace))
}

#[gpui_kit::test]
fn a_drag_selects_and_letting_go_copies_between_its_two_cells(cx: &mut TestAppContext) {
    let opened = showing(cx, false);
    let (from, to) = (middle(cx, &opened, 2, 3), middle(cx, &opened, 4, 10));
    down(cx, &opened, from, MouseButton::Left);
    assert_eq!(selection(cx, &opened), None, "a press alone selects nothing");
    settle(cx, &opened);
    let before = paints(cx, &opened);
    drag(cx, &opened, to, MouseButton::Left);
    settle(cx, &opened);
    assert!(paints(cx, &opened) > before, "the selection was not drawn");
    assert_eq!(selection(cx, &opened), Some(Selection { anchor: (2, 3), head: (4, 10) }));
    up(cx, &opened, to, MouseButton::Left);
    assert_eq!(said(&opened), ["Copy { from: (2, 3), to: (4, 10) }"]);
    // SHELL-INPUT-13: it stays drawn after the copy.
    assert_eq!(selection(cx, &opened), Some(Selection { anchor: (2, 3), head: (4, 10) }));
}

#[gpui_kit::test]
fn a_drag_the_other_way_copies_from_where_it_began(cx: &mut TestAppContext) {
    let opened = showing(cx, false);
    let (from, to) = (middle(cx, &opened, 5, 20), middle(cx, &opened, 1, 4));
    down(cx, &opened, from, MouseButton::Left);
    drag(cx, &opened, to, MouseButton::Left);
    up(cx, &opened, to, MouseButton::Left);
    assert_eq!(said(&opened), ["Copy { from: (5, 20), to: (1, 4) }"]);
}

#[gpui_kit::test]
fn a_press_and_release_in_one_cell_copies_nothing_and_leaves_no_selection(cx: &mut TestAppContext) {
    let opened = showing(cx, false);
    let at = middle(cx, &opened, 2, 3);
    down(cx, &opened, at, MouseButton::Left);
    // Wandering inside the cell is no drag.
    drag(cx, &opened, point(at.x + px(1.0), at.y), MouseButton::Left);
    up(cx, &opened, at, MouseButton::Left);
    assert_eq!(said(&opened), Vec::<String>::new());
    assert_eq!(selection(cx, &opened), None);
}

#[gpui_kit::test]
fn a_drag_past_the_edge_of_the_window_selects_to_the_edge_of_the_grid(cx: &mut TestAppContext) {
    let opened = showing(cx, false);
    let from = middle(cx, &opened, 2, 3);
    down(cx, &opened, from, MouseButton::Left);
    let far = point(px(WIDTH * 3.0), px(HEIGHT * 3.0));
    drag(cx, &opened, far, MouseButton::Left);
    up(cx, &opened, far, MouseButton::Left);
    assert_eq!(said(&opened), ["Copy { from: (2, 3), to: (23, 79) }"], "the last cell of a 24 by 80 grid");
    let before = point(px(-50.0), px(-50.0));
    down(cx, &opened, from, MouseButton::Left);
    drag(cx, &opened, before, MouseButton::Left);
    up(cx, &opened, before, MouseButton::Left);
    assert_eq!(said(&opened)[1], "Copy { from: (2, 3), to: (0, 0) }");
}

#[gpui_kit::test]
fn a_right_or_middle_press_starts_no_selection(cx: &mut TestAppContext) {
    let opened = showing(cx, false);
    let (from, to) = (middle(cx, &opened, 2, 3), middle(cx, &opened, 4, 10));
    for button in [MouseButton::Right, MouseButton::Middle] {
        down(cx, &opened, from, button);
        drag(cx, &opened, to, button);
        up(cx, &opened, to, button);
    }
    assert_eq!(said(&opened), Vec::<String>::new());
    assert_eq!(selection(cx, &opened), None);
}

#[gpui_kit::test]
fn a_new_press_clears_the_selection_and_a_key_does_too(cx: &mut TestAppContext) {
    let opened = showing(cx, false);
    let (from, to) = (middle(cx, &opened, 2, 3), middle(cx, &opened, 4, 10));
    down(cx, &opened, from, MouseButton::Left);
    drag(cx, &opened, to, MouseButton::Left);
    up(cx, &opened, to, MouseButton::Left);
    assert!(selection(cx, &opened).is_some());
    down(cx, &opened, from, MouseButton::Right);
    assert_eq!(selection(cx, &opened), None, "a press on any screen clears every selection");
    up(cx, &opened, from, MouseButton::Right);

    down(cx, &opened, from, MouseButton::Left);
    drag(cx, &opened, to, MouseButton::Left);
    up(cx, &opened, to, MouseButton::Left);
    assert!(selection(cx, &opened).is_some());
    type_keys(cx, &opened, "a");
    assert_eq!(selection(cx, &opened), None, "every key press clears every selection");
}

#[gpui_kit::test]
fn a_press_on_one_screen_clears_the_selection_on_another(cx: &mut TestAppContext) {
    let opened = showing(cx, false);
    cx.update(|cx| {
        opened.heard.update(cx, |heard, cx| {
            heard.mark(Some(Marked { side: Side::First, selection: Selection { anchor: (0, 0), head: (1, 1) } }), cx)
        })
    });
    let at = middle(cx, &opened, 2, 3);
    down(cx, &opened, at, MouseButton::Left);
    cx.update(|cx| assert_eq!(opened.heard.read(cx).screens.marked(), None));
}

#[gpui_kit::test]
fn a_child_that_asked_for_the_mouse_gets_the_press_the_drag_and_the_release(cx: &mut TestAppContext) {
    let opened = showing(cx, true);
    let (from, to) = (middle(cx, &opened, 2, 3), middle(cx, &opened, 2, 6));
    down(cx, &opened, from, MouseButton::Left);
    drag_cell(cx, &opened, 2, 3);
    drag(cx, &opened, to, MouseButton::Left);
    drag(cx, &opened, to, MouseButton::Left);
    up(cx, &opened, to, MouseButton::Left);
    assert_eq!(
        said(&opened),
        [
            "Mouse { gesture: Down, button: 0, at: (2, 3) }",
            "Mouse { gesture: Drag, button: 0, at: (2, 6) }",
            "Mouse { gesture: Up, button: 0, at: (2, 6) }",
        ],
        "a drag in one cell is said once"
    );
    assert_eq!(selection(cx, &opened), None, "the child's press selects nothing here");
}

#[gpui_kit::test]
fn any_button_is_the_childs_and_names_itself(cx: &mut TestAppContext) {
    let opened = showing(cx, true);
    let at = middle(cx, &opened, 1, 1);
    for button in [MouseButton::Middle, MouseButton::Right] {
        down(cx, &opened, at, button);
        up(cx, &opened, at, button);
    }
    assert_eq!(
        said(&opened),
        [
            "Mouse { gesture: Down, button: 1, at: (1, 1) }",
            "Mouse { gesture: Up, button: 1, at: (1, 1) }",
            "Mouse { gesture: Down, button: 2, at: (1, 1) }",
            "Mouse { gesture: Up, button: 2, at: (1, 1) }",
        ]
    );
}

#[gpui_kit::test]
fn shift_keeps_a_press_for_a_selection_even_when_the_child_asked_for_the_mouse(cx: &mut TestAppContext) {
    let opened = showing(cx, true);
    let (from, to) = (middle(cx, &opened, 2, 3), middle(cx, &opened, 3, 3));
    cx.update_window(opened.handle, |_, window, cx| window.change_modifiers(Modifiers::shift(), cx)).unwrap();
    down(cx, &opened, from, MouseButton::Left);
    drag(cx, &opened, to, MouseButton::Left);
    up(cx, &opened, to, MouseButton::Left);
    assert_eq!(said(&opened), ["Copy { from: (2, 3), to: (3, 3) }"]);
}

#[gpui_kit::test]
fn a_parked_view_never_takes_the_mouse(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![rail(7), grid_frame(7, Side::Workspace, true, Some(Parked { offset: 2, depth: 9 }))]);
    settle(cx, &opened);
    let (from, to) = (middle(cx, &opened, 2, 3), middle(cx, &opened, 3, 3));
    down(cx, &opened, from, MouseButton::Left);
    drag(cx, &opened, to, MouseButton::Left);
    up(cx, &opened, to, MouseButton::Left);
    assert_eq!(said(&opened), ["Copy { from: (2, 3), to: (3, 3) }"]);
}

#[gpui_kit::test]
fn a_move_with_no_button_is_reported_once_a_cell_to_a_child_that_asked_for_it(cx: &mut TestAppContext) {
    let opened = showing(cx, true);
    hover_cell(cx, &opened, 4, 4);
    hover_cell(cx, &opened, 4, 4);
    hover_cell(cx, &opened, 4, 5);
    assert_eq!(
        said(&opened),
        ["Mouse { gesture: Move, button: 0, at: (4, 4) }", "Mouse { gesture: Move, button: 0, at: (4, 5) }"]
    );
}

#[gpui_kit::test]
fn a_move_is_the_childs_only_if_it_asked_for_the_mouse(cx: &mut TestAppContext) {
    let opened = showing(cx, false);
    hover_cell(cx, &opened, 4, 4);
    assert_eq!(said(&opened), Vec::<String>::new());
}

#[gpui_kit::test]
fn a_release_the_window_never_saw_ends_the_childs_press_and_copies_nothing(cx: &mut TestAppContext) {
    let opened = showing(cx, true);
    let at = middle(cx, &opened, 2, 3);
    down(cx, &opened, at, MouseButton::Left);
    // The next move says no button is held: the release went elsewhere.
    hover_cell(cx, &opened, 3, 3);
    assert_eq!(
        said(&opened),
        ["Mouse { gesture: Down, button: 0, at: (2, 3) }", "Mouse { gesture: Up, button: 0, at: (3, 3) }"]
    );
    hover_cell(cx, &opened, 5, 5);
    assert_eq!(said(&opened).len(), 3, "and the press is over: only the move that follows is said");
}

#[gpui_kit::test]
fn a_press_on_a_console_side_that_is_not_typed_in_asks_for_the_keys_and_waits(cx: &mut TestAppContext) {
    let opened = open_side(cx, Side::First);
    let page = ConsolePage { showing: Some(5), beside: Some(6), typing: None, ..ConsolePage::default() };
    hear(cx, &opened, vec![Event::Console(Box::new(page)), grid_frame(5, Side::First, false, None)]);
    let handle = opened.handle;
    let focused = |cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| opened.terminal.read(cx).focus_handle().is_focused(window)).unwrap()
    };
    // The window is a screen at its whole size: any point is on it.
    down(cx, &opened, point(px(100.0), px(100.0)), MouseButton::Left);
    assert_eq!(said(&opened), ["Console { gesture: Select { pane: 5 } }"]);
    assert!(!focused(cx), "the keys move when the session says so");
}

#[test]
fn a_press_asks_for_the_keys_the_way_its_side_does() {
    assert!(matches!(claiming(Side::Workspace, 3), Call::Select { pane: 3 }));
    assert!(matches!(claiming(Side::Beside(1), 3), Call::FocusView { pane: 3 }));
    assert!(matches!(claiming(Side::View(0), 3), Call::FocusView { pane: 3 }));
    assert!(matches!(
        claiming(Side::First, 3),
        Call::Console { gesture: ConsoleGesture::Select { pane: 3 } }
    ));
    assert!(matches!(
        claiming(Side::Second, 3),
        Call::Console { gesture: ConsoleGesture::Select { pane: 3 } }
    ));
}

#[test]
fn a_button_is_the_doms_number() {
    use super::super::pointer::dom_button;
    use gpui_kit::NavigationDirection::{Back, Forward};
    assert_eq!(
        [
            MouseButton::Left,
            MouseButton::Middle,
            MouseButton::Right,
            MouseButton::Navigate(Back),
            MouseButton::Navigate(Forward)
        ]
        .map(dom_button),
        [0, 1, 2, 3, 4]
    );
}

fn wheel(cx: &mut TestAppContext, opened: &Opened, delta: ScrollDelta) {
    cx.update_window(opened.handle, |_, window, cx| window.scroll("terminal", delta, cx)).unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn a_wheel_is_counted_in_notches_negative_up_for_the_pane_under_the_pointer(cx: &mut TestAppContext) {
    let opened = showing(cx, false);
    // GPUI's y is positive up: a roll toward the user is down, one notch of three lines.
    wheel(cx, &opened, ScrollDelta::Lines(point(0.0, -3.0)));
    wheel(cx, &opened, ScrollDelta::Lines(point(0.0, 6.0)));
    let calls = said(&opened);
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert!(calls[0].starts_with("Wheel { pane: 7, notches: 1, at: ("), "{calls:?}");
    assert!(calls[1].starts_with("Wheel { pane: 7, notches: -2, at: ("), "{calls:?}");
}

#[gpui_kit::test]
fn a_touchpads_small_deltas_add_up_to_a_notch(cx: &mut TestAppContext) {
    let opened = showing(cx, false);
    for _ in 0..3 {
        wheel(cx, &opened, ScrollDelta::Pixels(point(px(0.0), px(-40.0))));
    }
    let calls = said(&opened);
    assert_eq!(calls.len(), 1, "40 and 80 are not a notch, 120 is: {calls:?}");
    assert!(calls[0].starts_with("Wheel { pane: 7, notches: 1,"), "{calls:?}");
}

#[gpui_kit::test]
fn a_wheel_over_a_screen_with_no_pane_says_nothing(cx: &mut TestAppContext) {
    let opened = open(cx);
    wheel(cx, &opened, ScrollDelta::Lines(point(0.0, -3.0)));
    assert_eq!(said(&opened), Vec::<String>::new());
}
