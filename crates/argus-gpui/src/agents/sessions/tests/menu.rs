//! The Filter button's menu (SESSIONS-9, -10, -21): its choices and what each sends, and an Escape
//! that closes the menu alone and leaves the dialog up, though the dialog's layer is listed first.

use gpui_kit::TestAppContext;

use super::{hear_page, said, sessions};
use crate::keys::root::tests::{click_label, drain, has_label, open, overlays, press};
use crate::kit::overlay::kind::Overlay;

#[gpui_kit::test]
fn the_menu_lists_every_then_each_and_choosing_one_sends_it_with_the_other_kept(cx: &mut TestAppContext) {
    let opened = open(cx);
    let mut page = sessions();
    page.checkout = Some("main".into());
    hear_page(cx, &opened, page);
    click_label(cx, &opened, "Filter sessions");
    for label in ["Every harness", "claude", "codex", "Every checkout"] {
        assert!(has_label(cx, &opened, label), "{label}");
    }
    drain(&opened);
    click_label(cx, &opened, "codex");
    assert_eq!(said(&opened), ["FilterSessions { harness: Some(\"codex\"), checkout: Some(\"main\") }"]);
}

#[gpui_kit::test]
fn escape_closes_the_menu_only_and_the_next_escape_asks_to_close_the_dialog(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear_page(cx, &opened, sessions());
    click_label(cx, &opened, "Filter sessions");
    assert!(has_label(cx, &opened, "Every harness"));
    drain(&opened);
    press(cx, &opened, "escape");
    assert!(!has_label(cx, &opened, "Every harness"), "the menu closed");
    assert!(said(&opened).is_empty(), "and the dialog was not asked to: {:?}", said(&opened));
    assert!(overlays(cx).contains(&Overlay::Sessions));
    press(cx, &opened, "escape");
    assert_eq!(said(&opened), ["SessionsAction { action: CloseLayer, row: None }"]);
}

#[gpui_kit::test]
fn a_key_with_a_menu_open_never_reaches_the_dialog(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear_page(cx, &opened, sessions());
    click_label(cx, &opened, "Filter sessions");
    drain(&opened);
    press(cx, &opened, "o");
    press(cx, &opened, "r");
    assert!(said(&opened).is_empty(), "{:?}", said(&opened));
}
