//! The dialog's keys (SESSIONS-5, -6, -8, -11, -20): the keymap's `Sessions` place sent as
//! `SessionsAction`, the search field's own Enter and Escape, and every other key held.

use argus_desktop_core::connection::page::Event;
use gpui_kit::TestAppContext;

use super::{hear_page, said, sessions};
use crate::kit::overlay::kind::Overlay;
use crate::keys::root::tests::{asked, drain, hear, open, overlays, press, type_text};

#[gpui_kit::test]
fn the_dialog_is_up_exactly_while_the_session_holds_it(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear_page(cx, &opened, sessions());
    assert!(overlays(cx).contains(&Overlay::Sessions));
    hear(cx, &opened, vec![Event::Sessions(Box::default())]);
    assert!(!overlays(cx).contains(&Overlay::Sessions), "open false is the session's word that it is shut");
}

#[gpui_kit::test]
fn the_keymaps_sessions_keys_are_sent_as_actions_and_nothing_is_predicted(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear_page(cx, &opened, sessions());
    drain(&opened);
    for (key, action) in [
        ("j", "Next"),
        ("down", "Next"),
        ("k", "Previous"),
        ("up", "Previous"),
        ("home", "First"),
        ("end", "Last"),
        ("enter", "Choose"),
        ("r", "Resume"),
        ("o", "Transcript"),
        ("h", "FilterHarness"),
        ("c", "FilterCheckout"),
        ("escape", "CloseLayer"),
    ] {
        press(cx, &opened, key);
        let last = said(&opened).pop().unwrap_or_default();
        assert_eq!(last, format!("SessionsAction {{ action: {action}, row: None }}"), "{key}");
        drain(&opened);
    }
    assert!(overlays(cx).contains(&Overlay::Sessions), "Escape asks; the dialog goes when the session says so");
}

#[gpui_kit::test]
fn a_key_the_dialog_does_not_bind_is_held_and_reaches_nothing_under_it(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear_page(cx, &opened, sessions());
    drain(&opened);
    for key in ["1", "x", "ctrl-n", "`", "tab"] {
        press(cx, &opened, key);
    }
    assert!(asked(&opened).is_empty(), "no stage switch, no key for a pane: {:?}", asked(&opened));
}

#[gpui_kit::test]
fn slash_gives_the_search_the_keys_each_edit_is_sent_and_enter_gives_the_list_the_keys_back(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear_page(cx, &opened, sessions());
    drain(&opened);
    press(cx, &opened, "/");
    assert!(asked(&opened).is_empty(), "the slash is the window's own: nothing is sent");
    type_text(cx, &opened, "ab");
    assert_eq!(said(&opened), ["SearchSessions { search: \"a\" }", "SearchSessions { search: \"ab\" }"], "each edit, no debounce");
    drain(&opened);
    press(cx, &opened, "escape");
    assert!(asked(&opened).is_empty(), "Escape in the field only blurs it");
    assert!(overlays(cx).contains(&Overlay::Sessions));
    press(cx, &opened, "j");
    assert_eq!(said(&opened), ["SessionsAction { action: Next, row: None }"], "the list has the keys back");
}

#[gpui_kit::test]
fn nothing_is_sent_while_the_daemon_is_gone(cx: &mut TestAppContext) {
    use argus_desktop_core::connection::page::{Connection, Status};
    let opened = open(cx);
    hear_page(cx, &opened, sessions());
    hear(cx, &opened, vec![Event::Status(Status { connection: Connection::Stopped, detail: None, alarm: None, chime: true, host: None })]);
    drain(&opened);
    press(cx, &opened, "j");
    press(cx, &opened, "enter");
    assert!(said(&opened).is_empty(), "{:?}", asked(&opened));
}
