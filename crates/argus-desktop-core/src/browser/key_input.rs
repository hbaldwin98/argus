//! A key on a browser's focused picture, as the browser takes it: a named
//! key or text typed (SHELL-BROWSER-13c).

use argus_protocol::{BrowserInput, BrowserKey};

use crate::keys::page_key::PageKey;

/// What `key` sends the page, or `None` when it is the window's: a chord
/// with Ctrl or Meta, a bare modifier or F-key, a dead key. Enter,
/// Backspace, Delete, Tab, Escape, the arrows, Home, End, PageUp and
/// PageDown are named keys; any key that is one character is text, with
/// Shift and Alt already in it as the layout made it.
pub fn key_input(key: &PageKey) -> Option<BrowserInput> {
    if key.ctrl || key.meta {
        return None;
    }
    let named = match key.key.as_str() {
        "Enter" => Some(BrowserKey::Enter),
        "Backspace" => Some(BrowserKey::Backspace),
        "Delete" => Some(BrowserKey::Delete),
        "Tab" => Some(BrowserKey::Tab),
        "Escape" => Some(BrowserKey::Escape),
        "ArrowUp" => Some(BrowserKey::ArrowUp),
        "ArrowDown" => Some(BrowserKey::ArrowDown),
        "ArrowLeft" => Some(BrowserKey::ArrowLeft),
        "ArrowRight" => Some(BrowserKey::ArrowRight),
        "Home" => Some(BrowserKey::Home),
        "End" => Some(BrowserKey::End),
        "PageUp" => Some(BrowserKey::PageUp),
        "PageDown" => Some(BrowserKey::PageDown),
        _ => None,
    };
    if let Some(key) = named {
        return Some(BrowserInput::Key { key });
    }
    (key.key.chars().count() == 1).then(|| BrowserInput::Text { text: key.key.clone() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(key: &str) -> PageKey {
        PageKey { key: key.into(), ..PageKey::default() }
    }

    #[test]
    fn named_keys_and_single_characters_go_to_the_page() {
        assert_eq!(key_input(&press("Enter")), Some(BrowserInput::Key { key: BrowserKey::Enter }));
        assert_eq!(key_input(&press("PageDown")), Some(BrowserInput::Key { key: BrowserKey::PageDown }));
        assert_eq!(key_input(&press("6")), Some(BrowserInput::Text { text: "6".into() }), "a digit is text, not the Browsers stage");
        assert_eq!(key_input(&press(" ")), Some(BrowserInput::Text { text: " ".into() }));
        assert_eq!(key_input(&press("é")), Some(BrowserInput::Text { text: "é".into() }));
        assert_eq!(key_input(&PageKey { shift: true, ..press("A") }), Some(BrowserInput::Text { text: "A".into() }));
    }

    #[test]
    fn keys_that_are_not_one_character_or_named_stay_the_windows() {
        for key in ["Shift", "F5", "Dead", "Control", "Unidentified", ""] {
            assert_eq!(key_input(&press(key)), None, "{key:?}");
        }
    }

    mod properties {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            // SHELL-BROWSER-13c: with Ctrl or Meta held nothing is sent, whatever the key.
            #[test]
            fn a_ctrl_or_meta_chord_is_the_windows(key in "\\PC{0,3}", ctrl: bool, alt: bool, shift: bool) {
                for (ctrl, meta) in [(true, false), (false, true), (ctrl, true)] {
                    prop_assert_eq!(key_input(&PageKey { key: key.clone(), ctrl, meta, alt, shift, ..PageKey::default() }), None);
                }
            }

            // SHELL-BROWSER-13c: any key that is one character is its text.
            #[test]
            fn one_character_is_text_typed(c in any::<char>(), alt: bool, shift: bool) {
                let sent = key_input(&PageKey { key: c.to_string(), alt, shift, ..PageKey::default() });
                prop_assert_eq!(sent, Some(BrowserInput::Text { text: c.to_string() }));
            }
        }
    }
}
