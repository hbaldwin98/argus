//! The Changes pane's Files tab in the whole window: the tab asks for the checkout's files, the
//! tree opens a folder, marks a change and opens it on its side, reads a file with no change whole,
//! and the Changed filter and the find narrow it. Each press sends its call, and what is read shows
//! when the session says so.

use super::changes::changes;
use super::*;
use argus_desktop_core::connection::page::{ReviewReading, ReviewTree};

fn asked(opened: &Opened) -> Vec<String> {
    let said = taken(opened);
    forget(opened);
    said
}

fn tree(files: &[&str], reading: Option<ReviewReading>) -> ReviewPage {
    let files = files.iter().map(|f| (*f).to_owned()).collect();
    ReviewPage { tree: Some(ReviewTree { files, listed: true, loading: false, reading }, ), ..changes() }
}

const FILES: [&str; 4] = ["Cargo.toml", "README.md", "new.rs", "src/lib.rs"];

/// The rows' labels, in order.
fn rows(cx: &mut TestAppContext, opened: &Opened) -> Vec<String> {
    (0..10usize).map_while(|at| label(cx, opened, ("review-tree-row", at))).collect()
}

#[gpui_kit::test]
fn the_files_tab_asks_for_the_checkout_and_lists_it_by_folder_with_the_changes_marked(cx: &mut TestAppContext) {
    let opened = on_main(cx);
    hear(cx, &opened, vec![reviewing(tree(&FILES, None))]);
    assert!(present(cx, &opened, "review-list-files"));
    forget(&opened);
    click(cx, &opened, "review-list-files");
    assert_eq!(asked(&opened), ["ReviewFiles"]);
    assert!(!present(cx, &opened, "review-commit-box"), "the commit box is the changes' own");
    assert_eq!(rows(cx, &opened), ["src", "Cargo.toml", "README.md, modified", "new.rs, untracked"], "each change said by its kind");
    click(cx, &opened, ("review-tree-row", 0usize));
    assert_eq!(rows(cx, &opened), ["src", "src/lib.rs, modified", "Cargo.toml", "README.md, modified", "new.rs, untracked"], "the folder opened");
    assert!(asked(&opened).is_empty(), "a folder is the window's own");
}

#[gpui_kit::test]
fn a_changed_file_opens_its_change_and_one_with_none_is_read_whole(cx: &mut TestAppContext) {
    let opened = on_main(cx);
    hear(cx, &opened, vec![reviewing(tree(&FILES, None))]);
    click(cx, &opened, "review-list-files");
    forget(&opened);
    click(cx, &opened, ("review-tree-row", 3usize));
    assert_eq!(asked(&opened), ["OpenChange { staged: false, path: \"new.rs\" }"]);
    click(cx, &opened, ("review-tree-row", 1usize));
    assert_eq!(asked(&opened), ["ReviewRead { path: \"Cargo.toml\" }"]);
    let reading = ReviewReading { path: "Cargo.toml".into(), rows: vec![line(1, "context", "[workspace]")], loading: false, note: None };
    hear(cx, &opened, vec![reviewing(tree(&FILES, Some(reading)))]);
    assert!(present(cx, &opened, "review-unchanged"), "its head says it is unchanged");
    assert!(present(cx, &opened, "review-whole-rows"), "its lines in place of the diff");
    assert!(!present(cx, &opened, "review-stage-file"), "nothing to stage");
    assert_eq!(selected(cx, &opened, ("review-tree-row", 1usize)), Some(true), "lit in the tree");
    click(cx, &opened, "review-open");
    assert_eq!(asked(&opened), ["ReviewAction { action: OpenCode }"]);
}

#[gpui_kit::test]
fn changed_only_and_a_find_narrow_the_files(cx: &mut TestAppContext) {
    let opened = on_main(cx);
    hear(cx, &opened, vec![reviewing(tree(&FILES, None))]);
    click(cx, &opened, "review-list-files");
    click(cx, &opened, "review-list-changed");
    assert_eq!(rows(cx, &opened), ["src", "README.md, modified", "new.rs, untracked"], "Cargo.toml holds no change");
    click(cx, &opened, "review-list-changed");
    click(cx, &opened, "review-find");
    type_text(cx, &opened, "LIB");
    assert_eq!(rows(cx, &opened), ["src/lib.rs, modified"], "flat, by the whole path, in either case");
}

#[gpui_kit::test]
fn a_review_with_no_files_tab_offers_none(cx: &mut TestAppContext) {
    let opened = on_main(cx);
    hear(cx, &opened, vec![reviewing(changes())]);
    assert!(!present(cx, &opened, "review-list-files"), "a daemon that cannot read code, or a commit");
}
