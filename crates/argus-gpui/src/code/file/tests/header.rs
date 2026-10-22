//! The file's header: the name and folder, the version, the facts and the buttons (CODE-40, 8).

use gpui_kit::TestAppContext;

use super::*;
use crate::checkouts::stage::tests::{click_id, label, present};

/// CODE-40: the name and folder, the facts joined by a middle dot, then the counts.
#[gpui_kit::test]
fn the_header_names_the_file_and_says_what_is_known_of_it(cx: &mut TestAppContext) {
    let opened = with(cx, CodePage { added: 4, removed: 1, ..reading(30, 12) });
    assert_eq!(label(cx, &opened, "code-path").as_deref(), Some("src/lib.rs"));
    assert_eq!(label(cx, &opened, "code-facts").as_deref(), Some("Ln 12 \u{b7} Rust \u{b7} 30 lines \u{b7} +4 \u{2212}1"));
    assert!(!present(cx, &opened, "code-version"), "the working tree is not named");
    says(cx, &opened, CodePage { version: Some("a1b2c3d".into()), ..reading(30, 12) });
    assert_eq!(label(cx, &opened, "code-version").as_deref(), Some("a1b2c3d"));
}

/// CODE-40: Open in editor, and Outline only while the outline is put away.
#[gpui_kit::test]
fn the_buttons_send_what_their_keys_send(cx: &mut TestAppContext) {
    let opened = with(cx, reading(30, 1));
    assert!(!present(cx, &opened, "code-outline-show") && !present(cx, &opened, "code-back"));
    click_id(cx, &opened, "code-edit");
    assert_eq!(sent(&opened), ["Code { gesture: Action { action: OpenEditor } }"]);
    says(cx, &opened, CodePage { outline: false, ..reading(30, 1) });
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
