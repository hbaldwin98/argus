//! The outline column: what the file defines and the changes in it (CODE-45, 46).

use argus_desktop_core::code::stage::CodePage;
use gpui_kit::TestAppContext;

use super::*;
use crate::code::file::tests::{change, reading, symbol};

fn outlined() -> CodePage {
    CodePage {
        symbols: vec![symbol("StatusBar", 10, false, true), symbol("new", 25, true, false)],
        changes_here: vec![change(26, "StatusBar {", true), change(40, "\u{2212} 2 lines removed", false)],
        ..reading(60, 26)
    }
}

/// CODE-45: a row for each definition, its kind in a badge, the one the cursor is in selected, and a dot where it changed.
#[gpui_kit::test]
fn each_definition_is_a_row_with_its_kind_and_a_dot_where_it_changed(cx: &mut TestAppContext) {
    let opened = with(cx, outlined());
    assert_eq!(label(cx, &opened, ("code-symbol", 0usize)).as_deref(), Some("StatusBar"));
    assert_eq!(label(cx, &opened, ("code-symbol-kind", 1usize)).as_deref(), None, "the badge is the row's, not a name of its own");
    assert!(present(cx, &opened, ("code-symbol-changed", 0usize)) && !present(cx, &opened, ("code-symbol-changed", 1usize)));
    assert_eq!(is_selected(cx, &opened, ("code-symbol", 1usize)), Some(true));
    assert_eq!(is_selected(cx, &opened, ("code-symbol", 0usize)), Some(false));
}

/// CODE-45: a row goes to its line, in a definition or a change.
#[gpui_kit::test]
fn a_row_goes_to_the_line_it_stands_for(cx: &mut TestAppContext) {
    let opened = with(cx, outlined());
    click_id(cx, &opened, ("code-symbol", 0usize));
    click_id(cx, &opened, ("code-change", 1usize));
    assert_eq!(sent(&opened), ["Code { gesture: Click { target: Go { line: 10 } } }", "Code { gesture: Click { target: Go { line: 40 } } }"]);
}

/// CODE-45: the changes are a section of their own, only when there are some.
#[gpui_kit::test]
fn the_changes_in_the_file_are_listed_when_there_are_some(cx: &mut TestAppContext) {
    let opened = with(cx, outlined());
    assert_eq!(label(cx, &opened, ("code-change", 0usize)).as_deref(), Some("Line 26"));
    assert_eq!(is_selected(cx, &opened, ("code-change", 0usize)), Some(true), "the cursor is within the run");
    says(cx, &opened, CodePage { changes_here: Vec::new(), ..outlined() });
    assert!(!present(cx, &opened, ("code-change", 0usize)));
}

/// CODE-45: a file that defines nothing the outline knows says so; no file says nothing.
#[gpui_kit::test]
fn a_file_with_no_definitions_says_so_and_no_file_has_no_outline(cx: &mut TestAppContext) {
    let opened = with(cx, reading(5, 1));
    assert_eq!(label(cx, &opened, "code-outline-none").as_deref(), Some("Nothing defined that the outline knows of."));
    says(cx, &opened, page());
    assert!(!present(cx, &opened, "code-outline"), "an outline of no file is not drawn");
}

/// CODE-46: the close button sends the toggle, and the column goes only when the session says so.
#[gpui_kit::test]
fn the_outline_closes_when_the_session_says_it_is_put_away(cx: &mut TestAppContext) {
    let opened = with(cx, outlined());
    click_id(cx, &opened, "code-outline-close");
    assert_eq!(sent(&opened), ["Code { gesture: Click { target: Outline } }"]);
    assert!(present(cx, &opened, "code-outline"), "nothing is predicted");
    says(cx, &opened, CodePage { outline: false, ..outlined() });
    assert!(!present(cx, &opened, "code-outline"));
}
