//! The bytes a key press is to a pane's child.
//!
//! Every client that types into a pane says what was pressed as a [`Key`]
//! and lets [`encode`] decide the bytes, so a terminal client and a window
//! never disagree about what ctrl-left or F5 sends. Translating a client's
//! own key events into a `Key` is the client's business; deciding *whether*
//! a key goes to a pane at all is too.

/// A key a client asks to type into a pane: what was pressed and which
/// modifiers were held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    pub code: KeyCode,
    pub mods: Mods,
}

impl Key {
    pub const fn new(code: KeyCode, mods: Mods) -> Key {
        Key { code, mods }
    }
}

/// The keys a pane's child has an encoding for. A key not listed here has
/// none, so a client leaves it out rather than mapping it to something near.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    Char(char),
    Enter,
    Tab,
    BackTab,
    Backspace,
    Esc,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Delete,
    Insert,
    /// A function key by number; only F1 to F12 have an encoding.
    F(u8),
}

/// The modifiers a child can be told about. Super, hyper and meta have no
/// place in the xterm encoding, so a client drops them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
}

impl Mods {
    pub const NONE: Mods = Mods { shift: false, alt: false, ctrl: false };
    pub const SHIFT: Mods = Mods { shift: true, ..Mods::NONE };
    pub const ALT: Mods = Mods { alt: true, ..Mods::NONE };
    pub const CTRL: Mods = Mods { ctrl: true, ..Mods::NONE };

    /// Both sets held at once.
    pub const fn with(self, other: Mods) -> Mods {
        Mods {
            shift: self.shift || other.shift,
            alt: self.alt || other.alt,
            ctrl: self.ctrl || other.ctrl,
        }
    }
}

/// The xterm modifier parameter: 1, plus a bit per held modifier. It is
/// what turns `\x1b[C` into `\x1b[1;5C`, and it is the only way a child
/// ever learns that ctrl was down for a key that is not a letter.
fn modifier_param(mods: Mods) -> u8 {
    1 + u8::from(mods.shift) + 2 * u8::from(mods.alt) + 4 * u8::from(mods.ctrl)
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

/// The byte sequence a terminal child process expects to read from its
/// pty for `key`. Covers the common case (shells, pagers, line editors);
/// exotic function-key / kitty-protocol sequences are not attempted. An
/// empty result means the key has no encoding and nothing is sent.
pub fn encode(key: &Key) -> Vec<u8> {
    let Mods { ctrl, alt, .. } = key.mods;
    let param = modifier_param(key.mods);

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

    fn k(code: KeyCode) -> Key {
        Key::new(code, Mods::NONE)
    }

    fn ctrl(c: char) -> Key {
        Key::new(KeyCode::Char(c), Mods::CTRL)
    }

    #[test]
    fn plain_characters_encode_as_themselves() {
        assert_eq!(encode(&k(KeyCode::Char('a'))), b"a");
        assert_eq!(encode(&k(KeyCode::Char('Z'))), b"Z");
        assert_eq!(encode(&k(KeyCode::Char(' '))), b" ");
    }

    #[test]
    fn non_ascii_characters_encode_as_utf8() {
        assert_eq!(encode(&k(KeyCode::Char('é'))), "é".as_bytes());
        assert_eq!(encode(&k(KeyCode::Char('日'))), "日".as_bytes());
    }

    #[test]
    fn control_letters_encode_as_c0() {
        assert_eq!(encode(&ctrl('a')), vec![0x01]);
        assert_eq!(
            encode(&ctrl('c')),
            vec![0x03],
            "SIGINT must reach the child"
        );
        assert_eq!(
            encode(&ctrl('d')),
            vec![0x04],
            "EOF must reach the child"
        );
        assert_eq!(encode(&ctrl('z')), vec![0x1a]);
    }

    #[test]
    fn control_letters_are_case_insensitive() {
        assert_eq!(encode(&ctrl('C')), encode(&ctrl('c')));
    }

    #[test]
    fn control_punctuation_encodes_to_its_c0_pair() {
        assert_eq!(encode(&ctrl('[')), vec![0x1b]);
        assert_eq!(encode(&ctrl(']')), vec![0x1d]);
        assert_eq!(encode(&ctrl('@')), vec![0x00]);
        assert_eq!(encode(&ctrl(' ')), vec![0x00], "ctrl-space is NUL");
    }

    #[test]
    fn named_keys_encode_to_their_usual_sequences() {
        assert_eq!(encode(&k(KeyCode::Enter)), b"\r");
        assert_eq!(encode(&k(KeyCode::Tab)), b"\t");
        assert_eq!(encode(&k(KeyCode::Backspace)), vec![0x7f]);
        assert_eq!(encode(&k(KeyCode::Esc)), vec![0x1b]);
    }

    #[test]
    fn arrows_encode_as_csi_sequences() {
        assert_eq!(encode(&k(KeyCode::Up)), b"\x1b[A");
        assert_eq!(encode(&k(KeyCode::Down)), b"\x1b[B");
        assert_eq!(encode(&k(KeyCode::Right)), b"\x1b[C");
        assert_eq!(encode(&k(KeyCode::Left)), b"\x1b[D");
    }

    #[test]
    fn alt_prefixes_an_escape() {
        let alt_a = Key::new(KeyCode::Char('a'), Mods::ALT);
        assert_eq!(encode(&alt_a), b"\x1ba");
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
            assert_eq!(&encode(&k(*code)), expected, "{code:?}");
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
            assert_eq!(encode(&ctrl(*c)), vec![*expected], "ctrl-{c:?}");
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
            assert_eq!(&encode(&k(KeyCode::F(*n))), expected, "F{n}");
        }
        for n in [0u8, 13, 20, 255] {
            assert!(encode(&k(KeyCode::F(n))).is_empty(), "F{n}");
        }
    }

    #[test]
    fn alt_prefixes_an_escape_onto_the_keys_that_take_one() {
        // Only keys that have no modifier parameter of their own; the
        // named ones say alt inside the sequence instead.
        for code in [KeyCode::Char('a'), KeyCode::Enter, KeyCode::Backspace] {
            let plain = encode(&k(code));
            let alt = encode(&Key::new(code, Mods::ALT));
            assert_eq!(alt.first(), Some(&0x1b), "{code:?}");
            assert_eq!(&alt[1..], &plain[..], "{code:?}");
        }
    }

    fn with(code: KeyCode, mods: Mods) -> Vec<u8> {
        encode(&Key::new(code, mods))
    }

    #[test]
    fn ctrl_arrows_carry_the_modifier_in_the_sequence() {
        // Word-wise movement in every shell line editor. Without the
        // parameter these arrive as bare arrows and move one column.
        assert_eq!(with(KeyCode::Right, Mods::CTRL), b"[1;5C");
        assert_eq!(with(KeyCode::Left, Mods::CTRL), b"[1;5D");
        assert_eq!(with(KeyCode::Up, Mods::CTRL), b"[1;5A");
        assert_eq!(with(KeyCode::Down, Mods::CTRL), b"[1;5B");
    }

    #[test]
    fn every_modifier_combination_has_its_own_parameter() {
        let table: &[(Mods, &[u8])] = &[
            (Mods::NONE, b"[C"),
            (Mods::SHIFT, b"[1;2C"),
            (Mods::ALT, b"[1;3C"),
            (Mods::CTRL, b"[1;5C"),
        ];
        for (mods, expected) in table {
            assert_eq!(&with(KeyCode::Right, *mods), expected, "{mods:?}");
        }
        assert_eq!(
            with(KeyCode::Right, Mods::CTRL.with(Mods::SHIFT)),
            b"[1;6C"
        );
        assert_eq!(
            with(
                KeyCode::Right,
                Mods::CTRL.with(Mods::ALT).with(Mods::SHIFT)
            ),
            b"[1;8C"
        );
    }

    #[test]
    fn modified_home_end_and_tilde_keys_keep_their_own_shape() {
        assert_eq!(with(KeyCode::Home, Mods::CTRL), b"[1;5H");
        assert_eq!(with(KeyCode::End, Mods::CTRL), b"[1;5F");
        assert_eq!(with(KeyCode::Delete, Mods::CTRL), b"[3;5~");
        assert_eq!(with(KeyCode::Insert, Mods::SHIFT), b"[2;2~");
        assert_eq!(with(KeyCode::PageUp, Mods::CTRL), b"[5;5~");
        assert_eq!(with(KeyCode::PageDown, Mods::CTRL), b"[6;5~");
    }

    #[test]
    fn modified_function_keys_move_from_ss3_to_csi() {
        assert_eq!(encode(&k(KeyCode::F(1))), b"OP", "bare stays SS3");
        assert_eq!(with(KeyCode::F(1), Mods::CTRL), b"[1;5P");
        assert_eq!(with(KeyCode::F(4), Mods::SHIFT), b"[1;2S");
        assert_eq!(with(KeyCode::F(5), Mods::CTRL), b"[15;5~");
        assert_eq!(with(KeyCode::F(12), Mods::ALT), b"[24;3~");
    }

    #[test]
    fn ctrl_punctuation_windows_reports_as_chars_reaches_the_child() {
        // These are the pairs a Windows console hands over as an ordinary
        // Char + CONTROL; before the table they were typed literally.
        assert_eq!(encode(&ctrl('/')), vec![0x1f], "undo in readline");
        assert_eq!(encode(&ctrl('?')), vec![0x7f]);
        assert_eq!(encode(&ctrl('2')), vec![0x00]);
        assert_eq!(encode(&ctrl('3')), vec![0x1b]);
        assert_eq!(encode(&ctrl('6')), vec![0x1e]);
        assert_eq!(encode(&ctrl('7')), vec![0x1f]);
        assert_eq!(encode(&ctrl('8')), vec![0x7f]);
    }

    #[test]
    fn ctrl_backspace_deletes_a_word_rather_than_a_character() {
        assert_eq!(encode(&k(KeyCode::Backspace)), vec![0x7f]);
        assert_eq!(
            encode(&Key::new(KeyCode::Backspace, Mods::CTRL)),
            vec![0x08]
        );
    }

    #[test]
    fn function_keys_encode_and_unknown_ones_do_not() {
        assert_eq!(encode(&k(KeyCode::F(1))), b"\x1bOP");
        assert_eq!(encode(&k(KeyCode::F(5))), b"\x1b[15~");
        assert_eq!(encode(&k(KeyCode::F(12))), b"\x1b[24~");
        assert!(encode(&k(KeyCode::F(13))).is_empty());
    }
}
