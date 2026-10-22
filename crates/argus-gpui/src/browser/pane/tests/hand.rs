//! Where the keys go when the address field lets them go (SHELL-BROWSER-8): to the window, never to
//! the page.

use gpui_kit::TestAppContext;

use super::*;

#[gpui_kit::test]
fn enter_and_escape_in_the_address_only_blur_it_so_the_next_key_is_the_windows_not_the_pages(cx: &mut TestAppContext) {
    for leave in ["enter", "escape"] {
        let opened = opened(cx, 1, browser());
        click(cx, &opened, "browser-address-105");
        type_text(cx, &opened, "q.test");
        opened.calls.borrow_mut().clear();
        press(cx, &opened, leave);
        opened.calls.borrow_mut().clear();
        press(cx, &opened, "6");
        assert_eq!(asked(&opened), ["Command { action: Browsers }"], "{leave}: the digit is the window's stage key and no text for the page");
    }
}
