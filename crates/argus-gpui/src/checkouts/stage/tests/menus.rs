//! The strip's menu and a row's menu (CHECKOUTS-14, 25, 26).

use argus_desktop_core::checkouts::table::Checkouts;
use gpui_kit::TestAppContext;

use super::*;

fn with_branch() -> Checkouts {
    let mut table = table("c101");
    table.branches_shown = true;
    table.rows.push(branch("old"));
    table
}

/// CHECKOUTS-14: the strip's `⋯` shows or hides the branches and fetches all.
#[gpui_kit::test]
fn the_strips_menu_toggles_the_branches_and_fetches_all(cx: &mut TestAppContext) {
    let opened = on(cx, table("c101"));
    assert_eq!(label(cx, &opened, "checkouts-more").as_deref(), Some("More about checkouts"));
    click_id(cx, &opened, "checkouts-more-button");
    click_menu_item(cx, &opened, 0);
    assert_eq!(drain(&opened), ["CheckoutsAction { action: Branches }"]);
    click_id(cx, &opened, "checkouts-more-button");
    click_menu_item(cx, &opened, 1);
    assert_eq!(drain(&opened), ["CheckoutsAction { action: Fetch }"]);
}

/// CHECKOUTS-26: a checkout row's menu: Review and History name the checkout and pick no row; Pull
/// picks the row first when it is not the selected one.
#[gpui_kit::test]
fn a_checkout_rows_menu_reviews_and_pulls_by_the_row_it_is_of(cx: &mut TestAppContext) {
    let opened = on(cx, table("c101"));
    hover_id(cx, &opened, "row-c100");
    hover_id(cx, &opened, "row-c100");
    click_id(cx, &opened, "row-more-button-c100");
    click_menu_item(cx, &opened, 0);
    assert_eq!(drain(&opened), ["OpenReview { checkout: 100 }"]);
    hover_id(cx, &opened, "row-c100");
    click_id(cx, &opened, "row-more-button-c100");
    click_menu_item(cx, &opened, 2);
    assert_eq!(drain(&opened), ["SelectCheckoutRow { key: \"c100\" }", "CheckoutsAction { action: Pull }"]);
}

/// CHECKOUTS-26: a shell and an agent per template start on that row's checkout, whichever row is
/// selected; the primary checkout has no removal in its menu.
#[gpui_kit::test]
fn the_menu_starts_a_shell_or_an_agent_per_template_on_its_own_checkout(cx: &mut TestAppContext) {
    let opened = on(cx, table("c101"));
    // Review, History, Pull, Files, Show in Explorer, a rule, New shell, claude, codex.
    hover_id(cx, &opened, "row-c100");
    click_id(cx, &opened, "row-more-button-c100");
    click_menu_item(cx, &opened, 6);
    assert_eq!(drain(&opened), ["SpawnShell { checkout: 100 }"]);
    hover_id(cx, &opened, "row-c100");
    click_id(cx, &opened, "row-more-button-c100");
    click_menu_item(cx, &opened, 8);
    assert_eq!(drain(&opened), ["SpawnAgent { checkout: 100, template: \"codex\" }"]);
}

/// CHECKOUTS-26: a branch row's menu offers a worktree and the deletion, each picking the row first.
#[gpui_kit::test]
fn a_branch_rows_menu_offers_a_worktree_and_a_deletion(cx: &mut TestAppContext) {
    let opened = on(cx, with_branch());
    hover_id(cx, &opened, "row-bold");
    click_id(cx, &opened, "row-more-button-bold");
    click_menu_item(cx, &opened, 0);
    assert_eq!(drain(&opened), ["SelectCheckoutRow { key: \"bold\" }", "NewWorktree { name: None }"]);
    hover_id(cx, &opened, "row-bold");
    click_id(cx, &opened, "row-more-button-bold");
    click_menu_item(cx, &opened, 2);
    assert_eq!(drain(&opened), ["SelectCheckoutRow { key: \"bold\" }", "CheckoutsAction { action: Delete }"]);
}

/// Removing a worktree is asked of the session, which holds the question.
#[gpui_kit::test]
fn delete_on_a_worktree_row_asks_the_session_which_asks_the_question(cx: &mut TestAppContext) {
    let opened = on(cx, table("c101"));
    hover_id(cx, &opened, "row-c101");
    click_id(cx, &opened, "row-more-button-c101");
    // Review, History, Pull, Files, Show in Explorer, a rule, New shell, claude, codex, a rule, Remove.
    click_menu_item(cx, &opened, 10);
    assert_eq!(drain(&opened), ["CheckoutsAction { action: Delete }"], "the question is the session's to hold");
}

/// A menu opened before the daemon went is chosen from after it did: nothing is sent.
#[gpui_kit::test]
fn a_menu_chosen_from_after_the_daemon_went_sends_nothing(cx: &mut TestAppContext) {
    let opened = on(cx, table("c101"));
    hover_id(cx, &opened, "row-c100");
    click_id(cx, &opened, "row-more-button-c100");
    hear(cx, &opened, vec![Event::Status(Status { connection: Connection::Stopped, detail: None, alarm: None, chime: true, host: None })]);
    drain(&opened);
    click_menu_item(cx, &opened, 0);
    assert!(drain(&opened).is_empty());
}
