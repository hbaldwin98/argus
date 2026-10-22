use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use gpui_kit::px;
use gpui_kit::component::v_flex;
use gpui_kit::test::{TestEventExt as _, TestQueryExt as _, TestWindowExt as _};
use gpui_kit::{
    size, AnyWindowHandle, Bounds, Context, MouseButton, Render, TestAppContext, WindowBounds, WindowOptions,
};

type Asked = Rc<RefCell<Vec<String>>>;

/// A ticked box, an unticked one, a box that cannot be ticked, and two rings with the first chosen.
struct Host {
    asked: Asked,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let ask = |what: &'static str, asked: &Asked| {
            let asked = asked.clone();
            move |value: bool, _: &mut Window, _: &mut App| asked.borrow_mut().push(format!("{what} {value}"))
        };
        let pick = |what: &'static str, asked: &Asked| {
            let asked = asked.clone();
            move |_: &mut Window, _: &mut App| asked.borrow_mut().push(what.to_owned())
        };
        v_flex()
            .size_full()
            .child(check_row("ticked", "Ticked", true, false, &tokens, ask("ticked", &self.asked)))
            .child(check_row("unticked", "Unticked", false, false, &tokens, ask("unticked", &self.asked)))
            .child(check_row("frozen", "Frozen", false, true, &tokens, ask("frozen", &self.asked)))
            .child(radio_row("first", "First", true, false, &tokens, pick("first", &self.asked)))
            .child(radio_row("second", "Second", false, false, &tokens, pick("second", &self.asked)))
    }
}

struct Opened {
    handle: AnyWindowHandle,
    asked: Asked,
}

fn open(cx: &mut TestAppContext) -> Opened {
    cx.update(crate::window::init);
    let asked: Asked = Rc::default();
    let host = Host { asked: asked.clone() };
    let (handle, _) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(Default::default(), size(px(400.0), px(300.0))))),
                ..Default::default()
            },
            cx,
            |_, cx| cx.new(|_| host),
        )
        .expect("open test window")
    });
    let opened = Opened { handle, asked };
    settle(cx, &opened);
    opened
}

fn settle(cx: &mut TestAppContext, opened: &Opened) {
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

fn press(cx: &mut TestAppContext, opened: &Opened, label: &str) {
    let at = cx.update_window(opened.handle, |_, window, _| window.find_by_label(label).bounds().center()).unwrap();
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(at, MouseButton::Left, cx);
        window.pointer_up(at, MouseButton::Left, cx);
    })
    .unwrap();
    settle(cx, opened);
}

fn checked(cx: &mut TestAppContext, opened: &Opened, label: &str) -> Option<bool> {
    cx.update_window(opened.handle, |_, window, _| window.find_by_label(label).checked()).unwrap()
}

#[gpui_kit::test]
fn a_box_asks_for_the_opposite_of_what_it_shows_and_keeps_showing_what_it_was_given(cx: &mut TestAppContext) {
    let opened = open(cx);
    assert_eq!((checked(cx, &opened, "Ticked"), checked(cx, &opened, "Unticked")), (Some(true), Some(false)));
    press(cx, &opened, "Unticked");
    press(cx, &opened, "Ticked");
    assert_eq!(*opened.asked.borrow(), ["unticked true", "ticked false"]);
    assert_eq!(
        (checked(cx, &opened, "Ticked"), checked(cx, &opened, "Unticked")),
        (Some(true), Some(false)),
        "a box drew its own answer before Rust gave one"
    );
}

#[gpui_kit::test]
fn a_box_that_cannot_be_ticked_asks_nothing(cx: &mut TestAppContext) {
    let opened = open(cx);
    press(cx, &opened, "Frozen");
    assert!(opened.asked.borrow().is_empty(), "{:?}", opened.asked.borrow());
}

#[gpui_kit::test]
fn a_ring_asks_to_be_chosen_and_the_one_chosen_asks_nothing(cx: &mut TestAppContext) {
    let opened = open(cx);
    press(cx, &opened, "First");
    assert!(opened.asked.borrow().is_empty(), "the chosen ring asked again: {:?}", opened.asked.borrow());
    press(cx, &opened, "Second");
    assert_eq!(*opened.asked.borrow(), ["second"]);
    assert_eq!((checked(cx, &opened, "First"), checked(cx, &opened, "Second")), (Some(true), Some(false)), "drew its own choice");
}
