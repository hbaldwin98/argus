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
    assert_eq!(rows(cx, &opened), ["src", "Cargo.toml", "new.rs", "README.md"], "folders first, then the files whatever their case, as Code orders them");
    assert!(present(cx, &opened, "mark-README.md") && present(cx, &opened, "mark-new.rs"), "each change marked as Code marks it");
    assert!(!present(cx, &opened, "mark-Cargo.toml"));
    assert_eq!(label(cx, &opened, "mark-src").as_deref(), Some("1 changed file under it"));
    click(cx, &opened, ("review-tree-row", 0usize));
    assert_eq!(rows(cx, &opened), ["src", "lib.rs", "Cargo.toml", "new.rs", "README.md"], "the folder opened");
    assert!(asked(&opened).is_empty(), "a folder is the window's own");
}

#[gpui_kit::test]
fn a_changed_file_opens_its_change_and_one_with_none_is_read_whole(cx: &mut TestAppContext) {
    let opened = on_main(cx);
    hear(cx, &opened, vec![reviewing(tree(&FILES, None))]);
    click(cx, &opened, "review-list-files");
    forget(&opened);
    click(cx, &opened, ("review-tree-row", 2usize));
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
    assert_eq!(rows(cx, &opened), ["src", "lib.rs", "new.rs", "README.md"], "Cargo.toml holds no change, and a short list shows open");
    click(cx, &opened, "review-list-changed");
    click(cx, &opened, "review-find");
    type_text(cx, &opened, "LIB");
    assert_eq!(rows(cx, &opened), ["src", "lib.rs"], "by the whole path, in either case, under its folder");
}

#[gpui_kit::test]
fn a_review_with_no_files_tab_offers_none(cx: &mut TestAppContext) {
    let opened = on_main(cx);
    hear(cx, &opened, vec![reviewing(changes())]);
    assert!(!present(cx, &opened, "review-list-files"), "a daemon that cannot read code, or a commit");
}

#[gpui_kit::test]
fn j_and_k_step_through_the_files_shown_and_open_each_as_a_click_does(cx: &mut TestAppContext) {
    let opened = on_main(cx);
    hear(cx, &opened, vec![reviewing(tree(&FILES, None))]);
    click(cx, &opened, "review-list-files");
    forget(&opened);
    press(cx, &opened, "j");
    assert_eq!(asked(&opened), ["ReviewRead { path: \"Cargo.toml\" }"], "none lit among the rows shown: the first file, past the shut folder");
    let reading = ReviewReading { path: "Cargo.toml".into(), rows: vec![line(1, "context", "[workspace]")], loading: false, note: None };
    hear(cx, &opened, vec![reviewing(tree(&FILES, Some(reading)))]);
    press(cx, &opened, "j");
    assert_eq!(asked(&opened), ["OpenChange { staged: false, path: \"new.rs\" }"], "a change opens on its side");
    press(cx, &opened, "k");
    assert!(asked(&opened).is_empty(), "still on Cargo.toml, the first: nothing before it");
}

/// The list answering the tab's ask redraws the window by itself, with no input after it: the
/// events come as the daemon loop sends them, a notify and no frame forced.
#[gpui_kit::test]
fn the_list_shows_when_it_comes_in_with_no_input_after_it(cx: &mut TestAppContext) {
    let opened = on_main(cx);
    let asked_for = ReviewPage { tree: Some(ReviewTree { files: Vec::new(), listed: false, loading: true, reading: None }), ..changes() };
    hear(cx, &opened, vec![reviewing(asked_for)]);
    click(cx, &opened, "review-list-files");
    assert!(present(cx, &opened, "review-tree-loading"));
    cx.update(|cx| {
        opened.heard.update(cx, |heard, cx| {
            heard.hear(reviewing(tree(&FILES, None)));
            cx.notify();
        })
    });
    cx.run_until_parked();
    assert!(present(cx, &opened, ("review-tree-row", 0usize)), "drawn without a press");
}
