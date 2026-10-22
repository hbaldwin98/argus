//! The tree: its rows, marks, fold and what a press on a row sends (FILES-10, 11, 14 to 16, 24).

use gpui_kit::TestAppContext;

use super::*;
use crate::checkouts::stage::tests::{click_id, expanded, label, present};

fn row_id(at: usize) -> (&'static str, usize) {
    ("files-row", at)
}

/// FILES-10: the rows are core's, in its order, the selected one lit and a folder saying if it is open.
#[gpui_kit::test]
fn each_row_says_whether_it_is_selected_and_a_folder_whether_it_is_open(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    // `.argus` and `target` are folded away, so the first row is `crates`.
    for at in [1usize, 2, 3, 4, 5] {
        assert!(present(cx, &opened, row_id(at)), "row {at}");
    }
    assert_eq!((is_selected(cx, &opened, row_id(3)), is_selected(cx, &opened, row_id(5))), (Some(true), Some(false)));
    assert_eq!((is_expanded(cx, &opened, row_id(1)), is_expanded(cx, &opened, row_id(2))), (Some(true), Some(true)));
    assert_eq!(is_expanded(cx, &opened, row_id(3)), None, "a file is not expandable");
}

/// FILES-10: a changed file says `M` and an untracked one `?`, each with its word for a reader.
#[gpui_kit::test]
fn marks_are_m_for_changed_and_a_question_mark_for_untracked(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    assert_eq!(label(cx, &opened, "mark-crates/argus-desktop/style.css").as_deref(), Some("Changed"));
    assert_eq!(label(cx, &opened, "mark-crates/notes.txt").as_deref(), Some("Untracked"));
    assert!(!present(cx, &opened, "mark-Cargo.toml"), "a clean file has no mark");
}

/// FILES-11, 23: the ignored top-level folders are held in one row at the foot that opens on a press.
#[gpui_kit::test]
fn the_ignored_folders_are_held_in_one_row_that_opens_on_a_press(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    assert_eq!(label(cx, &opened, "files-fold").as_deref(), Some("2 ignored \u{b7} .argus, target"));
    assert_eq!(expanded(cx, &opened, "files-fold"), Some(false));
    assert!(!present(cx, &opened, row_id(0)) && !present(cx, &opened, row_id(6)), "closed, the ignored rows are not drawn");
    click_id(cx, &opened, "files-fold");
    assert_eq!(expanded(cx, &opened, "files-fold"), Some(true));
    assert!(present(cx, &opened, row_id(0)) && present(cx, &opened, row_id(6)));
    assert!(sent(&opened).is_empty(), "the fold is the window's own memory: nothing is sent");
    says(cx, &opened, page());
    assert_eq!(expanded(cx, &opened, "files-fold"), Some(true), "a redraw does not close it");
}

/// FILES-11: a tree with nothing ignored has no fold row.
#[gpui_kit::test]
fn no_ignored_entry_means_no_fold(cx: &mut TestAppContext) {
    let mut files = page();
    files.rows.retain(|row| row.mark != Some(argus_desktop_core::checkouts::files_page::FileMark::Ignored));
    let opened = with(cx, files);
    assert!(!present(cx, &opened, "files-fold"));
}

/// FILES-14: a click sends the row's path, and selects nothing until the page says so.
#[gpui_kit::test]
fn a_click_sends_the_path_and_selects_nothing_itself(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    click_id(cx, &opened, row_id(5));
    assert_eq!(sent(&opened), ["Files { gesture: Click { path: \"Cargo.toml\" } }"]);
    assert_eq!(is_selected(cx, &opened, row_id(5)), Some(false), "the row is selected when the page says so, not before");
}

/// FILES-14: a double click on a folder is two clicks and opens nothing.
#[gpui_kit::test]
fn a_double_click_on_a_folder_only_clicks_it(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    click_times(cx, &opened, row_id(1), 2);
    assert_eq!(sent(&opened), ["Files { gesture: Click { path: \"crates\" } }", "Files { gesture: Click { path: \"crates\" } }"]);
}

/// FILES-14: the second click of a double click on a file opens it in Code.
#[gpui_kit::test]
fn the_second_click_on_a_file_opens_it_in_code(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    click_times(cx, &opened, row_id(5), 2);
    assert_eq!(
        sent(&opened),
        ["Files { gesture: Click { path: \"Cargo.toml\" } }", "Files { gesture: Click { path: \"Cargo.toml\" } }", "Files { gesture: Open { path: \"Cargo.toml\", how: Code } }"]
    );
}

/// FILES-15: the selected file carries the two buttons, which open it and select nothing.
#[gpui_kit::test]
fn the_selected_file_has_two_buttons_that_open_it_and_nothing_else(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    assert_eq!(label(cx, &opened, "files-open-code").as_deref(), Some("Open style.css in Code"));
    assert_eq!(label(cx, &opened, "files-open-editor").as_deref(), Some("Open style.css in the editor"));
    click_id(cx, &opened, "files-open-code");
    click_id(cx, &opened, "files-open-editor");
    assert_eq!(
        sent(&opened),
        [
            "Files { gesture: Open { path: \"crates/argus-desktop/style.css\", how: Code } }",
            "Files { gesture: Open { path: \"crates/argus-desktop/style.css\", how: Editor } }"
        ],
        "no Click: the buttons do not toggle or select the row"
    );
    says(cx, &opened, FilesPage { selected: Some("crates".into()), ..page() });
    assert!(!present(cx, &opened, "files-open-code"), "a folder, or another row, has none");
}

/// FILES-14, 16, 24: a right-click picks the row first, then its menu opens; Reveal names the row's path.
#[gpui_kit::test]
fn a_right_click_picks_the_row_and_its_menu_reveals_the_row_not_the_checkout(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    right_click(cx, &opened, row_id(5));
    assert_eq!(sent(&opened), ["Files { gesture: Select { path: \"Cargo.toml\" } }"]);
    choose(cx, &opened, "Show in Explorer");
    assert_eq!(sent(&opened), ["Files { gesture: Reveal { path: Some(\"Cargo.toml\") } }"]);
}

/// FILES-16: the copies are the host's spelling and the relative path.
#[gpui_kit::test]
fn the_copies_are_the_hosts_path_and_the_relative_one(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    right_click(cx, &opened, row_id(4));
    sent(&opened);
    choose(cx, &opened, "Copy path");
    assert_eq!(sent(&opened), ["CopyText { text: \"C:\\\\src\\\\desktop\\\\crates\\\\notes.txt\" }"]);
    right_click(cx, &opened, row_id(4));
    sent(&opened);
    choose(cx, &opened, "Copy relative path");
    assert_eq!(sent(&opened), ["CopyText { text: \"crates/notes.txt\" }"]);
}
