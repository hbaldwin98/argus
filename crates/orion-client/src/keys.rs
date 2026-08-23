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
