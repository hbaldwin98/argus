//! What a key is to Vim, and the `<C-r>` notation that writes one down.
//! The host sends Vim the keys it has with no Alt or Super held, Shift
//! folded into the character; what Vim does not take comes back unhandled.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Code {
    Char(char),
    Esc,
    Enter,
    Backspace,
    Delete,
    Tab,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    pub code: Code,
    pub ctrl: bool,
}

impl Key {
    pub fn ch(c: char) -> Key {
        Key { code: Code::Char(c), ctrl: false }
    }

    /// Ctrl and `c`; Ctrl-[ is Escape.
    pub fn ctrl(c: char) -> Key {
        if c == '[' {
            return Key::of(Code::Esc);
        }
        Key { code: Code::Char(c.to_ascii_lowercase()), ctrl: true }
    }

    pub fn of(code: Code) -> Key {
        Key { code, ctrl: false }
    }

    /// Whether it leaves insert and visual modes: Escape, Ctrl-[ and Ctrl-c.
    pub fn is_esc(self) -> bool {
        self.code == Code::Esc || self == Key::ctrl('c')
    }

    /// The character of a Ctrl chord.
    pub fn ctrl_char(self) -> Option<char> {
        match self {
            Key { code: Code::Char(c), ctrl: true } => Some(c),
            _ => None,
        }
    }

    /// The character it types, if it types one.
    pub fn typed(self) -> Option<char> {
        match self {
            Key { code: Code::Char(c), ctrl: false } => Some(c),
            _ => None,
        }
    }
}

/// Keys written as Vim writes them: plain characters, and `<Esc>`, `<CR>`,
/// `<BS>`, `<Del>`, `<Tab>`, `<Left>`... `<C-x>`, with `<lt>` for `<`. A
/// `<` that opens nothing known is the character.
pub fn parse(notation: &str) -> Vec<Key> {
    let mut out = Vec::new();
    let mut rest = notation;
    while let Some(c) = rest.chars().next() {
        if c == '<' {
            if let Some(end) = rest.find('>') {
                if let Some(key) = named(&rest[1..end]) {
                    out.push(key);
                    rest = &rest[end + 1..];
                    continue;
                }
            }
        }
        out.push(Key::ch(c));
        rest = &rest[c.len_utf8()..];
    }
    out
}

fn named(name: &str) -> Option<Key> {
    let code = match name.to_ascii_lowercase().as_str() {
        "esc" => Code::Esc,
        "cr" | "enter" | "return" => Code::Enter,
        "bs" => Code::Backspace,
        "del" => Code::Delete,
        "tab" => Code::Tab,
        "left" => Code::Left,
        "right" => Code::Right,
        "up" => Code::Up,
        "down" => Code::Down,
        "home" => Code::Home,
        "end" => Code::End,
        "pageup" => Code::PageUp,
        "pagedown" => Code::PageDown,
        "ins" | "insert" => Code::Insert,
        "lt" => Code::Char('<'),
        "space" => Code::Char(' '),
        other => {
            let mut chars = other.strip_prefix("c-")?.chars();
            let c = chars.next()?;
            return chars.next().is_none().then(|| Key::ctrl(c));
        }
    };
    Some(Key::of(code))
}

#[cfg(test)]
mod tests;
