use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Leader key: Ctrl-Space by default. Some terminals report this as
/// `Char(' ')` + CONTROL, others as a bare `Null` keycode.
pub fn is_leader(key: &KeyEvent) -> bool {
    (key.code == KeyCode::Char(' ') && key.modifiers.contains(KeyModifiers::CONTROL))
        || key.code == KeyCode::Null
}

/// Translate a parsed key event back into the byte sequence a terminal
/// child process expects to read from its pty. Covers the common case
/// (shells, pagers, line editors); exotic function-key / kitty-protocol
/// sequences are not attempted in M1.
pub fn encode_key(key: &KeyEvent) -> Vec<u8> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    let mut base: Vec<u8> = match key.code {
        KeyCode::Char(c) => {
            if ctrl {
                match c.to_ascii_lowercase() {
                    'a'..='z' => vec![c.to_ascii_lowercase() as u8 - b'a' + 1],
                    '[' => vec![0x1b],
                    ']' => vec![0x1d],
                    '\\' => vec![0x1c],
                    '^' => vec![0x1e],
                    '_' => vec![0x1f],
                    '@' | ' ' => vec![0x00],
                    _ => c.to_string().into_bytes(),
                }
            } else {
                let mut buf = [0u8; 4];
                c.encode_utf8(&mut buf).as_bytes().to_vec()
            }
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::BackTab => vec![0x1b, b'[', b'Z'],
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Left => vec![0x1b, b'[', b'D'],
        KeyCode::Right => vec![0x1b, b'[', b'C'],
        KeyCode::Up => vec![0x1b, b'[', b'A'],
        KeyCode::Down => vec![0x1b, b'[', b'B'],
        KeyCode::Home => vec![0x1b, b'[', b'H'],
        KeyCode::End => vec![0x1b, b'[', b'F'],
        KeyCode::PageUp => vec![0x1b, b'[', b'5', b'~'],
        KeyCode::PageDown => vec![0x1b, b'[', b'6', b'~'],
        KeyCode::Delete => vec![0x1b, b'[', b'3', b'~'],
        KeyCode::Insert => vec![0x1b, b'[', b'2', b'~'],
        KeyCode::F(n) => encode_function_key(n),
        _ => Vec::new(),
    };

    if alt && !base.is_empty() {
        let mut out = vec![0x1b];
        out.append(&mut base);
        base = out;
    }
    base
}

fn encode_function_key(n: u8) -> Vec<u8> {
    match n {
        1 => b"\x1bOP".to_vec(),
        2 => b"\x1bOQ".to_vec(),
        3 => b"\x1bOR".to_vec(),
        4 => b"\x1bOS".to_vec(),
        5 => b"\x1b[15~".to_vec(),
        6 => b"\x1b[17~".to_vec(),
        7 => b"\x1b[18~".to_vec(),
        8 => b"\x1b[19~".to_vec(),
        9 => b"\x1b[20~".to_vec(),
        10 => b"\x1b[21~".to_vec(),
        11 => b"\x1b[23~".to_vec(),
        12 => b"\x1b[24~".to_vec(),
        _ => Vec::new(),
    }
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
        assert!(!is_leader(&k(KeyCode::Char(' '))), "plain space types a space");
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
        assert_eq!(encode_key(&ctrl('c')), vec![0x03], "SIGINT must reach the child");
        assert_eq!(encode_key(&ctrl('d')), vec![0x04], "EOF must reach the child");
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
        assert!(encode_key(&alt_unknown).is_empty(), "alt must not resurrect it");
    }
}
