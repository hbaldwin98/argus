//! The file area with no file read (CODE-32, 34).

use gpui_kit::TestAppContext;

use super::*;

/// CODE-34: the changed files are rows that read them, with their folder and how much changed.
#[gpui_kit::test]
fn the_start_panel_lists_what_changed_and_a_row_reads_the_file(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    assert_eq!(label(cx, &opened, ("code-changed", 0usize)).as_deref(), Some("crates/argusd/src/board/standing.rs"));
    assert_eq!(label(cx, &opened, ("code-changed", 1usize)).as_deref(), Some("DESIGN.md"));
    click_id(cx, &opened, ("code-changed", 1usize));
    assert_eq!(sent(&opened), ["Code { gesture: Click { target: Tree { path: \"DESIGN.md\" } } }"]);
}

/// CODE-34: ten at most, and how many more there are.
#[gpui_kit::test]
fn ten_files_are_listed_and_the_rest_counted(cx: &mut TestAppContext) {
    let many = CodePage { changed_files: (0..13).map(|n| changed(&format!("src/f{n}.rs"), 1, 0)).collect(), ..page() };
    let opened = with(cx, many);
    assert!(present(cx, &opened, ("code-changed", 9usize)) && !present(cx, &opened, ("code-changed", 10usize)));
    assert_eq!(label(cx, &opened, "code-more").as_deref(), Some("and 3 more, under Changed in the tree."));
}

/// CODE-34: with nothing changed it says since when, or that there are no changes.
#[gpui_kit::test]
fn with_nothing_changed_it_says_since_which_commit(cx: &mut TestAppContext) {
    let opened = with(cx, CodePage { changed_files: Vec::new(), since: Some("9d41c07".into()), ..page() });
    assert_eq!(label(cx, &opened, "code-nothing").as_deref(), Some("Nothing since 9d41c07."));
    says(cx, &opened, CodePage { changed_files: Vec::new(), ..page() });
    assert_eq!(label(cx, &opened, "code-nothing").as_deref(), Some("No changes."));
}

/// CODE-34: the files read in this checkout are remembered by the window, newest first.
#[gpui_kit::test]
fn the_files_opened_lately_are_the_windows_memory(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    assert!(!present(cx, &opened, ("code-lately", 0usize)), "nothing opened yet, no section");
    says(cx, &opened, CodePage { path: Some("src/a.rs".into()), ..page() });
    says(cx, &opened, CodePage { path: Some("DESIGN.md".into()), ..page() });
    says(cx, &opened, page());
    assert_eq!(label(cx, &opened, ("code-lately", 0usize)).as_deref(), Some("DESIGN.md"));
    assert_eq!(label(cx, &opened, ("code-lately", 1usize)).as_deref(), Some("src/a.rs"));
    click_id(cx, &opened, ("code-lately", 1usize));
    assert_eq!(sent(&opened), ["Code { gesture: Click { target: Tree { path: \"src/a.rs\" } } }"]);
}

/// CODE-34: the hint about the field is always there.
#[gpui_kit::test]
fn the_hint_names_the_hash(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    assert_eq!(label(cx, &opened, "code-hint").as_deref(), Some("Type in the field to go to a file by name; start with # to search the checkout's text instead."));
}

/// CODE-32: with a match lit and no file open, the card offers it, and its button reads it.
#[gpui_kit::test]
fn a_lit_match_with_no_file_open_is_offered_in_place_of_the_start_panel(cx: &mut TestAppContext) {
    let opened = with(cx, searching(true));
    assert!(present(cx, &opened, "code-match") && !present(cx, &opened, "code-start"));
    click_id(cx, &opened, "code-open-match");
    assert_eq!(sent(&opened), ["Code { gesture: Click { target: Match { index: 1 } } }"]);
    says(cx, &opened, searching(false));
    assert!(present(cx, &opened, "code-start"), "results not current: the card is gone");
}

/// CODE-35: with a file open the area is the file's, not the start panel.
#[gpui_kit::test]
fn a_file_open_is_the_file_views_to_draw(cx: &mut TestAppContext) {
    let opened = with(cx, CodePage { path: Some("DESIGN.md".into()), ..page() });
    assert!(present(cx, &opened, "code-file-area") && !present(cx, &opened, "code-start"));
}
