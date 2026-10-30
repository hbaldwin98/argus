//! The files column: the scope row, the tree and its fold, the go-to list and the search
//! (CODE-13 to 18, 22 to 29, 52).

use gpui_kit::TestAppContext;

use super::*;

/// CODE-13: the checkout's name, and which of its files the tree lists; a click sends and waits.
#[gpui_kit::test]
fn the_scope_row_names_the_checkout_and_says_which_files_are_listed(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    // Code.dc.html: the files column is the field, then the tree; the branch is in the title bar's crumb.
    assert!(!present(cx, &opened, "code-scope"));
    assert_eq!((is_selected(cx, &opened, "code-all"), is_selected(cx, &opened, "code-changed-only")), (Some(true), Some(false)));
    click_id(cx, &opened, "code-changed-only");
    assert_eq!(sent(&opened), ["Code { gesture: Click { target: ChangedOnly { only: true } } }"]);
    assert_eq!(is_selected(cx, &opened, "code-changed-only"), Some(false), "the client never predicts");
    says(cx, &opened, CodePage { changed_only: true, ..page() });
    assert_eq!(is_selected(cx, &opened, "code-changed-only"), Some(true));
}

/// CODE-14: four bars while the tree is loading.
#[gpui_kit::test]
fn the_tree_is_four_bars_while_it_loads(cx: &mut TestAppContext) {
    let opened = with(cx, CodePage { tree_loading: true, tree: Vec::new(), ..page() });
    assert!(present(cx, &opened, "code-loading"));
}

/// CODE-15, 17: a folder click toggles, a file click reads, and marks say what changed.
#[gpui_kit::test]
fn a_folder_click_and_a_file_click_are_the_tree_gesture_with_the_path(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    click_id(cx, &opened, ("code-row", 0usize));
    click_id(cx, &opened, ("code-row", 1usize));
    assert_eq!(sent(&opened), ["Code { gesture: Click { target: Tree { path: \"crates\" } } }", "Code { gesture: Click { target: Tree { path: \"AGENTS.md\" } } }"]);
    assert_eq!(label(cx, &opened, "mark-crates").as_deref(), Some("1 changed file under it"));
    assert_eq!(label(cx, &opened, "mark-DESIGN.md").as_deref(), Some("Changed: +3 \u{2212}1"));
    assert!(!present(cx, &opened, "mark-AGENTS.md"));
}

/// CODE-18: the ignored top-level entries are one fold row and open to nothing.
#[gpui_kit::test]
fn the_ignored_entries_are_a_fold_that_opens_to_names_only(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    assert_eq!(label(cx, &opened, "code-fold").as_deref(), Some("2 ignored \u{b7} target, target-check"));
    assert!(!present(cx, &opened, ("code-row", 3usize)));
    click_id(cx, &opened, "code-fold");
    assert!(present(cx, &opened, ("code-row", 3usize)));
    click_id(cx, &opened, ("code-row", 3usize));
    assert!(sent(&opened).is_empty(), "an ignored entry is only named: its press is a no-op");
}

/// CODE-12: the field opens the go-to list when it is reached with no list open.
#[gpui_kit::test]
fn reaching_the_field_opens_the_go_to_list(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    click_id(cx, &opened, "code-find");
    assert_eq!(sent(&opened), ["Code { gesture: Action { action: Filter } }"]);
}

/// CODE-20 to 23: the go-to list, the one Enter reads lit, a row reading its path, and the empty line.
#[gpui_kit::test]
fn the_go_to_list_lights_its_match_and_a_row_reads_its_path(cx: &mut TestAppContext) {
    let opened = with(cx, jumping());
    assert!(!present(cx, &opened, "code-scope-row"), "the list is in place of the tree");
    assert_eq!((is_selected(cx, &opened, ("code-jump", 0usize)), is_selected(cx, &opened, ("code-jump", 1usize))), (Some(false), Some(true)));
    click_id(cx, &opened, ("code-jump", 0usize));
    assert_eq!(sent(&opened), ["Code { gesture: Click { target: Tree { path: \"src/main.rs\" } } }"]);
    says(cx, &opened, CodePage { jump: Some(CodeJump { query: "zz".into(), matches: Vec::new(), at: None }), ..page() });
    assert_eq!(label(cx, &opened, "code-jump-none").as_deref(), Some("No file matches \u{201c}zz\u{201d}."));
}

/// CODE-21: typing is sent as it stands to the list that is open, and its keys step, choose and close.
#[gpui_kit::test]
fn the_field_types_and_steps_in_the_go_to_list(cx: &mut TestAppContext) {
    let opened = with(cx, jumping());
    assert_eq!(label(cx, &opened, "code-find").as_deref(), Some("Go to a file, or # for text"));
    type_text(cx, &opened, "x");
    assert_eq!(sent(&opened), ["Code { gesture: Jump { gesture: Query { text: \"x\" } } }"]);
    for (key, gesture) in [("down", "Step { by: 1 }"), ("ctrl-n", "Step { by: 1 }"), ("up", "Step { by: -1 }"), ("ctrl-p", "Step { by: -1 }"), ("enter", "Choose"), ("escape", "Close")] {
        press(cx, &opened, key);
        assert_eq!(sent(&opened), [format!("Code {{ gesture: Jump {{ gesture: {gesture} }} }}")], "{key}");
    }
}

/// CODE-24: the field shows a search as `#query`, and typing a `#` is sent to whichever list is open.
#[gpui_kit::test]
fn a_hash_typed_is_sent_as_text_and_the_search_is_shown_with_its_hash(cx: &mut TestAppContext) {
    let opened = with(cx, jumping());
    type_text(cx, &opened, "#");
    assert_eq!(sent(&opened), ["Code { gesture: Jump { gesture: Query { text: \"#\" } } }"]);
    says(cx, &opened, searching(false));
    type_text(cx, &opened, "!");
    assert_eq!(sent(&opened), ["Code { gesture: Search { gesture: Query { text: \"#!\" } } }"], "what is typed still means the search, so it is kept and the letter goes to the search list");
}

/// CODE-25, 26, 29: the search's options, summary, files and hits; the hits only while current.
#[gpui_kit::test]
fn the_search_has_options_a_summary_and_its_hits_grouped_by_file(cx: &mut TestAppContext) {
    let opened = with(cx, searching(true));
    for (id, option) in [("code-opt-case", "case"), ("code-opt-word", "word"), ("code-opt-regex", "regex")] {
        click_id(cx, &opened, id);
        assert_eq!(sent(&opened), [format!("Code {{ gesture: Click {{ target: SearchOption {{ option: \"{option}\" }} }} }}")], "{id}");
    }
    assert_eq!(label(cx, &opened, "code-summary").as_deref(), Some("3 matches in 2 files"));
    assert_eq!(label(cx, &opened, ("code-hit-file", 0usize)).as_deref(), Some("src/a.rs"));
    assert_eq!(is_selected(cx, &opened, ("code-hit", 1usize)), Some(true));
    assert_eq!(label(cx, &opened, ("code-hit", 1usize)).as_deref(), Some("pub struct Dragged {"));
    assert!(!present(cx, &opened, ("code-hit", 2usize)), "a folded file's hits are not drawn");
    click_id(cx, &opened, ("code-hit", 0usize));
    click_id(cx, &opened, ("code-hit-file", 1usize));
    assert_eq!(sent(&opened), ["Code { gesture: Click { target: Match { index: 0 } } }", "Code { gesture: Click { target: HitFile { path: \"src/b.rs\" } } }"]);
}

/// CODE-26: the Alt chords flip an option from the field while it is a search, and only then.
#[gpui_kit::test]
fn the_alt_chords_flip_the_options_from_the_field_only_while_searching(cx: &mut TestAppContext) {
    let opened = with(cx, searching(true));
    press(cx, &opened, "alt-c");
    assert_eq!(sent(&opened), ["Code { gesture: Click { target: SearchOption { option: \"case\" } } }"]);
    says(cx, &opened, jumping());
    press(cx, &opened, "alt-c");
    assert!(sent(&opened).is_empty());
}

/// CODE-52: hits stand only for the query and options as they stand, and the summary says to search again.
#[gpui_kit::test]
fn a_stale_search_has_no_hits_and_says_enter_searches(cx: &mut TestAppContext) {
    let opened = with(cx, searching(false));
    assert!(!present(cx, &opened, ("code-hit-file", 0usize)));
    assert_eq!(label(cx, &opened, "code-summary").as_deref(), Some("Enter searches the working tree."));
}

/// CODE-27: the glob is sent as it is typed, Enter in it searches and Escape puts the search away.
#[gpui_kit::test]
fn the_glob_field_is_sent_as_typed_and_its_keys_search_or_close(cx: &mut TestAppContext) {
    let opened = with(cx, searching(true));
    click_id(cx, &opened, "code-glob");
    type_text(cx, &opened, "*.rs");
    assert_eq!(sent(&opened).last().map(String::as_str), Some("Code { gesture: Glob { glob: \"*.rs\" } }"));
    press(cx, &opened, "enter");
    assert_eq!(sent(&opened), ["Code { gesture: Search { gesture: Choose } }"]);
    press(cx, &opened, "escape");
    assert_eq!(sent(&opened), ["Code { gesture: Search { gesture: Close } }"]);
}
