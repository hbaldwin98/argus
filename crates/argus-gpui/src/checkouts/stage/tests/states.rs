//! What fills the stage before there is a table, and where there is nothing to list
//! (CHECKOUTS-1 to 3, 6, 44).

use argus_desktop_core::checkouts::table::Checkouts;
use argus_desktop_core::connection::page::Event;
use gpui_kit::TestAppContext;

use super::*;

/// CHECKOUTS-1: before the first snapshot there are five loading bars, and no table.
#[gpui_kit::test]
fn before_the_first_table_there_are_loading_bars_and_no_table(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), Event::Rail(Box::new(rail()))]);
    assert!(drawn(cx, &opened, "Loading"));
    assert!(!drawn(cx, &opened, "Checkouts in argus"));
    assert!(!drawn(cx, &opened, "checkouts-keys"));
}

/// CHECKOUTS-2: with no repository chosen a card says so, and there is no strip tool.
#[gpui_kit::test]
fn with_no_repository_a_card_says_so_and_the_strip_has_no_tools(cx: &mut TestAppContext) {
    let opened = on(cx, Checkouts::default());
    assert!(drawn(cx, &opened, "No repository") && drawn(cx, &opened, "Choose one on the left."));
    assert!(!drawn(cx, &opened, "New worktree") && !drawn(cx, &opened, "head-meta"));
    assert!(!drawn(cx, &opened, "Loading"));
}

/// CHECKOUTS-3: with a repository and no rows the body says why by the filter, and the key bar
/// stays.
#[gpui_kit::test]
fn a_repository_with_no_rows_says_why_and_keeps_the_key_bar(cx: &mut TestAppContext) {
    let none = Checkouts { rows: Vec::new(), selected: None, ..table("c101") };
    let opened = on(cx, none.clone());
    assert!(drawn(cx, &opened, "No checkouts."));
    assert!(drawn(cx, &opened, "Enter open"), "the key bar still shows");
    assert!(!drawn(cx, &opened, "Checkouts in argus"));
    hear(cx, &opened, vec![heard(Checkouts { filter: "zzz".into(), ..none })]);
    assert!(drawn(cx, &opened, "Nothing matches \u{201c}zzz\u{201d}."));
    assert!(!drawn(cx, &opened, "No checkouts."));
}

/// CHECKOUTS-9, 48: the strip carries the unfiltered counts and the main branch.
#[gpui_kit::test]
fn the_strip_says_the_counts_and_the_main_branch(cx: &mut TestAppContext) {
    let opened = on(cx, table("c101"));
    assert!(drawn(cx, &opened, "2 checkouts \u{b7} 9 other branches \u{b7} primary main"));
    let filtered = Checkouts { filter: "desk".into(), rows: table("c101").rows.into_iter().skip(1).collect(), ..table("c101") };
    hear(cx, &opened, vec![heard(filtered)]);
    assert!(drawn(cx, &opened, "2 checkouts \u{b7} 9 other branches \u{b7} primary main"), "a filter changes no total");
}

/// CHECKOUTS-27: the key bar lists the keys, and Files only when the daemon offers them.
#[gpui_kit::test]
fn the_key_bar_lists_the_keys_and_files_only_when_offered(cx: &mut TestAppContext) {
    let opened = on(cx, table("c101"));
    for key in ["Enter open", "R review", "H history", "E files", "P pull", "N new worktree", "D delete"] {
        assert!(drawn(cx, &opened, key), "{key}");
    }
    hear(cx, &opened, vec![heard(Checkouts { files_offered: false, ..table("c101") })]);
    assert!(!drawn(cx, &opened, "E files"));
    assert!(drawn(cx, &opened, "P pull"));
}
