//! The Escape ladder, with hooks standing in for layers whose surfaces are not in the window yet.

use std::cell::RefCell;
use std::rc::Rc;

use argus_desktop_core::keys::keymap::Layer;
use gpui_kit::TestAppContext;

use super::*;
use crate::keys::layer::{Handler, Layers};
use crate::keys::root::tests::{asked, blur, open, press};

/// A layer that is never up, and answers Escape as told, noting that it was asked.
struct Hook {
    name: &'static str,
    answers: bool,
    asked: Rc<RefCell<Vec<&'static str>>>,
}

impl Handler for Hook {
    fn open(&self, _: &Window, _: &App) -> bool {
        false
    }

    fn escape(&self, _: &mut Window, _: &mut App) -> bool {
        self.asked.borrow_mut().push(self.name);
        self.answers
    }
}

fn hook(cx: &mut TestAppContext, layer: Layer, name: &'static str, answers: bool, asked: &Rc<RefCell<Vec<&'static str>>>) {
    let handler = Rc::new(Hook { name, answers, asked: asked.clone() });
    cx.update(|cx| Layers::of(cx).register(layer, handler));
}

/// SHELL-KEYS-19: Escape asks each layer's hook in the table's order, up or not, and stops at the first
/// that puts something away.
#[gpui_kit::test]
fn escape_asks_the_hooks_in_the_tables_order_and_stops_at_the_first_that_answers(cx: &mut TestAppContext) {
    let opened = open(cx);
    blur(cx, &opened);
    let asked_of: Rc<RefCell<Vec<&'static str>>> = Rc::default();
    hook(cx, Layer::Sessions, "sessions", false, &asked_of);
    hook(cx, Layer::Capture, "capture", true, &asked_of);
    hook(cx, Layer::Outline, "outline", true, &asked_of);
    press(cx, &opened, "escape");
    assert_eq!(*asked_of.borrow(), ["sessions", "capture"], "the outline comes after the first answer");
    assert!(asked(&opened).is_empty(), "Escape put something away; it was not sent anywhere");
}

/// The hook of a layer is asked though the layer is not up: that is how a question outlives its menu.
#[gpui_kit::test]
fn a_hook_is_asked_whether_or_not_its_layer_is_up(cx: &mut TestAppContext) {
    let opened = open(cx);
    blur(cx, &opened);
    let asked_of: Rc<RefCell<Vec<&'static str>>> = Rc::default();
    hook(cx, Layer::Code, "code", false, &asked_of);
    press(cx, &opened, "escape");
    assert_eq!(*asked_of.borrow(), ["code"]);
}
