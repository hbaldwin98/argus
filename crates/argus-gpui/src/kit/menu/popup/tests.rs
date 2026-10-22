use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::*;
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::test::{TestEventExt as _, TestQueryExt as _, TestWindowExt as _};
use gpui_kit::ElementId;
use gpui_kit::{
    size, AnyWindowHandle, Bounds, MouseButton, Render, TestAppContext, WindowBounds, WindowOptions,
};

use crate::theme::Tokens;

type Done = Rc<RefCell<Vec<&'static str>>>;

/// A target that opens the menu on a right click.
struct Host {
    items: Vec<MenuItem<&'static str>>,
    held: Rc<Cell<bool>>,
    done: Done,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut gpui_kit::Context<Self>) -> impl IntoElement {
        let (items, held, done) = (self.items.clone(), self.held.clone(), self.done.clone());
        let tokens = *Tokens::global(cx);
        div().id("target").test_support().size_full().context_menu(move |menu, _, cx| {
            let (asking, done) = (held.clone(), done.clone());
            fill(menu, &items, tokens, move |_| asking.get(), move |does, _, _| done.borrow_mut().push(*does), cx)
        })
    }
}

struct Opened {
    handle: AnyWindowHandle,
    held: Rc<Cell<bool>>,
    done: Done,
}

fn open(cx: &mut TestAppContext, items: Vec<MenuItem<&'static str>>) -> Opened {
    cx.update(crate::window::init);
    let (held, done): (Rc<Cell<bool>>, Done) = Default::default();
    let host = Host { items, held: held.clone(), done: done.clone() };
    let (handle, _) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(Default::default(), size(px(600.0), px(400.0))))),
                ..Default::default()
            },
            cx,
            |_, cx| cx.new(|_| host),
        )
        .expect("open test window")
    });
    let opened = Opened { handle, held, done };
    settle(cx, &opened);
    opened
}

fn settle(cx: &mut TestAppContext, opened: &Opened) {
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

fn menu_opened(cx: &mut TestAppContext, opened: &Opened) {
    cx.update_window(opened.handle, |_, window, cx| window.right_click("target".to_owned(), cx)).unwrap();
    settle(cx, opened);
}

fn press(cx: &mut TestAppContext, opened: &Opened, key: &str) {
    cx.update_window(opened.handle, |_, window, cx| window.press(key, cx)).unwrap();
    settle(cx, opened);
}

fn pick(cx: &mut TestAppContext, opened: &Opened, label: &str) {
    let at = cx.update_window(opened.handle, |_, window, _| window.find_by_label(label).bounds().center()).unwrap();
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(at, MouseButton::Left, cx);
        window.pointer_up(at, MouseButton::Left, cx);
    })
    .unwrap();
    settle(cx, opened);
}

fn items() -> Vec<MenuItem<&'static str>> {
    vec![
        MenuItem::choice("Open", "open"),
        MenuItem::choice("Close", "close").danger(),
        MenuItem::choice("Busy", "busy").refused("here"),
        MenuItem::choice("Last", "last").keyed(vec![vec!["Ctrl Space".into(), "4".into()]]),
    ]
}

// SHELL-MENU-1: a choice with a reason cannot be chosen; the others run what they do.
#[gpui_kit::test]
fn a_choice_runs_what_it_does_and_one_with_a_reason_runs_nothing(cx: &mut TestAppContext) {
    let opened = open(cx, items());
    menu_opened(cx, &opened);
    pick(cx, &opened, "Busy");
    assert!(opened.done.borrow().is_empty(), "a refused choice ran: {:?}", opened.done.borrow());
    menu_opened(cx, &opened);
    pick(cx, &opened, "Close");
    assert_eq!(*opened.done.borrow(), ["close"]);
}

// SHELL-MENU-3: the menu is gone when what a choice does runs, not still open under it.
#[gpui_kit::test]
fn the_menu_has_closed_by_the_time_a_choice_runs(cx: &mut TestAppContext) {
    let opened = open(cx, items());
    menu_opened(cx, &opened);
    pick(cx, &opened, "Open");
    assert_eq!(*opened.done.borrow(), ["open"]);
    let still = cx.update_window(opened.handle, |_, window, _| window.try_find("popup-menu".to_owned()).is_some()).unwrap();
    assert!(!still, "the menu stayed open after a choice");
}

// The daemon being gone disables every choice, and a menu opened before it went sends nothing.
#[gpui_kit::test]
fn a_held_menu_runs_nothing_even_when_it_was_opened_before_the_hold(cx: &mut TestAppContext) {
    let opened = open(cx, items());
    opened.held.set(true);
    menu_opened(cx, &opened);
    pick(cx, &opened, "Open");
    assert!(opened.done.borrow().is_empty(), "built held: {:?}", opened.done.borrow());
    press(cx, &opened, "escape");

    opened.held.set(false);
    menu_opened(cx, &opened);
    opened.held.set(true);
    pick(cx, &opened, "Open");
    assert!(opened.done.borrow().is_empty(), "held after it opened: {:?}", opened.done.borrow());
}

// SHELL-MENU-4: j and k move the highlight as Down and Up do, past a choice with a reason, and wrap.
#[gpui_kit::test]
fn j_and_k_step_through_the_choices_that_can_be_chosen_and_wrap(cx: &mut TestAppContext) {
    let opened = open(cx, items());
    // Opens afresh, presses `keys`, then Enter: what the highlight was on runs.
    let after = |cx: &mut TestAppContext, keys: &[&str]| {
        opened.done.borrow_mut().clear();
        menu_opened(cx, &opened);
        for key in keys {
            press(cx, &opened, key);
        }
        press(cx, &opened, "enter");
        let said = opened.done.borrow().last().copied();
        said
    };
    assert_eq!(after(cx, &["j"]), Some("open"));
    assert_eq!(after(cx, &["j", "j"]), Some("close"));
    assert_eq!(after(cx, &["j", "j", "j"]), Some("last"), "past the choice that has a reason");
    assert_eq!(after(cx, &["j", "j", "j", "j"]), Some("open"), "wrapped past the end");
    assert_eq!(after(cx, &["k"]), Some("last"), "up from nothing is the end");
    assert_eq!(after(cx, &["j", "k"]), Some("last"), "up from the first wraps");
    assert_eq!(after(cx, &["j", "j", "k"]), Some("open"));
}

// SHELL-MENU-4 (ts:kit/menu.ts): a heading is never where the highlight rests, on the first step
// or on the wrap, as in the dock and console tab menus.
#[gpui_kit::test]
fn a_leading_heading_is_stepped_over_on_the_first_press_and_on_the_wrap(cx: &mut TestAppContext) {
    let under_a_heading = vec![
        MenuItem::heading("Tab"),
        MenuItem::choice("Open", "open"),
        MenuItem::choice("Busy", "busy").refused("here"),
        MenuItem::choice("Close", "close"),
    ];
    let opened = open(cx, under_a_heading);
    let after = |cx: &mut TestAppContext, keys: &[&str]| {
        opened.done.borrow_mut().clear();
        menu_opened(cx, &opened);
        for key in keys {
            press(cx, &opened, key);
        }
        press(cx, &opened, "enter");
        let said = opened.done.borrow().last().copied();
        said
    };
    assert_eq!(after(cx, &["j"]), Some("open"), "the first press");
    assert_eq!(after(cx, &["down"]), Some("open"), "the first press, by Down");
    assert_eq!(after(cx, &["j", "j"]), Some("close"));
    assert_eq!(after(cx, &["j", "j", "j"]), Some("open"), "the wrap");
    assert_eq!(after(cx, &["j", "k"]), Some("close"), "up from the first wraps");
}

// A list of choices that may be ticked (the project list): the ticked one says so.
#[gpui_kit::test]
fn a_ticked_choice_draws_its_check_and_an_unticked_one_does_not(cx: &mut TestAppContext) {
    let opened = open(cx, vec![MenuItem::choice("orion", "a").marked(true), MenuItem::choice("work", "b").marked(false)]);
    menu_opened(cx, &opened);
    let ticked = cx.update_window(opened.handle, |_, window, _| window.elements().into_iter().filter(|element| element.label() == Some("checked")).count()).unwrap();
    assert_eq!(ticked, 1, "one check, on the ticked choice");
    let columns = cx
        .update_window(opened.handle, |_, window, _| {
            window.elements().into_iter().filter(|element| element.path().iter().any(|id| matches!(id, ElementId::NamedInteger(name, _) if name.as_ref() == "menu-check"))).count()
        })
        .unwrap();
    assert_eq!(columns, 2, "both keep the column");
}
