//! The keys of a browser cell's page (SHELL-BROWSER-13c, SHELL-BROWSER-15): while the screen has
//! the keys it is a field of the key walk, as a terminal's screen is, but what it sends is the
//! browser's.
//!
//! Core's `key_input` says what a key is to the page: a named key, or one character of text. A
//! digit typed into a form is the page's, never the Browsers stage's. Any other key is the
//! window's (a Ctrl or Meta chord, a bare F-key), so it goes to the session as a key like any
//! other, which runs the window's command and types no byte into a pane that has no program
//! (`typed_to` is none for a browser). This is apart from the terminal's `page_key`, which sends
//! every key to a program.

use argus_desktop_core::browser::key_input::key_input;
use argus_desktop_core::connection::page::Call;
use argus_desktop_core::keys::keymap::leaves_to_window;
use gpui_kit::{App, FocusHandle, Window};

use crate::connection::caller::Caller;
use crate::keys::field::{Fields, Registered};
use crate::keys::layer::{Press, Verdict};

/// What the page does with `press`: the browser's input, the session's key, or (for a command key
/// on a Mac, which is the window's) nothing.
pub fn answer(pane: u64, caller: &Caller, press: &Press, mac: bool) -> Verdict {
    if let Some(input) = key_input(press.page) {
        caller.call(Call::BrowserInput { pane, input });
        return Verdict::Taken;
    }
    if leaves_to_window(press.page, mac) {
        return Verdict::Held;
    }
    caller.call(Call::Key { key: press.page.clone() });
    Verdict::Taken
}

/// Puts the screen in the key walk while it has the keys, ended with the cell.
pub fn register(pane: u64, caller: Caller, focus: FocusHandle, window: &Window, cx: &mut App) -> Registered {
    Fields::of(cx).register(window, focus, move |press, _: &mut Window, _: &mut App| answer(pane, &caller, press, cfg!(target_os = "macos")))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use argus_desktop_core::keys::page_key::PageKey;
    use gpui_kit::Keystroke;

    use super::*;

    fn asked(key: PageKey, mac: bool) -> (Verdict, Vec<String>) {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let seen = calls.clone();
        let caller = Caller::new(move |call| seen.borrow_mut().push(format!("{call:?}")));
        let keystroke = Keystroke::parse("a").unwrap();
        let verdict = answer(7, &caller, &Press { keystroke: &keystroke, page: &key }, mac);
        let said = calls.borrow().clone();
        (verdict, said)
    }

    fn key(name: &str) -> PageKey {
        PageKey { key: name.into(), ..PageKey::default() }
    }

    #[test]
    fn a_digit_is_text_for_the_page_and_not_the_stage() {
        let (verdict, said) = asked(key("6"), false);
        assert_eq!(verdict, Verdict::Taken);
        assert_eq!(said.len(), 1);
        assert!(said[0].contains("BrowserInput") && said[0].contains("Text"), "{said:?}");
    }

    #[test]
    fn a_named_key_is_the_pages() {
        let (_, said) = asked(key("Enter"), false);
        assert!(said[0].contains("Key { key: Enter }"), "{said:?}");
    }

    #[test]
    fn a_control_chord_is_the_windows_and_reaches_the_session_as_a_key() {
        let (verdict, said) = asked(PageKey { ctrl: true, ..key("k") }, false);
        assert_eq!(verdict, Verdict::Taken);
        assert!(said[0].starts_with("Key {"), "{said:?}");
    }

    #[test]
    fn a_command_chord_on_a_mac_is_left_to_the_window() {
        let (verdict, said) = asked(PageKey { meta: true, ..key("q") }, true);
        assert_eq!(verdict, Verdict::Held);
        assert!(said.is_empty());
    }
}
