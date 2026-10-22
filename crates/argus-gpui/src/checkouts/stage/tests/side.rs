//! The column beside the table (CHECKOUTS-33 to 38).

use argus_desktop_core::checkouts::files_page::FilesPage;
use argus_desktop_core::checkouts::table::{Checkouts, CheckoutsRow};
use argus_desktop_core::chrome::cursor::{RailCursorPage, StartFrom, StartMenu};
use gpui_kit::TestAppContext;

use super::*;
use crate::keys::root::tests::open_sized;

/// CHECKOUTS-33: the column is of the selected row: its name, its path cut at the first
/// `/.argus/`, and its note.
#[gpui_kit::test]
fn the_column_names_the_selected_row_and_its_path_from_the_argus_folder(cx: &mut TestAppContext) {
    let opened = on(cx, table("c101"));
    assert_eq!(label(cx, &opened, "side").as_deref(), Some("desktop, selected"));
    assert_eq!(label(cx, &opened, "side-title").as_deref(), Some("desktop"));
    assert_eq!(label(cx, &opened, "side-path").as_deref(), Some(".argus/worktrees/desktop"), "forward slashes, from the first /.argus/");
    hear(cx, &opened, vec![heard(table("c100"))]);
    assert_eq!(label(cx, &opened, "side").as_deref(), Some("main, selected"), "it follows the selection");
}

/// CHECKOUTS-34: its buttons are keys the table answers: Open, Review R, History H, Pull P.
#[gpui_kit::test]
fn the_buttons_send_what_their_keys_send(cx: &mut TestAppContext) {
    let opened = on(cx, table("c101"));
    for (id, sent) in [
        ("side-open", "CheckoutsAction { action: Choose }"),
        ("side-review", "OpenReview { checkout: 101 }"),
        ("side-history", "OpenHistory { checkout: 101 }"),
        ("side-pull", "CheckoutsAction { action: Pull }"),
    ] {
        click_id(cx, &opened, id);
        assert_eq!(drain(&opened), [sent], "{id}: the row is the selected one, so it is not picked again");
    }
}

/// CHECKOUTS-34: a branch row has Check out, which sends a worktree with no name.
#[gpui_kit::test]
fn a_branch_row_has_check_out_and_no_facts(cx: &mut TestAppContext) {
    let branches = Checkouts { branches_shown: true, rows: vec![checkout(100, "main", "main"), branch("spike")], selected: Some("bspike".into()), detail: None, ..table("bspike") };
    let opened = on(cx, branches);
    assert_eq!(label(cx, &opened, "side-title").as_deref(), Some("spike"));
    assert!(!present(cx, &opened, "side-facts") && !present(cx, &opened, "side-open"));
    click_id(cx, &opened, "side-check-out");
    assert_eq!(drain(&opened), ["NewWorktree { name: None }"]);
}

/// CHECKOUTS-35: the facts, each said by Rust.
#[gpui_kit::test]
fn the_facts_are_upstream_changes_feature_and_the_last_commit(cx: &mut TestAppContext) {
    let opened = on(cx, table("c101"));
    for fact in ["Upstream", "Changes", "Feature", "Last commit"] {
        assert!(present(cx, &opened, fact), "{fact}");
    }
    let quiet = Checkouts { rows: vec![checkout(100, "main", "main")], selected: Some("c100".into()), ..table("c100") };
    hear(cx, &opened, vec![heard(quiet)]);
    assert!(present(cx, &opened, "Upstream") && !present(cx, &opened, "Feature") && !present(cx, &opened, "Last commit"), "a fact the row lacks is not drawn");
}

/// CHECKOUTS-36: what runs here is a button to each pane, which goes to it.
#[gpui_kit::test]
fn each_running_pane_is_a_button_that_goes_to_it(cx: &mut TestAppContext) {
    let opened = on(cx, table("c101"));
    assert_eq!(label(cx, &opened, "side-running-heading").as_deref(), Some("Running here"));
    click_id(cx, &opened, ("side-pane", 1u64));
    assert_eq!(drain(&opened), ["GoToPane { pane: 1 }"]);
    let shared = CheckoutsRow { shared: Some(2), running: vec![running(1, "a", "working", true), running(2, "b", "idle", false)], ..table("c101").rows[1].clone() };
    hear(cx, &opened, vec![heard(Checkouts { rows: vec![shared], ..table("c101") })]);
    assert_eq!(label(cx, &opened, "side-running-heading").as_deref(), Some("Running here \u{b7} shared by 2"));
    hear(cx, &opened, vec![heard(Checkouts { rows: vec![checkout(101, "desktop", "desktop")], ..table("c101") })]);
    assert!(!present(cx, &opened, "side-running"), "nothing running, no section");
}

/// CHECKOUTS-37: the foot offers Files, the folder, a new pane and the removal; each is a key.
#[gpui_kit::test]
fn the_foot_sends_files_the_folder_and_the_removal(cx: &mut TestAppContext) {
    let opened = on(cx, table("c101"));
    for (id, sent) in [
        ("side-files", "CheckoutsAction { action: Files }"),
        ("side-reveal", "RevealCheckout { checkout: 101 }"),
        ("side-remove", "CheckoutsAction { action: Delete }"),
    ] {
        click_id(cx, &opened, id);
        assert_eq!(drain(&opened), [sent], "{id}");
    }
}

/// CHECKOUTS-37: Files and the folder only when the daemon offers them, and no removal for the
/// primary checkout.
#[gpui_kit::test]
fn the_foot_leaves_out_what_is_not_offered_and_the_primary_has_no_removal(cx: &mut TestAppContext) {
    let opened = on(cx, Checkouts { files_offered: false, reveal: None, ..table("c100") });
    assert!(!present(cx, &opened, "side-files") && !present(cx, &opened, "side-reveal") && !present(cx, &opened, "side-remove"));
    assert!(present(cx, &opened, "side-new-pane"));
}

/// CHECKOUTS-37: New pane toggles the start menu for the checkout, from the table, and the menu is
/// drawn while the session says it is open; a choice starts it.
#[gpui_kit::test]
fn new_pane_toggles_the_start_menu_and_a_choice_starts_there(cx: &mut TestAppContext) {
    let opened = on(cx, table("c101"));
    click_id(cx, &opened, "side-new-pane");
    assert_eq!(drain(&opened), ["ToggleStart { checkout: 101, from: Table }"]);
    assert!(!present(cx, &opened, "start-menu-101"), "the menu is the session's word");
    let mut open = rail();
    open.cursor = RailCursorPage { pick: None, start: Some(StartMenu { checkout: 101, from: StartFrom::Table, at: 0 }) };
    hear(cx, &opened, vec![Event::Rail(Box::new(open))]);
    assert!(present(cx, &opened, "start-menu-101"));
    assert_eq!(expanded(cx, &opened, "side-new-pane"), Some(true));
    click_id(cx, &opened, "start-101-1");
    assert_eq!(drain(&opened), ["StartOn { checkout: 101, template: Some(\"claude\") }"]);
}

/// CHECKOUTS-38: the Files panel takes the column's place, and the table keeps its room.
#[gpui_kit::test]
fn the_files_panel_takes_the_columns_place(cx: &mut TestAppContext) {
    let opened = on(cx, table("c101"));
    hear(cx, &opened, vec![heard(Checkouts { files: Some(FilesPage { checkout: 101, name: "desktop".into(), ..FilesPage::default() }), ..table("c101") })]);
    assert!(!present(cx, &opened, "side") && present(cx, &opened, "checkouts-files"));
    assert!(present(cx, &opened, "row-c100"), "the table keeps its room");
}

/// A narrow window has no column: the overflow on each row holds what it does.
#[gpui_kit::test]
fn a_narrow_window_has_no_column(cx: &mut TestAppContext) {
    let opened = open_sized(cx, 1000.0);
    hear(cx, &opened, vec![connected(), Event::Rail(Box::new(rail())), heard(table("c101"))]);
    assert!(!present(cx, &opened, "side"), "below 1060px");
    assert!(present(cx, &opened, "row-c101"));
}
