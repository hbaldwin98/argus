//! The dialog's head (SESSIONS-23): its close button is named by what it closes.

use gpui_kit::TestAppContext;

use super::{hear_page, said, sessions};
use crate::keys::root::tests::{click_label, drain, has_label, open};

#[gpui_kit::test]
fn the_close_button_is_labelled_by_what_it_closes_and_asks_to_close_the_dialog(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear_page(cx, &opened, sessions());
    assert!(has_label(cx, &opened, "Close the recent sessions"));
    assert!(!has_label(cx, &opened, "Close"), "the plain word names no sheet");
    drain(&opened);
    click_label(cx, &opened, "Close the recent sessions");
    assert_eq!(said(&opened), ["SessionsAction { action: CloseLayer, row: None }"]);
}
