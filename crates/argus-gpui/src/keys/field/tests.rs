use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::{AppContext as _, TestAppContext};

use super::*;
use crate::keys::root::tests::{asked, blur, open, overlays, press, Opened};
use crate::kit::overlay::kind::Overlay;

/// Gives the keys to a field that answers `place`, noting each action it is told and answering
/// only those in `ours`.
fn focus_field(
    cx: &mut TestAppContext,
    opened: &Opened,
    place: Place,
    ours: &'static [Action],
) -> (Rc<RefCell<Vec<Action>>>, Registered) {
    let told: Rc<RefCell<Vec<Action>>> = Rc::default();
    let noting = told.clone();
    let registered = cx
        .update_window(opened.handle, |_, window, cx| {
            let handle = cx.focus_handle();
            let registered = Fields::of(cx).register_place(window, handle.clone(), place, move |action, _, _| {
                noting.borrow_mut().push(action);
                ours.contains(&action)
            });
            window.focus(&handle, cx);
            registered
        })
        .unwrap();
    cx.run_until_parked();
    (told, registered)
}

/// A field that answers a place has that place's keys run before its own, and the rest typed.
#[gpui_kit::test]
fn a_field_answering_a_place_runs_its_keys_and_holds_every_other(cx: &mut TestAppContext) {
    let opened = open(cx);
    blur(cx, &opened);
    let (told, _registered) = focus_field(cx, &opened, Place::Jump, &[Action::Next]);
    press(cx, &opened, "ctrl-n");
    press(cx, &opened, "enter");
    press(cx, &opened, "3");
    assert_eq!(*told.borrow(), [Action::Next, Action::Choose], "the place's keys, in order; the digit is no key of it");
    assert!(asked(&opened).is_empty(), "neither the Compose that ctrl-n is on the stage nor the digit reached the stage: {:?}", asked(&opened));
}

/// A field leaves with its view: the keys are the stage's again.
#[gpui_kit::test]
fn a_field_that_is_dropped_no_longer_hears_keys(cx: &mut TestAppContext) {
    let opened = open(cx);
    blur(cx, &opened);
    let (told, registered) = focus_field(cx, &opened, Place::Jump, &[Action::Next]);
    let before = cx.update(|cx| Fields::of(cx).places().len());
    drop(registered);
    assert_eq!(cx.update(|cx| Fields::of(cx).places().len()), before - 1, "its place in the list went with it");
    press(cx, &opened, "ctrl-n");
    assert!(told.borrow().is_empty());
    assert_eq!(asked(&opened), ["ShowComposer { open: true }"], "ctrl-n is the stage's Compose again");
}

/// SHELL-KEYS-14: the palette's stand-in holds the keys while it is open, so a stage digit pressed
/// to search is not the stage switch under it.
#[gpui_kit::test]
fn a_digit_pressed_while_the_palette_is_open_does_not_switch_the_stage(cx: &mut TestAppContext) {
    let opened = open(cx);
    blur(cx, &opened);
    press(cx, &opened, "ctrl-k");
    assert_eq!(overlays(cx), [Overlay::Palette]);
    press(cx, &opened, "3");
    assert!(asked(&opened).is_empty(), "the digit reached the stage: {:?}", asked(&opened));
    assert_eq!(overlays(cx), [Overlay::Palette], "the palette is still the only overlay");
}
