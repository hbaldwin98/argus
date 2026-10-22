use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use gpui_kit::px;
use gpui_kit::component::h_flex;
use gpui_kit::test::{TestEventExt as _, TestWindowExt as _};
use gpui_kit::{point, size, AnyWindowHandle, Bounds, Pixels, Point, TestAppContext, WindowBounds, WindowOptions};

const HELD: Held = Held { from: 400.0, start: 320.0, zoom: 1.0, mirror: false };

#[test]
fn the_column_keeps_its_offset_from_the_pointer_and_does_not_jump() {
    assert_eq!(width_at(HELD, 400.0, 200.0, 520.0), 320.0);
    assert_eq!(width_at(HELD, 450.0, 200.0, 520.0), 370.0);
    assert_eq!(width_at(HELD, 380.0, 200.0, 520.0), 300.0);
}

/// The width is the page's pixels and the pointer the window's: at 200% the same travel is half the width.
#[test]
fn a_zoomed_press_moves_the_column_by_the_travel_over_the_zoom() {
    let zoomed = Held { zoom: 2.0, ..HELD };
    assert_eq!(width_at(zoomed, 450.0, 200.0, 520.0), 345.0);
    assert_eq!(width_at(zoomed, 360.0, 200.0, 520.0), 300.0);
}

#[test]
fn the_column_stays_within_its_bounds() {
    assert_eq!(width_at(HELD, 0.0, 200.0, 520.0), 200.0);
    assert_eq!(width_at(HELD, 9000.0, 200.0, 520.0), 520.0);
}

/// A column at the right has its edge on its left: the pointer moving left widens it.
#[test]
fn a_mirrored_column_grows_as_the_pointer_goes_left() {
    let held = Held { mirror: true, ..HELD };
    assert_eq!(width_at(held, 400.0, 200.0, 520.0), 320.0);
    assert_eq!(width_at(held, 350.0, 200.0, 520.0), 370.0);
    assert_eq!(width_at(held, 450.0, 200.0, 520.0), 270.0);
    assert_eq!(width_at(held, 9000.0, 200.0, 520.0), 200.0);
}

/// What a split told whoever listens, in order.
type Said = Rc<RefCell<Vec<String>>>;

struct Host {
    split: Entity<Split>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        h_flex().size_full().child(side(&self.split, "column", div().size_full(), &tokens, cx)).child(div().flex_1())
    }
}

struct Opened {
    handle: AnyWindowHandle,
    split: Entity<Split>,
    said: Said,
}

fn open(cx: &mut TestAppContext, reporting: bool) -> Opened {
    cx.update(crate::window::init);
    let said: Said = Rc::default();
    let mut built = None;
    let (handle, _) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(Default::default(), size(px(900.0), px(400.0))))),
                ..Default::default()
            },
            cx,
            |_, cx| {
                let (reports, holds) = (said.clone(), said.clone());
                let split = cx.new(|cx| {
                    let split = Split::new("test-split", "Resize the column", 300.0, 200.0, 500.0, cx).holding(move |on, _| {
                        holds.borrow_mut().push(format!("hold {on}"));
                    });
                    if reporting {
                        split.reporting(move |width, _, _| reports.borrow_mut().push(format!("report {width:?}")))
                    } else {
                        split
                    }
                });
                built = Some(split.clone());
                cx.new(|_| Host { split })
            },
        )
        .expect("open test window")
    });
    let opened = Opened { handle, split: built.unwrap(), said };
    settle(cx, &opened);
    opened
}

fn settle(cx: &mut TestAppContext, opened: &Opened) {
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

fn sash(cx: &mut TestAppContext, opened: &Opened) -> Point<Pixels> {
    cx.update_window(opened.handle, |_, window, _| window.find("test-split".to_owned()).bounds().center()).unwrap()
}

fn width(cx: &mut TestAppContext, opened: &Opened) -> f32 {
    cx.update(|cx| opened.split.read(cx).width())
}

fn by(at: Point<Pixels>, dx: f32) -> Point<Pixels> {
    point(at.x + px(dx), at.y)
}

fn press(cx: &mut TestAppContext, opened: &Opened, at: Point<Pixels>) {
    cx.update_window(opened.handle, |_, window, cx| window.pointer_down(at, MouseButton::Left, cx)).unwrap();
    settle(cx, opened);
}

fn drag_to(cx: &mut TestAppContext, opened: &Opened, at: Point<Pixels>) {
    cx.update_window(opened.handle, |_, window, cx| window.pointer_move(at, Some(MouseButton::Left), cx)).unwrap();
    settle(cx, opened);
}

fn release(cx: &mut TestAppContext, opened: &Opened, at: Point<Pixels>) {
    cx.update_window(opened.handle, |_, window, cx| window.pointer_up(at, MouseButton::Left, cx)).unwrap();
    settle(cx, opened);
}

#[gpui_kit::test]
fn a_drag_is_drawn_live_and_reported_once_on_release_as_whole_pixels(cx: &mut TestAppContext) {
    let opened = open(cx, true);
    let grab = sash(cx, &opened);
    press(cx, &opened, grab);
    drag_to(cx, &opened, by(grab, 60.0));
    assert_eq!(width(cx, &opened), 360.0, "drawn while held");
    drag_to(cx, &opened, by(grab, 80.0));
    assert_eq!(*opened.said.borrow(), ["hold true"], "nothing is sent while the sash is held");
    release(cx, &opened, by(grab, 80.0));
    assert_eq!(*opened.said.borrow(), ["hold true", "hold false", "report Some(380)"]);
}

#[gpui_kit::test]
fn a_drag_stays_within_the_bounds_it_is_kept_to(cx: &mut TestAppContext) {
    let opened = open(cx, true);
    let grab = sash(cx, &opened);
    press(cx, &opened, grab);
    drag_to(cx, &opened, by(grab, 900.0));
    release(cx, &opened, by(grab, 900.0));
    assert_eq!(opened.said.borrow().last().map(String::as_str), Some("report Some(500)"));
}

#[gpui_kit::test]
fn escape_puts_the_width_back_and_reports_nothing(cx: &mut TestAppContext) {
    let opened = open(cx, true);
    let grab = sash(cx, &opened);
    press(cx, &opened, grab);
    drag_to(cx, &opened, by(grab, 60.0));
    cx.update_window(opened.handle, |_, window, cx| window.press("escape", cx)).unwrap();
    settle(cx, &opened);
    release(cx, &opened, by(grab, 60.0));
    assert_eq!(width(cx, &opened), 300.0);
    assert_eq!(*opened.said.borrow(), ["hold true", "hold false"], "a cancelled drag reports nothing and lets the hold go");
}

#[gpui_kit::test]
fn a_press_that_does_not_travel_is_not_a_resize(cx: &mut TestAppContext) {
    let opened = open(cx, true);
    let grab = sash(cx, &opened);
    press(cx, &opened, grab);
    release(cx, &opened, by(grab, 2.0));
    assert_eq!(width(cx, &opened), 300.0);
    assert!(opened.said.borrow().is_empty(), "{:?}", opened.said.borrow());
}

#[gpui_kit::test]
fn a_double_click_gives_back_the_usual_width_and_reports_none(cx: &mut TestAppContext) {
    let opened = open(cx, true);
    let grab = sash(cx, &opened);
    press(cx, &opened, grab);
    drag_to(cx, &opened, by(grab, 100.0));
    release(cx, &opened, by(grab, 100.0));
    assert_eq!(width(cx, &opened), 400.0);
    opened.said.borrow_mut().clear();
    cx.update_window(opened.handle, |_, window, cx| window.double_click("test-split".to_owned(), cx)).unwrap();
    settle(cx, &opened);
    assert_eq!(width(cx, &opened), 300.0);
    assert_eq!(*opened.said.borrow(), ["report None"]);
}

#[gpui_kit::test]
fn a_split_that_only_keeps_its_width_sends_nothing_and_ignores_a_double_click(cx: &mut TestAppContext) {
    let opened = open(cx, false);
    let grab = sash(cx, &opened);
    press(cx, &opened, grab);
    drag_to(cx, &opened, by(grab, 100.0));
    release(cx, &opened, by(grab, 100.0));
    assert_eq!(width(cx, &opened), 400.0);
    cx.update_window(opened.handle, |_, window, cx| window.double_click("test-split".to_owned(), cx)).unwrap();
    settle(cx, &opened);
    assert_eq!(width(cx, &opened), 400.0, "a double-click is the reporting splits' reset");
}

#[gpui_kit::test]
fn what_rust_says_is_drawn_unless_the_sash_is_held_and_is_held_to_the_bounds(cx: &mut TestAppContext) {
    let opened = open(cx, true);
    cx.update(|cx| opened.split.update(cx, |split, cx| split.follow(450.0, cx)));
    settle(cx, &opened);
    assert_eq!(width(cx, &opened), 450.0);
    cx.update(|cx| opened.split.update(cx, |split, cx| split.follow(9000.0, cx)));
    assert_eq!(width(cx, &opened), 500.0);

    let grab = sash(cx, &opened);
    press(cx, &opened, grab);
    drag_to(cx, &opened, by(grab, -50.0));
    let held = width(cx, &opened);
    cx.update(|cx| opened.split.update(cx, |split, cx| split.follow(250.0, cx)));
    assert_eq!(width(cx, &opened), held, "an answer arriving mid-drag moved the column under the pointer");
}
