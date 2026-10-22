//! The filter field: what it sends, what it keeps across snapshots, and how it lets go of the
//! keys (BOARD-INDEX-6 to 10, BOARD-KEYS-6, 9).

use argus_desktop_core::connection::page::Features;
use gpui_kit::TestAppContext;

use super::*;
use crate::keys::root::tests::{asked, hear, press, type_text};

/// BOARD-INDEX-7, BOARD-KEYS-6: `/` takes the field and sends nothing; each letter sends the text
/// as it stands, and a letter the field has is not a key of the stage.
#[gpui_kit::test]
fn the_slash_takes_the_field_and_each_letter_sends_the_text_as_it_stands(cx: &mut TestAppContext) {
    let opened = on(cx, list());
    press(cx, &opened, "/");
    assert!(sent(&opened).is_empty(), "the page handles the slash itself");
    type_text(cx, &opened, "tr");
    assert_eq!(sent(&opened), ["FilterFeatures { filter: \"t\" }", "FilterFeatures { filter: \"tr\" }"]);
    type_text(cx, &opened, "j");
    assert_eq!(sent(&opened), ["FilterFeatures { filter: \"trj\" }"], "j is typed while the field has the keys: BOARD-KEYS-9");
}

/// BOARD-INDEX-8: Enter leaves the field and sends nothing; the filter stays, and the stage has the keys.
#[gpui_kit::test]
fn enter_and_escape_leave_the_field_and_give_the_keys_to_the_list(cx: &mut TestAppContext) {
    let opened = on(cx, list());
    for leave in ["enter", "escape"] {
        press(cx, &opened, "/");
        type_text(cx, &opened, "a");
        sent(&opened);
        press(cx, &opened, leave);
        assert!(sent(&opened).is_empty(), "{leave} sends nothing");
        press(cx, &opened, "j");
        assert_eq!(sent(&opened), ["FeatureAction { action: Next }"], "{leave}: the list has the keys again");
    }
}

/// BOARD-INDEX-9: a word half typed is never overwritten by the echo that is older than the last letter.
#[gpui_kit::test]
fn what_is_being_typed_is_not_overwritten_by_an_older_echo(cx: &mut TestAppContext) {
    let opened = on(cx, list());
    press(cx, &opened, "/");
    type_text(cx, &opened, "abc");
    sent(&opened);
    hear(cx, &opened, vec![heard(Features { filter: "ab".into(), ..list() })]);
    type_text(cx, &opened, "d");
    assert_eq!(sent(&opened), ["FilterFeatures { filter: \"abcd\" }"], "the text and the caret held across the echo");
}

/// BOARD-INDEX-10: a filter Rust changed (a reveal that clears it) replaces the text while the field
/// has not the keys, and sends nothing back.
#[gpui_kit::test]
fn a_filter_rust_cleared_replaces_the_text_and_sends_nothing(cx: &mut TestAppContext) {
    let opened = on(cx, Features { filter: "zz".into(), ..list() });
    hear(cx, &opened, vec![heard(list())]);
    assert!(asked(&opened).is_empty(), "putting the echo into the field is not an edit");
    press(cx, &opened, "/");
    type_text(cx, &opened, "q");
    assert_eq!(sent(&opened), ["FilterFeatures { filter: \"q\" }"], "the field held nothing of the old filter");
}

/// BOARD-INDEX-6: under 1200 px the field folds into the strip's button, which opens it and takes the keys.
#[gpui_kit::test]
fn a_narrow_window_folds_the_field_into_the_strips_button(cx: &mut TestAppContext) {
    let opened = crate::keys::root::tests::open_sized(cx, 1100.0);
    let rail = argus_desktop_core::connection::page::Rail { stage: argus_desktop_core::connection::page::Stage::Feature, ..crate::checkouts::stage::tests::rail() };
    hear(cx, &opened, vec![connected(), Event::Rail(Box::new(rail)), heard(list())]);
    assert!(!present(cx, &opened, "feature-filter"), "the field is folded away");
    assert!(present(cx, &opened, "feature-filter-toggle"));
    click(cx, &opened, "feature-filter-toggle");
    assert!(present(cx, &opened, "feature-filter"));
    type_text(cx, &opened, "a");
    assert_eq!(sent(&opened), ["FilterFeatures { filter: \"a\" }"], "the button took the keys");
    press(cx, &opened, "enter");
    hear(cx, &opened, vec![heard(Features { filter: "a".into(), ..list() })]);
    assert!(present(cx, &opened, "feature-filter"), "a filter with text keeps its field");
}

/// BOARD-INDEX-6: a wide window has no toggle.
#[gpui_kit::test]
fn a_wide_window_has_no_filter_toggle(cx: &mut TestAppContext) {
    let opened = on(cx, list());
    assert!(!present(cx, &opened, "feature-filter-toggle"));
}
