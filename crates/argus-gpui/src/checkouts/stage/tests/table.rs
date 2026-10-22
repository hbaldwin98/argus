//! The table: its rows, the fold, the State cell, the selection and what a click sends
//! (CHECKOUTS-5, 6, 15 to 25, 48).

use argus_desktop_core::checkouts::table::{Checkouts, CheckoutsRow};
use gpui_kit::TestAppContext;

use super::*;
use crate::keys::root::tests::{click_label, press};

fn with_branches(shown: bool) -> Checkouts {
    let mut table = table("c101");
    table.branches_shown = shown;
    if shown {
        table.rows.push(branch("old"));
        table.rows.push(CheckoutsRow { note: Some("remote only"), key: "rorigin/x".into(), ..branch("origin/x") });
    }
    table
}

/// CHECKOUTS-15, 16: the checkouts, then the fold that says how many branches have none, then the
/// branch rows while they are shown.
#[gpui_kit::test]
fn checkouts_come_first_then_the_fold_and_the_branches_only_while_shown(cx: &mut TestAppContext) {
    let opened = on(cx, with_branches(false));
    assert!(present(cx, &opened, "row-c100") && present(cx, &opened, "row-c101"));
    assert!(drawn(cx, &opened, "9 branches without a checkout"));
    assert_eq!(expanded(cx, &opened, "branches-fold"), Some(false));
    assert!(!present(cx, &opened, "row-bold"), "branch rows are in the table only while shown");
    hear(cx, &opened, vec![heard(with_branches(true))]);
    assert!(present(cx, &opened, "row-bold") && present(cx, &opened, "row-rorigin/x"));
    assert_eq!(expanded(cx, &opened, "branches-fold"), Some(true));
}

/// CHECKOUTS-16: a press on the fold is the `B` key.
#[gpui_kit::test]
fn the_fold_sends_the_branches_action_and_picks_no_row(cx: &mut TestAppContext) {
    let opened = on(cx, with_branches(false));
    click_id(cx, &opened, "branches-fold");
    assert_eq!(drain(&opened), ["CheckoutsAction { action: Branches }"]);
}

/// CHECKOUTS-48: the fold is there while the filter hides every branch row, and absent with none.
#[gpui_kit::test]
fn the_fold_stays_while_a_filter_hides_the_branches_and_goes_with_none(cx: &mut TestAppContext) {
    let opened = on(cx, with_branches(false));
    assert!(present(cx, &opened, "branches-fold"));
    hear(cx, &opened, vec![heard(Checkouts { branches: 0, ..with_branches(false) })]);
    assert!(!present(cx, &opened, "branches-fold"));
}

/// CHECKOUTS-17, 18: a checkout names itself, and a branch row names its branch.
#[gpui_kit::test]
fn a_checkout_row_names_itself_and_a_branch_row_its_branch(cx: &mut TestAppContext) {
    let opened = on(cx, with_branches(true));
    assert_eq!(label(cx, &opened, "row-c100").as_deref(), Some("main"));
    assert_eq!(label(cx, &opened, "row-c101").as_deref(), Some("desktop"));
    assert_eq!(label(cx, &opened, "row-bold").as_deref(), Some("old"), "a branch row is never blank");
}

/// CHECKOUTS-19, 20: the State cell says only what needs a look, and a quiet row says nothing.
#[gpui_kit::test]
fn the_state_cell_says_what_needs_a_look_and_a_quiet_row_says_nothing(cx: &mut TestAppContext) {
    let opened = on(cx, with_branches(true));
    assert_eq!(label(cx, &opened, "state-c101").as_deref(), Some("6 changed \u{2191}3 1 pane #42"));
    assert_eq!(label(cx, &opened, "state-c100").as_deref(), Some(""));
    assert_eq!(label(cx, &opened, "state-rorigin/x").as_deref(), Some("remote only"));
    assert_eq!(label(cx, &opened, ("running-dot", 1u64)).as_deref(), Some("claude \u{b7} screenshots: working"), "a status dot for each running pane");
}

/// CHECKOUTS-22: the selected row says so, and which it is is the session's word.
#[gpui_kit::test]
fn the_selection_is_the_tables_word_and_moves_when_it_says_so(cx: &mut TestAppContext) {
    let opened = on(cx, table("c101"));
    assert_eq!((selected(cx, &opened, "row-c101"), selected(cx, &opened, "row-c100")), (Some(true), Some(false)));
    click_id(cx, &opened, "row-c100");
    assert_eq!(drain(&opened), ["SelectCheckoutRow { key: \"c100\" }"]);
    assert_eq!(selected(cx, &opened, "row-c100"), Some(false), "a click predicts nothing");
    hear(cx, &opened, vec![heard(table("c100"))]);
    assert_eq!((selected(cx, &opened, "row-c100"), selected(cx, &opened, "row-c101")), (Some(true), Some(false)));
}

/// CHECKOUTS-24: a double click picks the row and then opens it.
#[gpui_kit::test]
fn a_double_click_picks_the_row_and_opens_it(cx: &mut TestAppContext) {
    let opened = on(cx, table("c101"));
    double_click_id(cx, &opened, "row-c100");
    let sent = drain(&opened);
    assert_eq!(sent.last().map(String::as_str), Some("CheckoutsAction { action: Choose }"));
    assert!(sent.iter().all(|call| call == "SelectCheckoutRow { key: \"c100\" }" || call == "CheckoutsAction { action: Choose }"), "{sent:?}");
    assert_eq!(sent.iter().filter(|call| call.starts_with("CheckoutsAction")).count(), 1, "opened once");
}

/// CHECKOUTS-25: a row's own button picks the row first when it is not the selected one, and a
/// branch row's is Check out.
#[gpui_kit::test]
fn a_rows_open_button_picks_it_first_and_a_branch_rows_checks_out(cx: &mut TestAppContext) {
    let opened = on(cx, with_branches(true));
    hover_id(cx, &opened, "row-c100");
    click_id(cx, &opened, "row-open-c100");
    assert_eq!(drain(&opened), ["SelectCheckoutRow { key: \"c100\" }", "CheckoutsAction { action: Choose }"]);
    click_id(cx, &opened, "row-open-c101");
    assert_eq!(drain(&opened), ["CheckoutsAction { action: Choose }"], "the selected row is not picked again");
    hover_id(cx, &opened, "row-bold");
    click_id(cx, &opened, "row-open-bold");
    assert_eq!(drain(&opened), ["SelectCheckoutRow { key: \"bold\" }", "NewWorktree { name: None }"]);
}

/// CHECKOUTS-5, 6: what the table says is what is drawn, whole, each time it is heard.
#[gpui_kit::test]
fn a_table_heard_again_replaces_what_was_drawn(cx: &mut TestAppContext) {
    let opened = on(cx, table("c101"));
    let fewer = Checkouts { rows: table("c101").rows.into_iter().take(1).collect(), selected: None, ..table("c101") };
    hear(cx, &opened, vec![heard(fewer)]);
    assert!(present(cx, &opened, "row-c100") && !present(cx, &opened, "row-c101"));
    assert!(!present(cx, &opened, "side"), "no selected row, no side column");
}

/// A held window (the daemon gone) sends nothing from a click, a button or a key.
#[gpui_kit::test]
fn a_held_window_sends_nothing(cx: &mut TestAppContext) {
    let opened = on(cx, table("c101"));
    hear(cx, &opened, vec![Event::Status(Status { connection: Connection::Stopped, detail: None, alarm: None, chime: true, host: None })]);
    drain(&opened);
    click_id(cx, &opened, "row-c100");
    click_label(cx, &opened, "New worktree");
    press(cx, &opened, "j");
    assert!(drain(&opened).is_empty());
}
