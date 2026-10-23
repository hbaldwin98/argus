//! What the stage is before and beside the index, and what the strip and the bar say (BOARD-INDEX-1
//! to 5, 12, 15).

use argus_desktop_core::connection::page::Features;
use gpui_kit::TestAppContext;

use super::*;
use crate::keys::root::tests::hear;

/// BOARD-INDEX-1: with no repository the stage says so.
#[gpui_kit::test]
fn with_no_repository_the_stage_says_to_choose_one(cx: &mut TestAppContext) {
    let opened = on(cx, Features::default());
    assert!(present(cx, &opened, "features-none"));
    assert_eq!(label(cx, &opened, "features-none-title").as_deref(), Some("No repository"));
    assert!(!present(cx, &opened, "feature-index"));
}

/// BOARD-INDEX-2: until the board arrives there are bars and the filter, and no rows.
#[gpui_kit::test]
fn until_the_board_arrives_the_stage_draws_bars_under_the_filter(cx: &mut TestAppContext) {
    let opened = on(cx, Features { repository: "argus".into(), layout: "list", loaded: false, ..Features::default() });
    assert!(present(cx, &opened, "features-loading"));
    assert!(present(cx, &opened, "feature-filter"), "the filter is up while it loads");
    assert!(!present(cx, &opened, "feature-list"));
}

/// BOARD-INDEX-3: a repository with no features says how they are made, and offers a note.
#[gpui_kit::test]
fn a_repository_with_none_says_how_to_make_one_and_its_button_opens_the_note(cx: &mut TestAppContext) {
    let opened = on(cx, Features { repository: "argus".into(), layout: "list", loaded: true, ..Features::default() });
    assert_eq!(label(cx, &opened, "features-nothing-title").as_deref(), Some("No features in argus yet"));
    assert!(present(cx, &opened, "feature-filter"), "the filter stays above it");
    click(cx, &opened, "features-new");
    assert_eq!(sent(&opened), ["Capture { gesture: Open }"]);
}

/// BOARD-INDEX-4, 15: the list or the board by the layout Rust says, and the keys under it with the
/// layout `V` would switch to.
#[gpui_kit::test]
fn the_layout_rust_says_is_drawn_with_the_key_bar_naming_the_other(cx: &mut TestAppContext) {
    let opened = on(cx, list());
    assert!(present(cx, &opened, "feature-list") && !present(cx, &opened, "feature-board"));
    assert_eq!(label(cx, &opened, "feature-keys-V").as_deref(), Some("V board"));
    assert!(!present(cx, &opened, "feature-board-hint"), "the hint is the board's");
    hear(cx, &opened, vec![heard(board())]);
    assert!(present(cx, &opened, "feature-board") && !present(cx, &opened, "feature-list"));
    assert_eq!(label(cx, &opened, "feature-keys-V").as_deref(), Some("V list"));
    assert_eq!(label(cx, &opened, "feature-board-hint").as_deref(), Some("Drag a card to Held to set it aside"));
    for key in ["N", "Enter", "M", "G"] {
        assert!(present(cx, &opened, format!("feature-keys-{key}")), "{key}");
    }
}

/// BOARD-INDEX-5: the radio is the layout Rust says, and a press sends it and changes nothing.
#[gpui_kit::test]
fn the_radio_sends_the_layout_and_shows_the_one_rust_says(cx: &mut TestAppContext) {
    let opened = on(cx, list());
    assert_eq!(selected(cx, &opened, "layout-list"), Some(true));
    assert_eq!(selected(cx, &opened, "layout-board"), Some(false));
    click(cx, &opened, "layout-board");
    assert_eq!(sent(&opened), ["SetLayout { layout: Board }"]);
    assert_eq!(selected(cx, &opened, "layout-list"), Some(true), "nothing is predicted");
}

/// BOARD-INDEX-5: `New feature` opens the note.
#[gpui_kit::test]
fn new_feature_in_the_strip_opens_the_note(cx: &mut TestAppContext) {
    let opened = on(cx, list());
    click(cx, &opened, "new-feature");
    assert_eq!(sent(&opened), ["Capture { gesture: Open }"]);
}

/// BOARD-INDEX-12: the fold toggle is the list's, says which way it goes, and sends the fold.
#[gpui_kit::test]
fn the_fold_toggle_is_on_the_list_only_and_sends_the_fold(cx: &mut TestAppContext) {
    let opened = on(cx, Features { show_folded: false, ..list() });
    assert_eq!(label(cx, &opened, "feature-fold").as_deref(), Some("Show held and archived"));
    click(cx, &opened, "feature-fold");
    assert_eq!(sent(&opened), ["FeatureAction { action: Folded }"]);
    hear(cx, &opened, vec![heard(list())]);
    assert_eq!(label(cx, &opened, "feature-fold").as_deref(), Some("Fold held and archived"));
    hear(cx, &opened, vec![heard(Features { folded: 0, ..list() })]);
    assert!(!present(cx, &opened, "feature-fold"), "nothing is folded");
    hear(cx, &opened, vec![heard(board())]);
    assert!(!present(cx, &opened, "feature-fold"), "the board's Archived column has its own");
}

/// BOARD-INDEX-16: a feature opened is drawn by its own surface, so the index draws nothing.
#[gpui_kit::test]
fn an_opened_feature_replaces_the_index(cx: &mut TestAppContext) {
    let opened = on(cx, list());
    assert!(present(cx, &opened, "feature-index"));
    let mut open = list();
    open.open = Some(open_view());
    hear(cx, &opened, vec![heard(open)]);
    assert!(!present(cx, &opened, "feature-index") && !present(cx, &opened, "index-head"));
}
