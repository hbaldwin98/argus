//! The Code layer of the key walk (CODE-7, 20, 25, 47).

use gpui_kit::TestAppContext;

use super::*;

fn pressed(cx: &mut TestAppContext, opened: &Opened, key: &str) -> Vec<String> {
    press(cx, opened, key);
    sent(opened)
}

/// CODE-47: the stage's own keys are sent as the actions they are.
#[gpui_kit::test]
fn the_stages_keys_are_sent_as_the_actions_they_are(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    for (key, action) in [("j", "Next"), ("down", "Next"), ("k", "Previous"), ("home", "First"), ("end", "Last"), ("]", "NextFile"), ("[", "PreviousFile"), ("v", "Pick"), ("r", "Refresh"), ("o", "ToggleOutline"), ("e", "OpenEditor"), ("b", "GoBack")] {
        assert_eq!(pressed(cx, &opened, key), [format!("Code {{ gesture: Action {{ action: {action} }} }}")], "{key}");
    }
}

/// CODE-20, 25: Ctrl-P and Ctrl-F open the go-to list and the search, in the session's hands.
#[gpui_kit::test]
fn ctrl_p_and_ctrl_f_ask_for_the_go_to_list_and_the_search(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    assert_eq!(pressed(cx, &opened, "ctrl-p"), ["Code { gesture: Action { action: Filter } }"]);
    assert_eq!(pressed(cx, &opened, "ctrl-f"), ["Code { gesture: Action { action: Search } }"]);
}

/// CODE-7: Escape is the stage's to answer and never leaves the stage by itself.
#[gpui_kit::test]
fn escape_is_sent_to_the_stage_which_never_leaves_it(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    assert_eq!(pressed(cx, &opened, "escape"), ["Code { gesture: Action { action: CloseLayer } }"]);
    assert!(present(cx, &opened, "code-tree"), "the stage stays");
}

/// The window's own keys run from the stage as anywhere: a digit changes stage.
#[gpui_kit::test]
fn the_windows_keys_run_from_the_stage(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    let sent = pressed(cx, &opened, "3");
    assert_eq!(sent, ["ShowCheckouts { open: true }"]);
}

/// A key the stage binds nothing to is held, and typed nowhere.
#[gpui_kit::test]
fn an_unbound_key_is_held(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    assert!(pressed(cx, &opened, "q").is_empty());
}

/// CODE-20: with the go-to list open the field has the keys, so a letter is typed and not a key of the stage.
#[gpui_kit::test]
fn a_letter_in_the_open_field_is_typed_not_a_stage_key(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    says(cx, &opened, jumping());
    press(cx, &opened, "j");
    assert_eq!(sent(&opened), ["Code { gesture: Jump { gesture: Query { text: \"j\" } } }"], "not Next");
}
