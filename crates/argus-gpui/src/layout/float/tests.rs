use argus_desktop_core::connection::page::{Connection, Event};
use argus_desktop_core::layout::docks::{DocksPage, FloatBounds, FloatPage, Panel, PanelKind, PresentPanel};
use argus_desktop_core::panes::console::ConsolePlace;
use gpui_kit::{px, Bounds, Pixels, Point, TestAppContext};

use super::*;
use crate::layout::work::tests::{
    bounds, by, centre, console, docks, down, drag_to, escape, hear, open, present_id, status, taken, up, Opened,
};

fn saved(name: &str) -> Panel {
    Panel::Saved(name.into())
}

fn at(x: i32, y: i32, width: u32, height: u32) -> FloatBounds {
    FloatBounds { x, y, width, height }
}

/// The base docks with the saved views `a` (behind) and `b` (in front) floating, `a` one that
/// closes and `b` one that only hides.
fn floating() -> DocksPage {
    let mut page = docks();
    for (name, bounds, closes) in [("a", at(96, 64, 560, 420), true), ("b", at(124, 92, 400, 300), false)] {
        page.floating.push(FloatPage { panel: saved(name), title: format!("View {name}"), bounds });
        page.present.push(PresentPanel { panel: saved(name), kind: PanelKind::Saved, floats: true, stage: false, closes });
    }
    page
}

fn opened(cx: &mut TestAppContext) -> Opened {
    open(cx, vec![status(Connection::Connected), console(ConsolePlace::Docked), Event::Docks(Box::new(floating()))])
}

fn frame(cx: &mut TestAppContext, opened: &Opened, name: &str) -> Bounds<Pixels> {
    bounds(cx, opened, &format!("float-{}", saved(name)))
}

fn head(cx: &mut TestAppContext, opened: &Opened, name: &str) -> Point<Pixels> {
    // The title's own corner: the middle of the head is the spacer, which presses the head too.
    let b = bounds(cx, opened, &format!("float-head-{}", saved(name)));
    by(b.origin, 40.0, b.size.height.as_f32() / 2.0)
}

fn grip(cx: &mut TestAppContext, opened: &Opened, name: &str) -> Point<Pixels> {
    centre(cx, opened, &format!("float-grip-{}", saved(name)))
}

fn relative(cx: &mut TestAppContext, opened: &Opened, name: &str) -> (f32, f32, f32, f32) {
    let (layer, frame) = (bounds(cx, opened, "floats"), frame(cx, opened, name));
    ((frame.origin.x - layer.origin.x).as_f32(), (frame.origin.y - layer.origin.y).as_f32(), frame.size.width.as_f32(), frame.size.height.as_f32())
}

#[test]
fn a_box_held_is_the_box_the_pointer_made_inside_the_cores_rules() {
    let (from, to) = (gpui_kit::point(px(10.0), px(10.0)), gpui_kit::point(px(50.4), px(-30.0)));
    let start = at(96, 64, 560, 420);
    assert_eq!(held(Grip::Move, start, from, to, (1000, 700), 1.0), at(136, 24, 560, 420), "whole pixels, moved by the travel");
    assert_eq!(held(Grip::Size, start, from, to, (1000, 700), 1.0), at(96, 64, 600, 380));
    assert_eq!(held(Grip::Size, start, from, gpui_kit::point(px(-900.0), px(-900.0)), (1000, 700), 1.0).width, 280, "SHELL-FLOAT-4: least 280 wide");
    assert_eq!(held(Grip::Size, start, from, gpui_kit::point(px(-900.0), px(-900.0)), (1000, 700), 1.0).height, 160, "and 160 tall");
}

#[test]
fn a_box_let_go_unchanged_reports_nothing_and_a_changed_one_reports_whole_pixels() {
    let start = at(96, 64, 560, 420);
    assert!(released(saved("a"), start, start).is_none());
    let call = released(saved("a"), start, at(100, 64, 560, 420)).expect("moved");
    assert_eq!(format!("{call:?}"), "Docks { gesture: Bounds { panel: Saved(\"a\"), bounds: FloatBounds { x: 100, y: 64, width: 560, height: 420 } } }");
}

#[gpui_kit::test]
fn each_dialog_is_drawn_where_rust_says_in_the_order_rust_says(cx: &mut TestAppContext) {
    let opened = opened(cx);
    assert_eq!(relative(cx, &opened, "a"), (96.0, 64.0, 560.0, 420.0));
    assert_eq!(relative(cx, &opened, "b"), (124.0, 92.0, 400.0, 300.0));
    assert_eq!(label_of(cx, &opened, "float-saved:a").as_deref(), Some("View a"));
    // Back to front: the one last is in front, so a point both hold lands on it.
    let point = by(frame(cx, &opened, "b").origin, 100.0, 100.0);
    assert!(frame(cx, &opened, "a").contains(&point) && frame(cx, &opened, "b").contains(&point));
    // A dialog no longer floating is gone.
    hear(cx, &opened, vec![Event::Docks(Box::new(docks()))]);
    assert!(!present_id(cx, &opened, "float-saved:a") && !present_id(cx, &opened, "float-saved:b"));
}

fn label_of(cx: &mut TestAppContext, opened: &Opened, id: &str) -> Option<String> {
    crate::layout::work::tests::label(cx, opened, id)
}

#[gpui_kit::test]
fn a_press_on_a_dialog_behind_raises_it_and_one_on_the_front_sends_nothing(cx: &mut TestAppContext) {
    let opened = opened(cx);
    let behind = by(frame(cx, &opened, "a").origin, 30.0, 380.0);
    assert!(!frame(cx, &opened, "b").contains(&behind), "the point is only on the dialog behind");
    down(cx, &opened, behind);
    up(cx, &opened, behind);
    assert_eq!(taken(&opened), ["Docks { gesture: Raise { panel: Saved(\"a\") } }"]);
    opened.calls.borrow_mut().clear();
    let front = by(frame(cx, &opened, "b").origin, 100.0, 150.0);
    down(cx, &opened, front);
    up(cx, &opened, front);
    assert!(taken(&opened).is_empty(), "the one in front has nothing to be raised over");
}

#[gpui_kit::test]
fn a_press_on_a_dialog_does_not_reach_the_stage_under_it(cx: &mut TestAppContext) {
    let opened = opened(cx);
    let inside = by(frame(cx, &opened, "b").origin, 100.0, 150.0);
    down(cx, &opened, inside);
    up(cx, &opened, inside);
    assert_eq!(opened.ups.get(), 0, "the stage was pressed through the dialog");
}

#[gpui_kit::test]
fn the_head_moves_the_dialog_live_and_reports_the_box_once_on_release(cx: &mut TestAppContext) {
    let opened = opened(cx);
    let grab = head(cx, &opened, "b");
    down(cx, &opened, grab);
    drag_to(cx, &opened, by(grab, 40.0, 30.0));
    assert_eq!(relative(cx, &opened, "b"), (164.0, 122.0, 400.0, 300.0), "it follows the pointer while held");
    assert!(taken(&opened).iter().all(|call| !call.contains("Bounds")), "nothing is sent while it is held");
    // A redraw from Rust (here, the same page again) leaves it where the pointer has it.
    hear(cx, &opened, vec![Event::Docks(Box::new(floating()))]);
    assert_eq!(relative(cx, &opened, "b").0, 164.0);
    up(cx, &opened, by(grab, 40.0, 30.0));
    let sent: Vec<String> = taken(&opened).into_iter().filter(|call| call.contains("Bounds")).collect();
    assert_eq!(sent, ["Docks { gesture: Bounds { panel: Saved(\"b\"), bounds: FloatBounds { x: 164, y: 122, width: 400, height: 300 } } }"]);
    // Let go, the box is Rust's again until the next `Docks` event says otherwise.
    assert_eq!(relative(cx, &opened, "b").0, 124.0);
}

#[gpui_kit::test]
fn a_press_on_the_head_that_stays_under_five_pixels_moves_and_sends_nothing(cx: &mut TestAppContext) {
    let opened = opened(cx);
    let grab = head(cx, &opened, "b");
    down(cx, &opened, grab);
    drag_to(cx, &opened, by(grab, 3.0, 0.0));
    assert_eq!(relative(cx, &opened, "b").0, 124.0);
    up(cx, &opened, by(grab, 3.0, 0.0));
    assert!(taken(&opened).iter().all(|call| !call.contains("Bounds")));
}

#[gpui_kit::test]
fn escape_while_moving_puts_the_dialog_back_and_sends_no_box(cx: &mut TestAppContext) {
    let opened = opened(cx);
    let grab = head(cx, &opened, "b");
    down(cx, &opened, grab);
    drag_to(cx, &opened, by(grab, 80.0, 0.0));
    escape(cx, &opened);
    up(cx, &opened, by(grab, 80.0, 0.0));
    assert_eq!(relative(cx, &opened, "b").0, 124.0);
    assert!(taken(&opened).iter().all(|call| !call.contains("Bounds")));
}

#[gpui_kit::test]
fn the_corner_resizes_no_smaller_than_280_by_160_and_reports_the_box(cx: &mut TestAppContext) {
    let opened = opened(cx);
    let grab = grip(cx, &opened, "b");
    down(cx, &opened, grab);
    drag_to(cx, &opened, by(grab, 60.0, 20.0));
    assert_eq!(relative(cx, &opened, "b"), (124.0, 92.0, 460.0, 320.0));
    drag_to(cx, &opened, by(grab, -900.0, -900.0));
    assert_eq!(relative(cx, &opened, "b"), (124.0, 92.0, 280.0, 160.0), "SHELL-FLOAT-4");
    up(cx, &opened, by(grab, -900.0, -900.0));
    let sent: Vec<String> = taken(&opened).into_iter().filter(|call| call.contains("Bounds")).collect();
    assert_eq!(sent, ["Docks { gesture: Bounds { panel: Saved(\"b\"), bounds: FloatBounds { x: 124, y: 92, width: 280, height: 160 } } }"]);
}

#[gpui_kit::test]
fn a_dialog_dragged_out_keeps_120_px_across_and_40_px_of_head_in_reach(cx: &mut TestAppContext) {
    let opened = opened(cx);
    let layer = bounds(cx, &opened, "floats");
    let grab = head(cx, &opened, "b");
    down(cx, &opened, grab);
    drag_to(cx, &opened, by(grab, 5000.0, 5000.0));
    let (x, y, _, _) = relative(cx, &opened, "b");
    assert_eq!(x, layer.size.width.as_f32() - 120.0, "SHELL-FLOAT-5: 120 px stay in horizontally");
    assert_eq!(y, layer.size.height.as_f32() - 40.0, "and 40 px stay above the bottom");
    drag_to(cx, &opened, by(grab, -5000.0, -5000.0));
    let (x, y, width, _) = relative(cx, &opened, "b");
    assert_eq!((x, y), (120.0 - width, 0.0), "the top never goes above the area");
    up(cx, &opened, by(grab, -5000.0, -5000.0));
    let sent = taken(&opened).into_iter().find(|call| call.contains("Bounds")).expect("reported");
    assert!(sent.contains("y: 0"), "{sent}: what is drawn is what is reported");
}

#[gpui_kit::test]
fn a_frame_rust_places_outside_the_area_is_drawn_clamped_into_it(cx: &mut TestAppContext) {
    // SHELL-FLOAT-5: clamped on every draw, not only while held.
    let mut page = floating();
    page.floating[0].bounds = at(5000, 9000, 560, 420);
    page.floating[1].bounds = at(0, 0, 9000, 9000);
    let opened = open(cx, vec![status(Connection::Connected), console(ConsolePlace::Docked), Event::Docks(Box::new(page))]);
    let layer = bounds(cx, &opened, "floats");
    let (area_w, area_h) = (layer.size.width.as_f32(), layer.size.height.as_f32());
    let (x, y, width, _) = relative(cx, &opened, "a");
    assert_eq!((x, y), (area_w - 120.0, area_h - 40.0), "120 px of the width stay in, 40 px of head above the bottom");
    assert_eq!(width, 560.0);
    let (_, _, width, height) = relative(cx, &opened, "b");
    assert_eq!((width, height), (area_w, area_h), "no bigger than the area");
}

#[gpui_kit::test]
fn a_dialogs_close_sends_close_for_a_view_that_closes_and_hide_for_one_that_does_not(cx: &mut TestAppContext) {
    let opened = opened(cx);
    for (name, call) in [("a", "Close"), ("b", "Hide")] {
        let button = centre(cx, &opened, &format!("float-close-{}", saved(name)));
        down(cx, &opened, button);
        up(cx, &opened, button);
        let sent: Vec<String> = taken(&opened).into_iter().filter(|sent| sent.contains(call)).collect();
        assert_eq!(sent, [format!("Docks {{ gesture: {call} {{ panel: Saved(\"{name}\") }} }}")]);
        opened.calls.borrow_mut().clear();
    }
}

#[gpui_kit::test]
fn a_stopped_daemon_leaves_the_dialogs_answering_no_press(cx: &mut TestAppContext) {
    let opened = opened(cx);
    hear(cx, &opened, vec![status(Connection::Stopped)]);
    let behind = by(frame(cx, &opened, "a").origin, 30.0, 380.0);
    down(cx, &opened, behind);
    up(cx, &opened, behind);
    let grab = head(cx, &opened, "b");
    down(cx, &opened, grab);
    drag_to(cx, &opened, by(grab, 80.0, 0.0));
    assert_eq!(relative(cx, &opened, "b").0, 124.0, "held, it does not move");
    up(cx, &opened, by(grab, 80.0, 0.0));
    assert!(taken(&opened).is_empty());
}
