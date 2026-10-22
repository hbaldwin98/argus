//! The Checkouts layer of the key walk (CHECKOUTS-12, 28 to 32, 47).

use argus_desktop_core::checkouts::table::Checkouts;
use argus_desktop_core::keys::keymap::{Place, BINDINGS};
use gpui_kit::TestAppContext;

use super::*;
use crate::keys::layer::action_for;
use crate::keys::root::tests::{asked, overlays, press, type_text};
use crate::kit::overlay::kind::Overlay;

fn sent(opened: &Opened, key: &str, cx: &mut TestAppContext) -> Vec<String> {
    press(cx, opened, key);
    drain(opened)
}

/// CHECKOUTS-28: j and k, the arrows, Home and End move the selection, as the session decides.
#[gpui_kit::test]
fn the_moving_keys_are_sent_as_the_actions_they_are(cx: &mut TestAppContext) {
    let opened = on(cx, table("c101"));
    for (key, action) in [("j", "Next"), ("down", "Next"), ("k", "Previous"), ("up", "Previous"), ("home", "First"), ("end", "Last")] {
        assert_eq!(sent(&opened, key, cx), [format!("CheckoutsAction {{ action: {action} }}")], "{key}");
    }
    assert_eq!(table("c101").selected.as_deref(), Some("c101"), "no key predicts where the selection goes");
}

/// CHECKOUTS-29, 30: the table's keys, capital and small alike but for the deletion.
#[gpui_kit::test]
fn the_tables_keys_are_the_actions_the_stage_runs(cx: &mut TestAppContext) {
    let opened = on(cx, table("c101"));
    for (key, action) in [
        ("enter", "Choose"),
        ("b", "Branches"),
        ("f", "Fetch"),
        ("p", "Pull"),
        ("r", "OpenReview"),
        ("h", "OpenHistory"),
        ("s", "StartShell"),
        ("a", "StartAgent"),
        ("e", "Files"),
        ("shift-p", "Pull"),
        ("shift-d", "Delete"),
    ] {
        assert_eq!(sent(&opened, key, cx), [format!("CheckoutsAction {{ action: {action} }}")], "{key}");
    }
    assert!(sent(&opened, "d", cx).is_empty(), "delete is the capital alone");
}

/// CHECKOUTS-30, 39, 49: N asks for a name on a checkout row, and sends at once on a branch row.
#[gpui_kit::test]
fn n_asks_for_a_name_on_a_checkout_row_and_sends_at_once_on_a_branch(cx: &mut TestAppContext) {
    let opened = on(cx, table("c101"));
    assert!(sent(&opened, "n", cx).is_empty(), "nothing is sent until the dialog's Create");
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    assert_eq!(overlays(cx), [Overlay::Worktree]);
    press(cx, &opened, "escape");
    let branches = Checkouts { branches_shown: true, rows: vec![checkout(100, "main", "main"), branch("old")], selected: Some("bold".into()), ..table("bold") };
    hear(cx, &opened, vec![heard(branches)]);
    assert_eq!(sent(&opened, "n", cx), ["NewWorktree { name: None }"]);
    assert!(overlays(cx).is_empty());
}

/// CHECKOUTS-11, 12, 30: `/` takes the filter field and sends nothing; each letter sends the text as
/// it stands; Enter gives the keys back to the table, the filter kept.
#[gpui_kit::test]
fn the_slash_takes_the_filter_and_enter_gives_the_keys_back(cx: &mut TestAppContext) {
    let opened = on(cx, table("c101"));
    assert!(sent(&opened, "/", cx).is_empty(), "the page handles the slash itself");
    type_text(cx, &opened, "ab");
    assert_eq!(drain(&opened), ["FilterCheckouts { filter: \"a\" }", "FilterCheckouts { filter: \"ab\" }"]);
    assert_eq!(sent(&opened, "j", cx), ["FilterCheckouts { filter: \"abj\" }"], "j is typed in the field while it has the keys, and moves nothing");
    press(cx, &opened, "enter");
    assert_eq!(drain(&opened), Vec::<String>::new(), "Enter leaves the field and sends nothing");
    assert_eq!(sent(&opened, "j", cx), ["CheckoutsAction { action: Next }"], "the table has the keys again");
}

/// CHECKOUTS-11: a word half typed is never overwritten by the table the typing brings back.
#[gpui_kit::test]
fn the_field_is_not_overwritten_while_it_is_typed_in(cx: &mut TestAppContext) {
    let opened = on(cx, table("c101"));
    press(cx, &opened, "/");
    type_text(cx, &opened, "abc");
    drain(&opened);
    hear(cx, &opened, vec![heard(Checkouts { filter: "ab".into(), ..table("c101") })]);
    type_text(cx, &opened, "d");
    assert_eq!(drain(&opened), ["FilterCheckouts { filter: \"abcd\" }"], "the caret and the text held across the echo");
}

/// CHECKOUTS-32: Escape on the table does nothing.
#[gpui_kit::test]
fn escape_on_the_table_does_nothing(cx: &mut TestAppContext) {
    let opened = on(cx, table("c101"));
    assert!(sent(&opened, "escape", cx).is_empty());
    assert!(overlays(cx).is_empty());
}

/// SHELL-KEYS-23: a key the stage binds nothing to is held, not typed into the pane under it, and
/// the window's own keys still work.
#[gpui_kit::test]
fn an_unbound_key_is_held_and_the_stage_digits_still_switch(cx: &mut TestAppContext) {
    let opened = on(cx, table("c101"));
    assert!(sent(&opened, "q", cx).is_empty(), "held, so not typed anywhere: {:?}", asked(&opened));
    assert_eq!(sent(&opened, "1", cx), ["ShowFeatures { open: false }"], "the Workspace");
}

/// CHECKOUTS-47: an extension command of the `checkout` scope is heard here, one of the `anywhere`
/// scope too, and one of the `pane` scope is not.
#[gpui_kit::test]
fn the_checkout_scope_of_the_extension_commands_is_heard_on_this_stage(cx: &mut TestAppContext) {
    let opened = on(cx, table("c101"));
    hear(cx, &opened, vec![crate::keys::extension::tests::listed()]);
    assert_eq!(sent(&opened, "u", cx), ["ExtensionCommand { extension: \"docker\", command: \"down\", checkout: None, pane: None }"], "the session resolves the stage's checkout");
    assert_eq!(sent(&opened, "y", cx), ["ExtensionCommand { extension: \"docker\", command: \"up\", checkout: None, pane: None }"]);
    assert!(sent(&opened, "w", cx).is_empty(), "a pane's command needs a pane");
}

/// The order the lookups are made in changes nothing: no key of the table is also the window's.
#[test]
fn no_key_of_the_table_is_also_a_key_of_anywhere() {
    use argus_desktop_core::keys::page_key::PageKey;
    for binding in BINDINGS.iter().filter(|binding| binding.place == Place::Checkouts) {
        for chord in binding.chords.iter().filter(|chord| !chord.leader) {
            let page = PageKey { key: chord.key.to_owned(), shift: chord.shift, ctrl: chord.ctrl, alt: chord.alt, meta: chord.meta, ..PageKey::default() };
            assert_eq!(action_for(&[Place::Anywhere], &page), None, "{:?}", binding.action);
        }
    }
}
