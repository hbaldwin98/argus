//! What a key is to Vim, and the `<C-r>` notation that writes one down.
//! The host sends Vim the keys it has with no Alt or Super held, Shift
//! folded into the character (a key with no character, Space and Backspace,
//! says whether Shift was down); what Vim does not take comes back unhandled.

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
    /// Only Space and Backspace tell: `<S-Space>` is `w` and `<S-BS>` is `b`
    /// in normal and visual mode. A character has Shift in it already.
    pub shift: bool,
}

impl Key {
    pub fn ch(c: char) -> Key {
        Key { code: Code::Char(c), ctrl: false, shift: false }
    }

    /// Ctrl and `c`; Ctrl-[ is Escape.
    pub fn ctrl(c: char) -> Key {
        if c == '[' {
            return Key::of(Code::Esc);
        }
        Key { code: Code::Char(c.to_ascii_lowercase()), ctrl: true, shift: false }
    }

    pub fn of(code: Code) -> Key {
        Key { code, ctrl: false, shift: false }
    }

    /// A key with Shift held, for the two it matters to.
    pub fn shifted(code: Code) -> Key {
        Key { code, ctrl: false, shift: true }
    }

    /// Whether it leaves insert and visual modes: Escape, Ctrl-[ and Ctrl-c.
    pub fn is_esc(self) -> bool {
        self.code == Code::Esc || self == Key::ctrl('c')
    }

    /// The character of a Ctrl chord.
    pub fn ctrl_char(self) -> Option<char> {
        match self {
            Key { code: Code::Char(c), ctrl: true, .. } => Some(c),
            _ => None,
        }
    }

    /// The character it types, if it types one.
    pub fn typed(self) -> Option<char> {
        match self {
            Key { code: Code::Char(c), ctrl: false, .. } => Some(c),
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
    let lower = name.to_ascii_lowercase();
    let code = match lower.as_str() {
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
            if let Some(rest) = other.strip_prefix("s-") {
                return match rest {
                    "space" => Some(Key::shifted(Code::Char(' '))),
                    "bs" => Some(Key::shifted(Code::Backspace)),
                    _ => None,
                };
            }
            let rest = other.strip_prefix("c-")?;
            if rest == "space" {
                return Some(Key::ctrl(' '));
            }
            if rest == "bs" {
                return Some(Key { ctrl: true, ..Key::of(Code::Backspace) });
            }
            let mut chars = rest.chars();
            let c = chars.next()?;
            return chars.next().is_none().then(|| Key::ctrl(c));
        }
    };
    Some(Key::of(code))
}

/// A key written as `parse` reads it, so keys recorded into a register
/// play back as the same keys.
pub fn notation(key: Key) -> String {
    let base = match key.code {
        Code::Char('<') => "lt".to_owned(),
        Code::Char(' ') if key.ctrl || key.shift => "Space".to_owned(),
        Code::Char(c) if !key.ctrl => return c.to_string(),
        Code::Char(c) => c.to_string(),
        Code::Esc => "Esc".into(),
        Code::Enter => "CR".into(),
        Code::Backspace => "BS".into(),
        Code::Delete => "Del".into(),
        Code::Tab => "Tab".into(),
        Code::Left => "Left".into(),
        Code::Right => "Right".into(),
        Code::Up => "Up".into(),
        Code::Down => "Down".into(),
        Code::Home => "Home".into(),
        Code::End => "End".into(),
        Code::PageUp => "PageUp".into(),
        Code::PageDown => "PageDown".into(),
        Code::Insert => "Ins".into(),
    };
    let prefix = match (key.ctrl, key.shift) {
        (true, _) => "C-",
        (false, true) => "S-",
        _ => "",
    };
    format!("<{prefix}{base}>")
}

#[cfg(test)]
mod tests;
