//! The walk itself: who hears a key first, what holds it, and what a layer that runs it asks.

use std::cell::RefCell;
use std::rc::Rc;

use argus_desktop_core::connection::page::Event;
use argus_desktop_core::keys::keymap::Action;
use gpui_kit::TestAppContext;

use super::*;
use crate::kit::overlay::host::{Dismiss, Spec};

fn open_overlay(cx: &mut TestAppContext, opened: &Opened, spec: Spec) {
    let host = cx.update(|cx| Overlays::get(cx));
    cx.update_window(opened.handle, |_, window, cx| {
        host.open(spec, |dialog, _, _, _| dialog.title("x"), window, cx);
    })
    .unwrap();
    cx.run_until_parked();
}

/// SHELL-KEYS-5: inside a pane every key is the program's, the session's to say what it is.
#[gpui_kit::test]
fn a_pane_with_the_keys_is_sent_every_key_even_one_the_stage_binds(cx: &mut TestAppContext) {
    let opened = open(cx);
    press(cx, &opened, "a");
    press(cx, &opened, "3");
    press(cx, &opened, "escape");
    let keys = asked(&opened);
    assert_eq!(keys.len(), 3, "{keys:?}");
    assert!(keys.iter().all(|call| call.starts_with("Key {")), "the digit and Escape stayed the pane's: {keys:?}");
}

/// SHELL-KEYS-3: the zoom keys are the window's, wherever the keys are.
#[gpui_kit::test]
fn the_zoom_keys_are_heard_before_a_pane_that_has_the_keys(cx: &mut TestAppContext) {
    let opened = open(cx);
    for (key, call) in [("=", "ZoomIn"), ("-", "ZoomOut"), ("0", "ZoomReset")] {
        press(cx, &opened, &zoom(key));
        assert_eq!(asked(&opened).pop().as_deref(), Some(format!("Command {{ action: {call} }}").as_str()), "{key}");
    }
    assert_eq!(asked(&opened).len(), 3, "none of them reached the pane as a key");
}

/// SHELL-KEYS-17: with nothing holding the keys the stage runs the Anywhere place.
#[gpui_kit::test]
fn a_digit_with_no_field_switches_the_stage_by_the_call_the_page_sends(cx: &mut TestAppContext) {
    let opened = open(cx);
    blur(cx, &opened);
    for (key, call) in [
        ("1", "ShowFeatures { open: false }"),
        ("2", "ShowFeatures { open: true }"),
        ("3", "ShowCheckouts { open: true }"),
        ("5", "ShowCode { open: true }"),
        // SHELL-KEYS-24: the Browsers stage is the session's command.
        ("6", "Command { action: Browsers }"),
        ("`", "Console { gesture: Toggle }"),
        ("ctrl-n", "ShowComposer { open: true }"),
    ] {
        press(cx, &opened, key);
        assert_eq!(asked(&opened).pop().as_deref(), Some(call), "{key}");
    }
}

/// A key no layer binds is held: it is neither sent anywhere nor typed.
#[gpui_kit::test]
fn a_key_the_stage_does_not_bind_does_nothing(cx: &mut TestAppContext) {
    let opened = open(cx);
    blur(cx, &opened);
    press(cx, &opened, "q");
    press(cx, &opened, "shift");
    assert!(asked(&opened).is_empty(), "{:?}", asked(&opened));
}

/// SHELL-KEYS-13: the palette, the sheet and the settings open from their keys. Each has the keys
/// while it is up, so its own key is how it goes (the page's `Escape`).
#[gpui_kit::test]
fn the_palette_the_sheet_and_the_settings_open_by_key_and_the_session_is_told_only_of_the_palette(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![super::settings_event()]);
    drain(&opened);
    for (key, kind) in [("ctrl-k", Overlay::Palette), ("ctrl-,", Overlay::Settings)] {
        blur(cx, &opened);
        press(cx, &opened, key);
        assert_eq!(overlays(cx), [kind], "{key}");
        press(cx, &opened, "escape");
        assert!(overlays(cx).is_empty(), "Escape closed {kind:?}");
    }
    blur(cx, &opened);
    question_mark(cx, &opened);
    assert_eq!(overlays(cx), [Overlay::Sheet]);
    assert_eq!(asked(&opened), ["Palette { open: true }", "Palette { open: false }"], "the window opened them itself; the session is told only when the palette opens and closes, so it sends what it can go to");
}

/// SHELL-KEYS-14: a dialog holds the keys that would open another: the palette key does nothing in the settings.
#[gpui_kit::test]
fn a_dialog_that_is_up_does_not_hand_its_keys_to_the_window(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![super::settings_event()]);
    drain(&opened);
    blur(cx, &opened);
    press(cx, &opened, "ctrl-,");
    press(cx, &opened, "ctrl-k");
    assert_eq!(overlays(cx), [Overlay::Settings]);
}

/// SHELL-KEYS-13: `?` while the sheet is up closes it, and so does Escape.
#[gpui_kit::test]
fn the_sheet_closes_on_its_own_key_and_on_escape(cx: &mut TestAppContext) {
    let opened = open(cx);
    blur(cx, &opened);
    question_mark(cx, &opened);
    assert_eq!(overlays(cx), [Overlay::Sheet]);
    blur(cx, &opened);
    question_mark(cx, &opened);
    assert!(overlays(cx).is_empty(), "the toggle");
    question_mark(cx, &opened);
    // The keys are not in the dialog, so the kit's own Escape would not reach it: the layer closes it.
    blur(cx, &opened);
    press(cx, &opened, "escape");
    assert!(overlays(cx).is_empty(), "Escape");
}

/// SHELL-KEYS-14: a dialog that answers its own keys holds every key, the stage's among them.
#[gpui_kit::test]
fn an_open_settings_holds_the_keys_the_stage_would_run(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![super::settings_event()]);
    drain(&opened);
    blur(cx, &opened);
    press(cx, &opened, "ctrl-,");
    press(cx, &opened, "3");
    press(cx, &opened, "`");
    assert!(asked(&opened).is_empty(), "{:?}", asked(&opened));
}

/// SHELL-KEYS-18, 19: a question over the stage holds the stage's keys, and Escape answers it no.
#[gpui_kit::test]
fn a_question_holds_the_stage_keys_and_escape_answers_it_no(cx: &mut TestAppContext) {
    let opened = open(cx);
    blur(cx, &opened);
    let answered: Rc<RefCell<u32>> = Rc::default();
    let said = answered.clone();
    let no = Dismiss::Answers(Rc::new(move |_, _| *said.borrow_mut() += 1));
    open_overlay(cx, &opened, Spec::new(Overlay::Question).dismissed(no));
    press(cx, &opened, "3");
    assert!(asked(&opened).is_empty(), "the stage's key ran under a question");
    press(cx, &opened, "escape");
    assert_eq!(*answered.borrow(), 1);
    assert!(overlays(cx).is_empty());
}

/// SHELL-KEYS-9: a key in a pane that asked for the palette, the sheet or the settings reaches the
/// window as `Run`, and the window opens it.
#[gpui_kit::test]
fn a_run_the_session_sends_opens_what_it_names_and_closes_the_others(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![super::settings_event()]);
    drain(&opened);
    for (action, kind) in [(Action::Shortcuts, Overlay::Sheet), (Action::Palette, Overlay::Palette), (Action::Settings, Overlay::Settings)] {
        opened.events.send(vec![Event::Run { action }]).unwrap();
        cx.run_until_parked();
        assert_eq!(overlays(cx), [kind], "{action:?}");
    }
}

/// SHELL-KEYS-19: the menu layer's hook answers a question no, and is asked by the ladder, which
/// does not wait for the dialog to have the keys (an extension's question may not).
#[gpui_kit::test]
fn the_ladders_menu_hook_answers_a_question_no_though_the_dialog_has_not_the_keys(cx: &mut TestAppContext) {
    let opened = open(cx);
    blur(cx, &opened);
    let answered: Rc<RefCell<u32>> = Rc::default();
    let said = answered.clone();
    let no = Dismiss::Answers(Rc::new(move |_, _| *said.borrow_mut() += 1));
    open_overlay(cx, &opened, Spec::new(Overlay::Question).dismissed(no).keeping_focus());
    let put_away = cx.update_window(opened.handle, |_, window, cx| crate::keys::run::escape(window, cx)).unwrap();
    cx.run_until_parked();
    assert!(put_away, "the ladder found the question");
    assert_eq!(*answered.borrow(), 1, "answered no, not only put away");
    assert!(overlays(cx).is_empty());
    let again = cx.update_window(opened.handle, |_, window, cx| crate::keys::run::escape(window, cx)).unwrap();
    assert!(!again, "nothing left to put away");
}

/// SHELL-KEYS-13: a `Run` for the sheet while the sheet is up closes it, as `?` does, and does not
/// open it a second time.
#[gpui_kit::test]
fn a_run_for_the_sheet_while_it_is_up_closes_it(cx: &mut TestAppContext) {
    let opened = open(cx);
    opened.events.send(vec![Event::Run { action: Action::Shortcuts }]).unwrap();
    cx.run_until_parked();
    assert_eq!(overlays(cx), [Overlay::Sheet]);
    opened.events.send(vec![Event::Run { action: Action::Shortcuts }]).unwrap();
    cx.run_until_parked();
    assert!(overlays(cx).is_empty());
}

/// SHELL-KEYS-18: a question over the stage holds the window's actions, one the rail's layer runs
/// (it comes before the menu in the table) included.
#[gpui_kit::test]
fn an_action_run_while_a_question_is_up_is_refused(cx: &mut TestAppContext) {
    let opened = open(cx);
    blur(cx, &opened);
    open_overlay(cx, &opened, Spec::new(Overlay::Question));
    let ran = cx.update_window(opened.handle, |_, window, cx| crate::keys::run::run(Action::Checkouts, window, cx)).unwrap();
    assert!(!ran);
    assert!(asked(&opened).is_empty(), "{:?}", asked(&opened));
}
