//! A terminal key event as the key a pane's child is sent, and the leader
//! chord that is never sent at all.
//!
//! The bytes themselves are `argus_protocol::keys`'s to decide, so this
//! client and the desktop window type the same thing for the same key.

use argus_protocol::keys::{self, Key, Mods};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Leader key: Ctrl-Space by default. Some terminals report this as
/// `Char(' ')` + CONTROL, others as a bare `Null` keycode.
pub fn is_leader(key: &KeyEvent) -> bool {
    (key.code == KeyCode::Char(' ') && key.modifiers.contains(KeyModifiers::CONTROL))
        || key.code == KeyCode::Null
}

/// The bytes a pane's child reads for `key`; empty for a key the protocol
/// has no encoding for, which the caller takes as nothing to send.
pub fn encode_key(key: &KeyEvent) -> Vec<u8> {
    to_key(key).map(|key| keys::encode(&key)).unwrap_or_default()
}

/// The protocol's key for a crossterm event, or `None` for a key a child
/// has no encoding for (caps lock, media keys, a bare modifier).
fn to_key(key: &KeyEvent) -> Option<Key> {
    let code = match key.code {
        KeyCode::Char(c) => keys::KeyCode::Char(c),
        KeyCode::Enter => keys::KeyCode::Enter,
        KeyCode::Tab => keys::KeyCode::Tab,
        KeyCode::BackTab => keys::KeyCode::BackTab,
        KeyCode::Backspace => keys::KeyCode::Backspace,
        KeyCode::Esc => keys::KeyCode::Esc,
        KeyCode::Left => keys::KeyCode::Left,
        KeyCode::Right => keys::KeyCode::Right,
        KeyCode::Up => keys::KeyCode::Up,
        KeyCode::Down => keys::KeyCode::Down,
        KeyCode::Home => keys::KeyCode::Home,
        KeyCode::End => keys::KeyCode::End,
        KeyCode::PageUp => keys::KeyCode::PageUp,
        KeyCode::PageDown => keys::KeyCode::PageDown,
        KeyCode::Delete => keys::KeyCode::Delete,
        KeyCode::Insert => keys::KeyCode::Insert,
        KeyCode::F(n) => keys::KeyCode::F(n),
        _ => return None,
    };
    let held = key.modifiers;
    let mods = Mods {
        shift: held.contains(KeyModifiers::SHIFT),
        alt: held.contains(KeyModifiers::ALT),
        ctrl: held.contains(KeyModifiers::CONTROL),
    };
    Some(Key::new(code, mods))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    #[test]
    fn leader_is_recognized_in_both_terminal_dialects() {
        assert!(is_leader(&ctrl(' ')), "Char(' ')+CONTROL form");
        assert!(is_leader(&k(KeyCode::Null)), "bare Null form");
    }

    #[test]
    fn ordinary_keys_are_not_the_leader() {
        assert!(
            !is_leader(&k(KeyCode::Char(' '))),
            "plain space types a space"
        );
        assert!(!is_leader(&ctrl('a')));
        assert!(!is_leader(&k(KeyCode::Esc)));
    }

    #[test]
    fn crossterm_keys_type_the_bytes_they_did_before_the_encoder_moved() {
        // One row for every key code and modifier the encoder's own tests
        // cover, so a code or modifier dropped in the mapping shows up
        // here; the encoding itself is tested beside it in the protocol.
        let none = KeyModifiers::NONE;
        let ctrl = KeyModifiers::CONTROL;
        let alt = KeyModifiers::ALT;
        let shift = KeyModifiers::SHIFT;
        let table: &[(KeyCode, KeyModifiers, &[u8])] = &[
            (KeyCode::Char('a'), none, b"a"),
            (KeyCode::Char('é'), none, "é".as_bytes()),
            (KeyCode::Char('c'), ctrl, &[0x03]),
            (KeyCode::Char('/'), ctrl, &[0x1f]),
            (KeyCode::Char('a'), alt, b"\x1ba"),
            (KeyCode::Enter, none, b"\r"),
            (KeyCode::Enter, alt, b"\x1b\r"),
            (KeyCode::Tab, none, b"\t"),
            (KeyCode::BackTab, shift, b"\x1b[Z"),
            (KeyCode::Backspace, none, &[0x7f]),
            (KeyCode::Backspace, ctrl, &[0x08]),
            (KeyCode::Esc, none, &[0x1b]),
            (KeyCode::Left, none, b"\x1b[D"),
            (KeyCode::Right, ctrl, b"\x1b[1;5C"),
            (KeyCode::Right, shift, b"\x1b[1;2C"),
            (KeyCode::Right, alt, b"\x1b[1;3C"),
            (KeyCode::Right, ctrl | alt | shift, b"\x1b[1;8C"),
            (KeyCode::Up, none, b"\x1b[A"),
            (KeyCode::Down, none, b"\x1b[B"),
            (KeyCode::Home, ctrl, b"\x1b[1;5H"),
            (KeyCode::End, none, b"\x1b[F"),
            (KeyCode::PageUp, none, b"\x1b[5~"),
            (KeyCode::PageDown, ctrl, b"\x1b[6;5~"),
            (KeyCode::Delete, none, b"\x1b[3~"),
            (KeyCode::Insert, shift, b"\x1b[2;2~"),
            (KeyCode::F(1), none, b"\x1bOP"),
            (KeyCode::F(1), ctrl, b"\x1b[1;5P"),
            (KeyCode::F(12), alt, b"\x1b[24;3~"),
            (KeyCode::F(13), none, b""),
        ];
        for (code, mods, expected) in table {
            let event = KeyEvent::new(*code, *mods);
            assert_eq!(&encode_key(&event), expected, "{code:?} {mods:?}");
        }
    }

    #[test]
    fn unhandled_keys_encode_to_nothing_so_nothing_is_sent() {
        // `App::on_key_pane_content` only sends when the encoding is
        // non-empty — an unmapped key must not send a stray empty Input.
        assert!(encode_key(&k(KeyCode::CapsLock)).is_empty());
        let alt_unknown = KeyEvent::new(KeyCode::CapsLock, KeyModifiers::ALT);
        assert!(
            encode_key(&alt_unknown).is_empty(),
            "alt must not resurrect it"
        );
        assert!(encode_key(&k(KeyCode::Null)).is_empty(), "the leader is never typed");
    }
}
