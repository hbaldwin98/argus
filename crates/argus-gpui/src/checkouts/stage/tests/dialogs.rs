//! The question before a removal and the name of a new worktree (CHECKOUTS-39 to 43, 49, 50).

use std::time::Duration;

use argus_desktop_core::checkouts::stage::Confirm;
use argus_desktop_core::checkouts::table::Checkouts;
use gpui_kit::TestAppContext;

use super::*;
use crate::keys::root::tests::{asked, click_label, overlays, press, type_text};
use crate::kit::overlay::kind::Overlay;

fn asking(confirm: Option<Confirm>) -> Checkouts {
    Checkouts { confirm, ..table("c101") }
}

fn force() -> Confirm {
    Confirm::Force { branch: "spike".into(), commits: vec![commit("a1b2c3d", "Try a thing"), commit("e4f5a6b", "Try another")] }
}

/// The dialog slides in over its first frames, on the machine's clock and not the test's.
fn settle(cx: &mut TestAppContext, opened: &Opened) {
    cx.executor().advance_clock(Duration::from_secs(1));
    hear(cx, opened, Vec::new());
}

/// CHECKOUTS-41: a removal question names the worktree and what a yes gives up, and `Keep it` has
/// the keys.
#[gpui_kit::test]
fn a_removal_question_names_the_worktree_and_says_what_goes(cx: &mut TestAppContext) {
    let opened = on(cx, asking(None));
    assert!(overlays(cx).is_empty());
    hear(cx, &opened, vec![heard(asking(Some(removing("store-faults"))))]);
    settle(cx, &opened);
    assert_eq!(overlays(cx), [Overlay::Removal]);
    assert!(drawn(cx, &opened, "Remove store-faults?"));
    assert!(drawn(cx, &opened, "Its panes end; its directory and branch are deleted."));
    assert!(present(cx, &opened, "confirm-keep") && present(cx, &opened, "confirm-yes"));
}

/// CHECKOUTS-41: a refused branch deletion asks to force it, naming the commits a yes gives up.
#[gpui_kit::test]
fn a_forced_deletion_names_the_branch_and_each_commit_it_gives_up(cx: &mut TestAppContext) {
    let opened = on(cx, asking(Some(force())));
    settle(cx, &opened);
    assert!(drawn(cx, &opened, "Delete spike anyway?"));
    assert!(drawn(cx, &opened, "It has commits no other branch has, and nothing is pushed."));
    assert!(drawn(cx, &opened, "a1b2c3d Try a thing \u{b7} 40m") && drawn(cx, &opened, "e4f5a6b Try another \u{b7} 40m"));
    assert!(present(cx, &opened, "confirm-keep") && present(cx, &opened, "confirm-yes"));
}

/// CHECKOUTS-42: a yes answers yes, once, and the dialog closes with it; the table heard again
/// before the session clears the question does not put it back.
#[gpui_kit::test]
fn a_yes_is_sent_once_and_the_question_stays_away_until_the_session_clears_it(cx: &mut TestAppContext) {
    let opened = on(cx, asking(Some(removing("store-faults"))));
    settle(cx, &opened);
    click_in_dialog(cx, &opened, "confirm-yes");
    assert_eq!(drain(&opened), ["AnswerCheckouts { yes: true }"]);
    assert!(overlays(cx).is_empty());
    hear(cx, &opened, vec![heard(asking(Some(removing("store-faults"))))]);
    assert!(overlays(cx).is_empty(), "the same question heard again is not asked again");
    assert!(drain(&opened).is_empty());
    hear(cx, &opened, vec![heard(asking(None))]);
    hear(cx, &opened, vec![heard(asking(Some(removing("store-faults"))))]);
    settle(cx, &opened);
    assert_eq!(overlays(cx), [Overlay::Removal], "a new question is asked");
}

/// CHECKOUTS-42: Keep it, Escape and the close button each answer no.
#[gpui_kit::test]
fn keep_it_and_escape_answer_no(cx: &mut TestAppContext) {
    let opened = on(cx, asking(Some(removing("store-faults"))));
    settle(cx, &opened);
    click_in_dialog(cx, &opened, "confirm-keep");
    assert_eq!(drain(&opened), ["AnswerCheckouts { yes: false }"]);
    assert!(overlays(cx).is_empty());
    hear(cx, &opened, vec![heard(asking(None))]);
    hear(cx, &opened, vec![heard(asking(Some(removing("store-faults"))))]);
    settle(cx, &opened);
    press(cx, &opened, "escape");
    assert_eq!(drain(&opened), ["AnswerCheckouts { yes: false }"]);
    assert!(overlays(cx).is_empty());
}

/// CHECKOUTS-42: the question goes when the table says there is none.
#[gpui_kit::test]
fn the_question_goes_when_the_table_holds_none(cx: &mut TestAppContext) {
    let opened = on(cx, asking(Some(removing("store-faults"))));
    settle(cx, &opened);
    assert_eq!(overlays(cx), [Overlay::Removal]);
    hear(cx, &opened, vec![heard(asking(None))]);
    assert!(overlays(cx).is_empty());
    assert!(drain(&opened).is_empty(), "taking it away answers nothing");
}

/// CHECKOUTS-50: a question that replaces another without passing through none is not drawn
/// again: the old words stay.
#[gpui_kit::test]
fn a_question_replacing_another_without_none_is_not_redrawn(cx: &mut TestAppContext) {
    let opened = on(cx, asking(Some(removing("store-faults"))));
    settle(cx, &opened);
    hear(cx, &opened, vec![heard(asking(Some(removing("ratatui-bump"))))]);
    assert!(drawn(cx, &opened, "Remove store-faults?") && !drawn(cx, &opened, "Remove ratatui-bump?"));
}

/// CHECKOUTS-39: New worktree on a checkout row opens a dialog off that branch, and Create sends
/// the name; Escape sends nothing.
#[gpui_kit::test]
fn new_worktree_on_a_checkout_asks_for_a_name_and_create_sends_it(cx: &mut TestAppContext) {
    let opened = on(cx, table("c101"));
    click_label(cx, &opened, "New worktree");
    settle(cx, &opened);
    assert_eq!(overlays(cx), [Overlay::Worktree]);
    assert!(drawn(cx, &opened, "Off desktop-design") && drawn(cx, &opened, "Branch"));
    assert_eq!(drain(&opened), Vec::<String>::new(), "nothing is sent until Create");
    type_text(cx, &opened, "spike");
    click_label(cx, &opened, "Create");
    assert_eq!(drain(&opened), ["NewWorktree { name: Some(\"spike\") }"]);
    assert!(overlays(cx).is_empty());
}

#[gpui_kit::test]
fn enter_in_the_name_field_creates_and_escape_and_cancel_send_nothing(cx: &mut TestAppContext) {
    let opened = on(cx, table("c101"));
    click_label(cx, &opened, "New worktree");
    settle(cx, &opened);
    type_text(cx, &opened, "ab");
    press(cx, &opened, "enter");
    assert_eq!(drain(&opened), ["NewWorktree { name: Some(\"ab\") }"]);
    assert!(overlays(cx).is_empty());
    click_label(cx, &opened, "New worktree");
    settle(cx, &opened);
    press(cx, &opened, "escape");
    assert!(overlays(cx).is_empty() && drain(&opened).is_empty());
    click_label(cx, &opened, "New worktree");
    settle(cx, &opened);
    click_label(cx, &opened, "Cancel");
    assert!(overlays(cx).is_empty() && drain(&opened).is_empty());
}

/// CHECKOUTS-39, 49: a branch row, or no row, sends a worktree at once with no name and no dialog.
#[gpui_kit::test]
fn new_worktree_on_a_branch_row_sends_at_once(cx: &mut TestAppContext) {
    let branches = Checkouts { branches_shown: true, rows: vec![checkout(100, "main", "main"), branch("spike")], selected: Some("bspike".into()), ..table("bspike") };
    let opened = on(cx, branches);
    click_label(cx, &opened, "New worktree");
    assert_eq!(asked(&opened), ["NewWorktree { name: None }"]);
    assert!(overlays(cx).is_empty());
}
