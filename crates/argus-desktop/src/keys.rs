//! A key the page reports, as the protocol's key.
//!
//! The page sends what the browser saw — `KeyboardEvent.key`, `.code` and
//! the modifiers — and this decides which protocol key, if any, that is.
//! The bytes are `argus_protocol::keys`'s, so this window and the terminal
//! client type the same thing for the same key.

use argus_protocol::keys::{Key, KeyCode, Mods};
use serde::Deserialize;

/// A `keydown` as the page reports it.
#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PageKey {
    /// `KeyboardEvent.key`: what the key means under the current layout.
    pub key: String,
    /// `KeyboardEvent.code`: which physical key it was.
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub meta: bool,
}

/// The protocol's key for a page key, or `None` for one a pane's child is
/// not sent: a bare modifier, a dead key waiting for its letter, anything
/// held with the command key, which belongs to the window.
pub fn to_key(page: &PageKey) -> Option<Key> {
    if page.meta {
        return None;
    }
    let mods = Mods {
        shift: page.shift,
        alt: page.alt,
        ctrl: page.ctrl,
    };
    let code = match page.key.as_str() {
        "Enter" => KeyCode::Enter,
        // A shifted tab is its own key to a terminal, as crossterm reports it.
        "Tab" if page.shift => KeyCode::BackTab,
        "Tab" => KeyCode::Tab,
        "Backspace" => KeyCode::Backspace,
        "Escape" => KeyCode::Esc,
        "ArrowLeft" => KeyCode::Left,
        "ArrowRight" => KeyCode::Right,
        "ArrowUp" => KeyCode::Up,
        "ArrowDown" => KeyCode::Down,
        "Home" => KeyCode::Home,
        "End" => KeyCode::End,
        "PageUp" => KeyCode::PageUp,
        "PageDown" => KeyCode::PageDown,
        "Delete" => KeyCode::Delete,
        "Insert" => KeyCode::Insert,
        name => match function_key(name) {
            Some(n) => KeyCode::F(n),
            None => KeyCode::Char(character(page)?),
        },
    };
    Some(Key::new(code, mods))
}

/// `F1`…`F24` as their number.
fn function_key(name: &str) -> Option<u8> {
    let n: u8 = name.strip_prefix('F')?.parse().ok()?;
    (1..=24).contains(&n).then_some(n)
}

/// The character a key types. With alt held, macOS's option layer turns a
/// letter into another character entirely (option-f is `ƒ`), and a terminal
/// expects alt-f; so with alt the physical key decides, where it names a
/// letter or a digit.
fn character(page: &PageKey) -> Option<char> {
    if page.alt {
        if let Some(c) = physical(&page.code) {
            return Some(if page.shift { c.to_ascii_uppercase() } else { c });
        }
    }
    let mut chars = page.key.chars();
    let c = chars.next()?;
    // Named keys — `Shift`, `Dead`, `Unidentified`, `CapsLock` — are more
    // than one character long; only a single character is typed.
    chars.next().is_none().then_some(c)
}

fn physical(code: &str) -> Option<char> {
    let c = code
        .strip_prefix("Key")
        .or_else(|| code.strip_prefix("Digit"))?;
    let mut chars = c.chars();
    let c = chars.next()?;
    chars.next().is_none().then(|| c.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(name: &str) -> PageKey {
        PageKey {
            key: name.into(),
            ..PageKey::default()
        }
    }

    fn bytes(page: PageKey) -> Vec<u8> {
        to_key(&page).map(|k| argus_protocol::keys::encode(&k)).unwrap_or_default()
    }

    #[test]
    fn named_dom_keys_map_to_the_protocols_keys() {
        let table: &[(&str, KeyCode)] = &[
            ("Enter", KeyCode::Enter),
            ("Tab", KeyCode::Tab),
            ("Backspace", KeyCode::Backspace),
            ("Escape", KeyCode::Esc),
            ("ArrowUp", KeyCode::Up),
            ("ArrowDown", KeyCode::Down),
            ("ArrowLeft", KeyCode::Left),
            ("ArrowRight", KeyCode::Right),
            ("Home", KeyCode::Home),
            ("End", KeyCode::End),
            ("PageUp", KeyCode::PageUp),
            ("PageDown", KeyCode::PageDown),
            ("Delete", KeyCode::Delete),
            ("Insert", KeyCode::Insert),
            ("F5", KeyCode::F(5)),
            ("F12", KeyCode::F(12)),
        ];
        for (name, code) in table {
            assert_eq!(to_key(&key(name)), Some(Key::new(*code, Mods::NONE)), "{name}");
        }
    }

    #[test]
    fn printable_characters_type_themselves_shifted_or_not() {
        assert_eq!(bytes(key("a")), b"a");
        let shifted = PageKey { shift: true, ..key("A") };
        assert_eq!(bytes(shifted), b"A");
        assert_eq!(bytes(key("é")), "é".as_bytes());
        assert_eq!(bytes(key(" ")), b" ");
    }

    #[test]
    fn ctrl_and_alt_letters_reach_the_child_as_a_terminal_sends_them() {
        let ctrl_c = PageKey { ctrl: true, code: "KeyC".into(), ..key("c") };
        assert_eq!(bytes(ctrl_c), [0x03]);
        let alt_f = PageKey { alt: true, code: "KeyF".into(), ..key("f") };
        assert_eq!(bytes(alt_f), b"\x1bf");
    }

    #[test]
    fn alt_on_a_mac_types_the_letter_not_the_option_layer() {
        let option_f = PageKey { alt: true, code: "KeyF".into(), ..key("ƒ") };
        assert_eq!(bytes(option_f), b"\x1bf");
    }

    #[test]
    fn shift_tab_is_a_back_tab() {
        let back = PageKey { shift: true, ..key("Tab") };
        assert_eq!(bytes(back), b"\x1b[Z");
    }

    #[test]
    fn keys_with_nothing_to_type_send_nothing() {
        for name in ["Shift", "Control", "Alt", "Meta", "CapsLock", "Dead", "Unidentified", "F25"] {
            assert_eq!(to_key(&key(name)), None, "{name}");
        }
        let command_c = PageKey { meta: true, ..key("c") };
        assert_eq!(to_key(&command_c), None, "the command key is the window's");
    }
}
