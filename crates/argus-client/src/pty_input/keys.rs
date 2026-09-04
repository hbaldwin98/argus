//! Turning a key event back into the bytes a pty child expects to read.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Leader key: Ctrl-Space by default. Some terminals report this as
/// `Char(' ')` + CONTROL, others as a bare `Null` keycode.
pub fn is_leader(key: &KeyEvent) -> bool {
    (key.code == KeyCode::Char(' ') && key.modifiers.contains(KeyModifiers::CONTROL))
        || key.code == KeyCode::Null
}

/// The xterm modifier parameter: 1, plus a bit per held modifier. It is
/// what turns `\x1b[C` into `\x1b[1;5C`, and it is the only way a child
/// ever learns that ctrl was down for a key that is not a letter.
fn modifier_param(key: &KeyEvent) -> u8 {
    let m = key.modifiers;
    1 + u8::from(m.contains(KeyModifiers::SHIFT))
        + 2 * u8::from(m.contains(KeyModifiers::ALT))
        + 4 * u8::from(m.contains(KeyModifiers::CONTROL))
}

/// `\x1b[A` unmodified, `\x1b[1;5A` with ctrl down.
fn csi_letter(final_byte: u8, param: u8) -> Vec<u8> {
    if param == 1 {
        vec![0x1b, b'[', final_byte]
    } else {
        format!("\x1b[1;{param}{}", final_byte as char).into_bytes()
    }
}

/// `\x1b[3~` unmodified, `\x1b[3;5~` with ctrl down.
fn csi_tilde(number: u8, param: u8) -> Vec<u8> {
    if param == 1 {
        format!("\x1b[{number}~").into_bytes()
    } else {
        format!("\x1b[{number};{param}~").into_bytes()
    }
}

/// The C0 byte a ctrl-punctuation pair collapses to, for the pairs that
/// have one. Windows reports these as `Char` + CONTROL like any other,
/// so without this table ctrl-/ types a slash and ctrl-\ types a
/// backslash — both of which the child reads as ordinary text.
fn control_byte(c: char) -> Option<u8> {
    Some(match c.to_ascii_lowercase() {
        'a'..='z' => c.to_ascii_lowercase() as u8 - b'a' + 1,
        '[' => 0x1b,
        '\\' => 0x1c,
        ']' => 0x1d,
        '^' | '6' => 0x1e,
        '_' | '/' => 0x1f,
        '@' | ' ' | '2' => 0x00,
        '3' => 0x1b,
        '4' => 0x1c,
        '5' => 0x1d,
        '7' => 0x1f,
        '8' | '?' => 0x7f,
        _ => return None,
    })
}

/// Translate a parsed key event back into the byte sequence a terminal
/// child process expects to read from its pty. Covers the common case
/// (shells, pagers, line editors); exotic function-key / kitty-protocol
/// sequences are not attempted in M1.
pub fn encode_key(key: &KeyEvent) -> Vec<u8> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let param = modifier_param(key);

    // Named keys carry their modifiers in the sequence itself, so they
    // are encoded whole rather than getting the alt escape bolted on
    // afterwards — `\x1b\x1b[C` is not what a shell reads as alt-right.
    let named = match key.code {
        KeyCode::Left => Some(csi_letter(b'D', param)),
        KeyCode::Right => Some(csi_letter(b'C', param)),
        KeyCode::Up => Some(csi_letter(b'A', param)),
        KeyCode::Down => Some(csi_letter(b'B', param)),
        KeyCode::Home => Some(csi_letter(b'H', param)),
        KeyCode::End => Some(csi_letter(b'F', param)),
        KeyCode::PageUp => Some(csi_tilde(5, param)),
        KeyCode::PageDown => Some(csi_tilde(6, param)),
        KeyCode::Delete => Some(csi_tilde(3, param)),
        KeyCode::Insert => Some(csi_tilde(2, param)),
        KeyCode::F(n) => encode_function_key(n, param),
        _ => None,
    };
    if let Some(bytes) = named {
        return bytes;
    }

    let mut base: Vec<u8> = match key.code {
        KeyCode::Char(c) => {
            if ctrl {
                match control_byte(c) {
                    Some(b) => vec![b],
                    None => c.to_string().into_bytes(),
                }
            } else {
                let mut buf = [0u8; 4];
                c.encode_utf8(&mut buf).as_bytes().to_vec()
            }
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::BackTab => vec![0x1b, b'[', b'Z'],
        // Ctrl-backspace is how a terminal asks for "delete the word
        // behind me"; readline and every shell line editor bind 0x08 to
        // it, and without this it arrives as a plain backspace.
        KeyCode::Backspace if ctrl => vec![0x08],
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Esc => vec![0x1b],
        _ => Vec::new(),
    };

    if alt && !base.is_empty() {
        let mut out = vec![0x1b];
        out.append(&mut base);
        base = out;
    }
    base
}

fn encode_function_key(n: u8, param: u8) -> Option<Vec<u8>> {
    // F1-F4 are SS3 when bare and CSI once a modifier joins them; the
    // rest keep their tilde form either way.
    let ss3 = match n {
        1 => Some(b'P'),
        2 => Some(b'Q'),
        3 => Some(b'R'),
        4 => Some(b'S'),
        _ => None,
    };
    if let Some(final_byte) = ss3 {
        return Some(if param == 1 {
            vec![0x1b, b'O', final_byte]
        } else {
            csi_letter(final_byte, param)
        });
    }
    let number = match n {
        5 => 15,
        6 => 17,
        7 => 18,
        8 => 19,
        9 => 20,
        10 => 21,
        11 => 23,
        12 => 24,
        _ => return Some(Vec::new()),
    };
    Some(csi_tilde(number, param))
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
    fn plain_characters_encode_as_themselves() {
        assert_eq!(encode_key(&k(KeyCode::Char('a'))), b"a");
        assert_eq!(encode_key(&k(KeyCode::Char('Z'))), b"Z");
        assert_eq!(encode_key(&k(KeyCode::Char(' '))), b" ");
    }

    #[test]
    fn non_ascii_characters_encode_as_utf8() {
        assert_eq!(encode_key(&k(KeyCode::Char('é'))), "é".as_bytes());
        assert_eq!(encode_key(&k(KeyCode::Char('日'))), "日".as_bytes());
    }

    #[test]
    fn control_letters_encode_as_c0() {
        assert_eq!(encode_key(&ctrl('a')), vec![0x01]);
        assert_eq!(
            encode_key(&ctrl('c')),
            vec![0x03],
            "SIGINT must reach the child"
        );
        assert_eq!(
            encode_key(&ctrl('d')),
            vec![0x04],
            "EOF must reach the child"
        );
        assert_eq!(encode_key(&ctrl('z')), vec![0x1a]);
    }

    #[test]
    fn control_letters_are_case_insensitive() {
        assert_eq!(encode_key(&ctrl('C')), encode_key(&ctrl('c')));
    }

    #[test]
    fn control_punctuation_encodes_to_its_c0_pair() {
        assert_eq!(encode_key(&ctrl('[')), vec![0x1b]);
        assert_eq!(encode_key(&ctrl(']')), vec![0x1d]);
        assert_eq!(encode_key(&ctrl('@')), vec![0x00]);
        assert_eq!(encode_key(&ctrl(' ')), vec![0x00], "ctrl-space is NUL");
    }

    #[test]
    fn named_keys_encode_to_their_usual_sequences() {
        assert_eq!(encode_key(&k(KeyCode::Enter)), b"\r");
        assert_eq!(encode_key(&k(KeyCode::Tab)), b"\t");
        assert_eq!(encode_key(&k(KeyCode::Backspace)), vec![0x7f]);
        assert_eq!(encode_key(&k(KeyCode::Esc)), vec![0x1b]);
    }

    #[test]
    fn arrows_encode_as_csi_sequences() {
        assert_eq!(encode_key(&k(KeyCode::Up)), b"\x1b[A");
        assert_eq!(encode_key(&k(KeyCode::Down)), b"\x1b[B");
        assert_eq!(encode_key(&k(KeyCode::Right)), b"\x1b[C");
        assert_eq!(encode_key(&k(KeyCode::Left)), b"\x1b[D");
    }

    #[test]
    fn alt_prefixes_an_escape() {
        let alt_a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::ALT);
        assert_eq!(encode_key(&alt_a), b"\x1ba");
    }

    #[test]
    fn every_named_key_encodes_to_its_documented_sequence() {
        // The whole table at once, so a key nobody presses by hand while
        // testing cannot quietly stop reaching the child.
        let table: &[(KeyCode, &[u8])] = &[
            (KeyCode::Enter, b"\r"),
            (KeyCode::Tab, b"\t"),
            (KeyCode::BackTab, b"\x1b[Z"),
            (KeyCode::Backspace, &[0x7f]),
            (KeyCode::Esc, &[0x1b]),
            (KeyCode::Left, b"\x1b[D"),
            (KeyCode::Right, b"\x1b[C"),
            (KeyCode::Up, b"\x1b[A"),
            (KeyCode::Down, b"\x1b[B"),
            (KeyCode::Home, b"\x1b[H"),
            (KeyCode::End, b"\x1b[F"),
            (KeyCode::PageUp, b"\x1b[5~"),
            (KeyCode::PageDown, b"\x1b[6~"),
            (KeyCode::Delete, b"\x1b[3~"),
            (KeyCode::Insert, b"\x1b[2~"),
        ];
        for (code, expected) in table {
            assert_eq!(&encode_key(&k(*code)), expected, "{code:?}");
        }
    }

    #[test]
    fn every_control_pair_encodes_to_its_c0_byte() {
        let table: &[(char, u8)] = &[
            ('a', 0x01),
            ('z', 0x1a),
            ('[', 0x1b),
            ('\\', 0x1c),
            (']', 0x1d),
            ('^', 0x1e),
            ('_', 0x1f),
            ('@', 0x00),
            (' ', 0x00),
        ];
        for (c, expected) in table {
            assert_eq!(encode_key(&ctrl(*c)), vec![*expected], "ctrl-{c:?}");
        }
    }

    #[test]
    fn every_function_key_has_a_sequence_and_nothing_past_twelve_does() {
        let table: &[(u8, &[u8])] = &[
            (1, b"\x1bOP"),
            (2, b"\x1bOQ"),
            (3, b"\x1bOR"),
            (4, b"\x1bOS"),
            (5, b"\x1b[15~"),
            (6, b"\x1b[17~"),
            (7, b"\x1b[18~"),
            (8, b"\x1b[19~"),
            (9, b"\x1b[20~"),
            (10, b"\x1b[21~"),
            (11, b"\x1b[23~"),
            (12, b"\x1b[24~"),
        ];
        for (n, expected) in table {
            assert_eq!(&encode_key(&k(KeyCode::F(*n))), expected, "F{n}");
        }
        for n in [0u8, 13, 20, 255] {
            assert!(encode_key(&k(KeyCode::F(n))).is_empty(), "F{n}");
        }
    }

    #[test]
    fn alt_prefixes_an_escape_onto_the_keys_that_take_one() {
        // Only keys that have no modifier parameter of their own; the
        // named ones say alt inside the sequence instead.
        for code in [KeyCode::Char('a'), KeyCode::Enter, KeyCode::Backspace] {
            let plain = encode_key(&k(code));
            let alt = encode_key(&KeyEvent::new(code, KeyModifiers::ALT));
            assert_eq!(alt.first(), Some(&0x1b), "{code:?}");
            assert_eq!(&alt[1..], &plain[..], "{code:?}");
        }
    }

    fn with(code: KeyCode, mods: KeyModifiers) -> Vec<u8> {
        encode_key(&KeyEvent::new(code, mods))
    }

    #[test]
    fn ctrl_arrows_carry_the_modifier_in_the_sequence() {
        // Word-wise movement in every shell line editor. Without the
        // parameter these arrive as bare arrows and move one column.
        assert_eq!(with(KeyCode::Right, KeyModifiers::CONTROL), b"[1;5C");
        assert_eq!(with(KeyCode::Left, KeyModifiers::CONTROL), b"[1;5D");
        assert_eq!(with(KeyCode::Up, KeyModifiers::CONTROL), b"[1;5A");
        assert_eq!(with(KeyCode::Down, KeyModifiers::CONTROL), b"[1;5B");
    }

    #[test]
    fn every_modifier_combination_has_its_own_parameter() {
        let table: &[(KeyModifiers, &[u8])] = &[
            (KeyModifiers::NONE, b"[C"),
            (KeyModifiers::SHIFT, b"[1;2C"),
            (KeyModifiers::ALT, b"[1;3C"),
            (KeyModifiers::CONTROL, b"[1;5C"),
        ];
        for (mods, expected) in table {
            assert_eq!(&with(KeyCode::Right, *mods), expected, "{mods:?}");
        }
        assert_eq!(
            with(KeyCode::Right, KeyModifiers::CONTROL | KeyModifiers::SHIFT),
            b"[1;6C"
        );
        assert_eq!(
            with(
                KeyCode::Right,
                KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT
            ),
            b"[1;8C"
        );
    }

    #[test]
    fn modified_home_end_and_tilde_keys_keep_their_own_shape() {
        assert_eq!(with(KeyCode::Home, KeyModifiers::CONTROL), b"[1;5H");
        assert_eq!(with(KeyCode::End, KeyModifiers::CONTROL), b"[1;5F");
        assert_eq!(with(KeyCode::Delete, KeyModifiers::CONTROL), b"[3;5~");
        assert_eq!(with(KeyCode::Insert, KeyModifiers::SHIFT), b"[2;2~");
        assert_eq!(with(KeyCode::PageUp, KeyModifiers::CONTROL), b"[5;5~");
        assert_eq!(with(KeyCode::PageDown, KeyModifiers::CONTROL), b"[6;5~");
    }

    #[test]
    fn modified_function_keys_move_from_ss3_to_csi() {
        assert_eq!(encode_key(&k(KeyCode::F(1))), b"OP", "bare stays SS3");
        assert_eq!(with(KeyCode::F(1), KeyModifiers::CONTROL), b"[1;5P");
        assert_eq!(with(KeyCode::F(4), KeyModifiers::SHIFT), b"[1;2S");
        assert_eq!(with(KeyCode::F(5), KeyModifiers::CONTROL), b"[15;5~");
        assert_eq!(with(KeyCode::F(12), KeyModifiers::ALT), b"[24;3~");
    }

    #[test]
    fn ctrl_punctuation_windows_reports_as_chars_reaches_the_child() {
        // These are the pairs a Windows console hands over as an ordinary
        // Char + CONTROL; before the table they were typed literally.
        assert_eq!(encode_key(&ctrl('/')), vec![0x1f], "undo in readline");
        assert_eq!(encode_key(&ctrl('?')), vec![0x7f]);
        assert_eq!(encode_key(&ctrl('2')), vec![0x00]);
        assert_eq!(encode_key(&ctrl('3')), vec![0x1b]);
        assert_eq!(encode_key(&ctrl('6')), vec![0x1e]);
        assert_eq!(encode_key(&ctrl('7')), vec![0x1f]);
        assert_eq!(encode_key(&ctrl('8')), vec![0x7f]);
    }

    #[test]
    fn ctrl_backspace_deletes_a_word_rather_than_a_character() {
        assert_eq!(encode_key(&k(KeyCode::Backspace)), vec![0x7f]);
        assert_eq!(
            encode_key(&KeyEvent::new(KeyCode::Backspace, KeyModifiers::CONTROL)),
            vec![0x08]
        );
    }

    #[test]
    fn function_keys_encode_and_unknown_ones_do_not() {
        assert_eq!(encode_key(&k(KeyCode::F(1))), b"\x1bOP");
        assert_eq!(encode_key(&k(KeyCode::F(5))), b"\x1b[15~");
        assert_eq!(encode_key(&k(KeyCode::F(12))), b"\x1b[24~");
        assert!(encode_key(&k(KeyCode::F(13))).is_empty());
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
    }
}
