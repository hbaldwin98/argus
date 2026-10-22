//! The window's keys in front of a pane that has the keys (SHELL-INPUT-2, SHELL-KEYS-3).

use super::*;
use crate::keys::root::tests::zoom;
use gpui_kit::Modifiers;

#[gpui_kit::test]
fn the_zoom_keys_are_the_windows_and_never_reach_the_pane_that_has_the_keys(cx: &mut TestAppContext) {
    let opened = open(cx);
    for key in ["=", "-", "0"] {
        type_keys(cx, &opened, &zoom(key));
    }
    let calls = taken(&opened);
    assert_eq!(calls.len(), 3, "{calls:?}");
    assert!(
        calls.iter().all(|call| matches!(call, Call::Command { .. })),
        "a zoom key went to the child as a key: {calls:?}"
    );
}

#[gpui_kit::test]
fn on_a_mac_the_command_keys_chords_are_the_windows_unless_the_pane_binds_them(cx: &mut TestAppContext) {
    let opened = open(cx);
    cx.update(|cx| opened.terminal.read(cx).command_is_window.set(true));
    // The command key is `cmd` to GPUI, whichever platform runs the test.
    type_keys(cx, &opened, "cmd-c cmd-q");
    assert!(taken(&opened).is_empty(), "the window's chords were sent to the child: {:?}", taken(&opened));
    // Plain typing is still the pane's.
    type_keys(cx, &opened, "a");
    assert_eq!(taken(&opened).len(), 1);
}

#[gpui_kit::test]
fn off_a_mac_the_command_key_is_just_a_modifier_the_child_is_told_of(cx: &mut TestAppContext) {
    let opened = open(cx);
    cx.update(|cx| opened.terminal.read(cx).command_is_window.set(false));
    type_keys(cx, &opened, "cmd-c");
    let calls = taken(&opened);
    assert!(matches!(calls.as_slice(), [Call::Key { key }] if key.meta && key.key == "c"), "{calls:?}");
}

#[gpui_kit::test]
fn a_key_the_adapter_cannot_name_reaches_the_input_handler_as_text(cx: &mut TestAppContext) {
    let opened = open(cx);
    // A platform that types a key as several characters (a compose sequence, a dead key's result).
    let keystroke = Keystroke { modifiers: Modifiers::none(), key: "a".into(), key_char: Some("ae".into()) };
    cx.update_window(opened.handle, |_, window, cx| window.dispatch_keystroke(keystroke, cx)).unwrap();
    cx.run_until_parked();
    let calls = taken(&opened);
    assert!(matches!(calls.as_slice(), [Call::Text { text }] if text == "ae"), "{calls:?}");
}
