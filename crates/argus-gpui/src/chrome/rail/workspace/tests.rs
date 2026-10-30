//! The new-workspace dialog in a test window: it asks with what was typed, shows the daemon's
//! refusal under the field, and goes only when the session lets go of the ask.

use argus_desktop_core::connection::page::{Connection, Event, NewWorkspace, Rail};
use gpui_kit::prelude::*;
use gpui_kit::test::{TestQueryExt as _, TestWindowExt as _};
use gpui_kit::TestAppContext;

use super::open as open_dialog;
use crate::chrome::rail::tests::{connected_and_busy, hear, open, status, taken, Opened};
use crate::connection::caller::Caller;
use crate::kit::overlay::host::Overlays;
use crate::kit::overlay::kind::Overlay;

fn overlays(cx: &mut TestAppContext) -> Vec<Overlay> {
    [Overlay::NewWorkspace].into_iter().filter(|kind| cx.update(|cx| Overlays::get(cx).is_open(*kind))).collect()
}

fn has_label(cx: &mut TestAppContext, opened: &Opened, label: &str) -> bool {
    cx.update_window(opened.handle, |_, window, _| window.try_find_by_label(label).is_some()).unwrap()
}

fn press_key(cx: &mut TestAppContext, opened: &Opened, key: &str) {
    cx.update_window(opened.handle, |_, window, cx| window.press(key, cx)).unwrap();
    cx.run_until_parked();
}

fn type_text(cx: &mut TestAppContext, opened: &Opened, text: &str) {
    for key in text.chars() {
        press_key(cx, opened, &key.to_string());
    }
}

/// The dialog up over a connected rail.
fn dialog(cx: &mut TestAppContext) -> Opened {
    let opened = open(cx, connected_and_busy());
    let calls = opened.calls.clone();
    let caller = Caller::new(move |call| calls.borrow_mut().push(call));
    let heard = opened.heard.clone();
    cx.update_window(opened.handle, |_, window, cx| open_dialog(caller, heard, window, cx)).unwrap();
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(overlays(cx), [Overlay::NewWorkspace]);
    opened
}

fn say(new_workspace: Option<NewWorkspace>) -> Vec<Event> {
    vec![status(Connection::Connected), Event::Rail(Box::new(Rail { new_workspace, ..argus_desktop_core::connection::page::Rail::default() }))]
}

#[gpui_kit::test]
fn enter_in_the_field_asks_with_what_was_typed_and_the_dialog_waits_for_the_answer(cx: &mut TestAppContext) {
    let opened = dialog(cx);
    assert!(has_label(cx, &opened, "Enter creates and opens it \u{b7} Esc cancels"), "the hint line");
    type_text(cx, &opened, "side");
    press_key(cx, &opened, "enter");
    assert_eq!(taken(&opened), ["CreateWorkspace { name: \"side\" }"]);
    assert_eq!(overlays(cx), [Overlay::NewWorkspace], "nothing is made until the daemon says so");
}

#[gpui_kit::test]
fn a_refusal_is_said_under_the_field_which_keeps_the_dialog_and_the_name(cx: &mut TestAppContext) {
    let opened = dialog(cx);
    type_text(cx, &opened, "default");
    press_key(cx, &opened, "enter");
    hear(cx, &opened, say(Some(NewWorkspace { creating: true, refusal: None })));
    hear(cx, &opened, say(Some(NewWorkspace { creating: false, refusal: Some("workspace already exists: default".into()) })));
    assert!(has_label(cx, &opened, "workspace already exists: default"));
    assert_eq!(overlays(cx), [Overlay::NewWorkspace]);
    press_key(cx, &opened, "enter");
    assert_eq!(taken(&opened), ["CreateWorkspace { name: \"default\" }", "CreateWorkspace { name: \"default\" }"], "the name is still there to send again");
}

#[gpui_kit::test]
fn the_dialog_goes_when_the_session_lets_go_of_the_ask_it_held(cx: &mut TestAppContext) {
    let opened = dialog(cx);
    type_text(cx, &opened, "side");
    press_key(cx, &opened, "enter");
    hear(cx, &opened, say(None));
    assert_eq!(overlays(cx), [Overlay::NewWorkspace], "a rail from before the ask is not the answer");
    hear(cx, &opened, say(Some(NewWorkspace { creating: true, refusal: None })));
    hear(cx, &opened, say(None));
    assert!(overlays(cx).is_empty());
}

#[gpui_kit::test]
fn escape_tells_the_session_to_forget_the_ask_and_asks_nothing(cx: &mut TestAppContext) {
    let opened = dialog(cx);
    type_text(cx, &opened, "side");
    press_key(cx, &opened, "escape");
    assert_eq!(taken(&opened), ["DismissNewWorkspace"]);
    assert!(overlays(cx).is_empty());
}
