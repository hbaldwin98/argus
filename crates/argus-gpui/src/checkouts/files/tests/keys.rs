//! The filter field and the panel's keys (FILES-3 to 6, 17).

use argus_desktop_core::checkouts::files_page::FilesPage;
use gpui_kit::TestAppContext;

use super::*;
use crate::keys::root::tests::{press, type_text};

/// FILES-3: a panel that opens takes the keys, so the moving keys are the panel's and not the table's.
#[gpui_kit::test]
fn a_panel_that_opens_takes_the_keys_and_its_moving_keys_are_its_own(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    press(cx, &opened, "j");
    assert_eq!(sent(&opened), ["Files { gesture: Action { action: Next } }"], "not CheckoutsAction");
    for (key, action) in [("down", "Next"), ("k", "Previous"), ("up", "Previous"), ("home", "First"), ("end", "Last"), ("enter", "Choose"), ("o", "OpenEditor"), ("l", "OpenFolder"), ("right", "OpenFolder"), ("h", "ShutFolder"), ("left", "ShutFolder")] {
        press(cx, &opened, key);
        assert_eq!(sent(&opened), [format!("Files {{ gesture: Action {{ action: {action} }} }}")], "{key}");
    }
}

/// FILES-3: a redraw of the same checkout does not move the keys, and one opening on another does.
#[gpui_kit::test]
fn a_redraw_keeps_the_keys_where_they_are(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    press(cx, &opened, "/");
    assert!(sent(&opened).is_empty(), "the filter takes the keys and nothing is sent");
    says(cx, &opened, FilesPage { changed: 7, ..page() });
    type_text(cx, &opened, "j");
    assert_eq!(sent(&opened), ["Files { gesture: Filter { filter: \"j\" } }"], "typed in the filter still");
    says(cx, &opened, FilesPage { checkout: 102, ..page() });
    press(cx, &opened, "k");
    assert_eq!(sent(&opened), ["Files { gesture: Action { action: Previous } }"], "another checkout's panel took the keys");
}

/// FILES-4: Escape with the tree focused puts the panel away, by asking for it.
#[gpui_kit::test]
fn escape_in_the_tree_asks_for_the_panel_to_close(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    press(cx, &opened, "escape");
    assert_eq!(sent(&opened), ["Files { gesture: Close }"]);
    assert!(present_panel(cx, &opened), "it is the session's word that puts it away");
}

fn present_panel(cx: &mut TestAppContext, opened: &Opened) -> bool {
    crate::checkouts::stage::tests::present(cx, opened, "files-panel")
}

/// FILES-4, 6: Escape in the filter gives the keys back to the tree and closes nothing.
#[gpui_kit::test]
fn escape_in_the_filter_returns_the_keys_to_the_tree(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    press(cx, &opened, "/");
    press(cx, &opened, "escape");
    assert!(sent(&opened).is_empty(), "nothing closed");
    press(cx, &opened, "j");
    assert_eq!(sent(&opened), ["Files { gesture: Action { action: Next } }"]);
    press(cx, &opened, "/");
    press(cx, &opened, "enter");
    press(cx, &opened, "j");
    assert_eq!(sent(&opened), ["Files { gesture: Action { action: Next } }"], "Enter leaves too");
}

/// FILES-5: what is typed is the filter, edit by edit.
#[gpui_kit::test]
fn typing_sends_the_filter_as_it_stands(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    press(cx, &opened, "/");
    type_text(cx, &opened, "sty");
    assert_eq!(sent(&opened), ["Files { gesture: Filter { filter: \"s\" } }", "Files { gesture: Filter { filter: \"st\" } }", "Files { gesture: Filter { filter: \"sty\" } }"]);
}

/// FILES-5: a `#` is a search of the text and filters no names; Enter sends it once something follows.
#[gpui_kit::test]
fn a_hash_is_a_search_that_filters_nothing_and_enter_sends_it(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    press(cx, &opened, "/");
    type_text(cx, &opened, "#Dragged");
    assert!(sent(&opened).is_empty(), "the names are not filtered by it: the filter stays empty and is not sent again");
    press(cx, &opened, "enter");
    assert_eq!(sent(&opened), ["Files { gesture: Search { query: \"Dragged\" } }"]);
}

/// FILES-5: the text of a search is kept in the field though Rust holds no filter for it.
#[gpui_kit::test]
fn a_typed_search_is_kept_in_the_field_across_the_pages_that_follow(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    press(cx, &opened, "/");
    type_text(cx, &opened, "#ab");
    says(cx, &opened, FilesPage { filter: String::new(), ..page() });
    press(cx, &opened, "enter");
    assert_eq!(sent(&opened), ["Files { gesture: Search { query: \"ab\" } }"], "the field still says #ab");
}

/// FILES-5: the field says what the page's filter is when nobody is typing in it.
#[gpui_kit::test]
fn the_filter_shows_what_the_page_says_while_the_tree_has_the_keys(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    says(cx, &opened, FilesPage { filter: "style".into(), ..page() });
    press(cx, &opened, "/");
    type_text(cx, &opened, "x");
    assert_eq!(sent(&opened), ["Files { gesture: Filter { filter: \"stylex\" } }"], "the field held `style`, so the letter was added to it");
}

/// FILES-6: `/` in the tree takes the filter and sends nothing.
#[gpui_kit::test]
fn a_slash_takes_the_filter_and_sends_nothing(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    press(cx, &opened, "/");
    assert!(sent(&opened).is_empty());
}

/// FILES-4: a key the panel does not bind goes on to the table's layer, which holds it or runs it.
#[gpui_kit::test]
fn a_key_the_panel_does_not_bind_goes_on_to_the_table(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    press(cx, &opened, "p");
    assert_eq!(sent(&opened), ["CheckoutsAction { action: Pull }"]);
}

/// FILES-5: Enter on a `#` with nothing after it searches nothing, and leaves the field as Enter does.
#[gpui_kit::test]
fn enter_on_a_bare_hash_searches_nothing_and_leaves_the_field(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    press(cx, &opened, "/");
    type_text(cx, &opened, "#");
    press(cx, &opened, "enter");
    assert!(sent(&opened).is_empty(), "a blank query is no search");
    press(cx, &opened, "j");
    assert_eq!(sent(&opened), ["Files { gesture: Action { action: Next } }"], "the keys are the tree's again");
}
