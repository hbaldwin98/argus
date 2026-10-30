//! The file's header: the name and folder, the version, the facts and the buttons (CODE-40, 8).

use gpui_kit::TestAppContext;

use super::*;
use crate::checkouts::stage::tests::{click_id, label, present};
use crate::code::stage::tests::is_selected;

/// CODE-40: the name and folder, the facts joined by a middle dot, then the counts.
#[gpui_kit::test]
fn the_header_names_the_file_and_says_what_is_known_of_it(cx: &mut TestAppContext) {
    let opened = with(cx, CodePage { added: 4, removed: 1, ..reading(30, 12) });
    assert_eq!(label(cx, &opened, "code-head").as_deref(), Some("src/lib.rs"));
    assert_eq!(label(cx, &opened, "file-head-meta").as_deref(), Some("Ln 12 \u{b7} Rust \u{b7} 30 lines"));
    says(cx, &opened, CodePage { version: Some("a1b2c3d".into()), ..reading(30, 12) });
    assert_eq!(label(cx, &opened, "file-head-meta").as_deref(), Some("a1b2c3d \u{b7} Ln 12 \u{b7} Rust \u{b7} 30 lines"), "the version leads what is read when it is not the working tree");
}

/// CODE-40: Open in editor, and Outline, pressed while the column is up.
#[gpui_kit::test]
fn the_buttons_send_what_their_keys_send(cx: &mut TestAppContext) {
    let opened = with(cx, reading(30, 1));
    assert_eq!(is_selected(cx, &opened, "code-outline-show"), Some(true), "the outline is up, so the toggle is pressed");
    assert!(!present(cx, &opened, "code-back"));
    click_id(cx, &opened, "code-edit");
    assert_eq!(sent(&opened), ["Code { gesture: Action { action: OpenEditor } }"]);
    says(cx, &opened, CodePage { outline: false, ..reading(30, 1) });
    assert_eq!(is_selected(cx, &opened, "code-outline-show"), Some(false));
    click_id(cx, &opened, "code-outline-show");
    assert_eq!(sent(&opened), ["Code { gesture: Click { target: Outline } }"]);
}

/// CODE-8: opened from a dialog, the header says where `B` goes back to, and its button goes.
#[gpui_kit::test]
fn a_stage_opened_from_a_dialog_says_where_back_goes_and_goes_there(cx: &mut TestAppContext) {
    let opened = with(cx, CodePage { back: Some("review \u{b7} lib.rs:4".into()), ..reading(30, 4) });
    assert_eq!(label(cx, &opened, "code-back").as_deref(), Some("Back to review \u{b7} lib.rs:4"));
    click_id(cx, &opened, "code-back");
    assert_eq!(sent(&opened), ["Code { gesture: Click { target: Back } }"]);
}

/// CODE-40: with no file read the header is the way back alone, over the start panel.
#[gpui_kit::test]
fn with_no_file_the_header_is_only_the_way_back(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    assert!(!present(cx, &opened, "code-head"));
    says(cx, &opened, CodePage { back: Some("history".into()), ..page() });
    assert!(present(cx, &opened, "code-back") && present(cx, &opened, "code-start") && !present(cx, &opened, "code-edit"));
}
