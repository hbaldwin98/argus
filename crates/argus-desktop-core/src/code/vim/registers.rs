//! What yanks and deletes leave, and what a paste reads: the unnamed
//! register, `0`, `1`-`9`, `a`-`z` (a capital appends), `-` (a small
//! delete), `.` (the last typed text), `:` (the last ex command), `/` (the
//! last search), `_` (nothing) and `+` (the system clipboard). One set is
//! shared by every editor, so a yank in one file is pasted in another
//! (EDITOR-VIM-17); the window holds it. A recorded macro is the keys it
//! was typed with, written into a register as text.
//!
//! The clipboard is the host's: it offers what it holds with `offer_clipboard`
//! before keys are fed, and writes out what `take_clipboard_write` hands
//! back, through the one path every copy takes (EDITOR-VIM-23). The unnamed
//! register never syncs to it.

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reg {
    pub text: String,
    /// Whole lines: `text` then ends in a line break, and a paste puts it
    /// on lines of its own.
    pub linewise: bool,
    /// A rectangle of a visual block, one line of `text` per row: a paste
    /// puts each row under the one above it.
    pub blockwise: bool,
}

impl Reg {
    pub fn new(text: &str, linewise: bool) -> Reg {
        let mut text = text.to_owned();
        if linewise && !text.ends_with('\n') {
            text.push('\n');
        }
        Reg { text, linewise, blockwise: false }
    }

    /// The rows of a block, joined by line breaks.
    pub fn block(rows: &[String]) -> Reg {
        Reg { text: rows.join("\n"), linewise: false, blockwise: true }
    }

    fn as_lines(&self) -> String {
        let mut t = self.text.clone();
        if !t.ends_with('\n') {
            t.push('\n');
        }
        t
    }
}

#[derive(Debug, Default)]
pub struct Registers {
    unnamed: Option<Reg>,
    yanked: Option<Reg>,
    shifted: [Option<Reg>; 9],
    small: Option<Reg>,
    named: [Option<Reg>; 26],
    inserted: Option<Reg>,
    command: Option<Reg>,
    search: Option<Reg>,
    /// `_`, and the other word characters a macro may be recorded into.
    other: [Option<Reg>; 1],
    clipboard_in: Option<String>,
    clipboard_out: Option<String>,
}

/// A register's name, or none for the unnamed one. `*` is no register: the
/// library takes it for the unnamed one.
fn canon(name: Option<char>) -> Option<char> {
    match name {
        Some('"' | '*') | None => None,
        other => other,
    }
}

impl Registers {
    /// What the system clipboard holds now, for `"+p` to read.
    pub fn offer_clipboard(&mut self, text: Option<String>) {
        self.clipboard_in = text.map(|t| crate::code::sync::lf(&t));
    }

    /// What `"+y` or `"+d` put on the clipboard since the last call.
    pub fn take_clipboard_write(&mut self) -> Option<String> {
        self.clipboard_out.take()
    }

    pub fn get(&self, name: Option<char>) -> Option<Reg> {
        match canon(name) {
            None => self.unnamed.clone(),
            Some('0') => self.yanked.clone(),
            Some(c @ '1'..='9') => self.shifted[c as usize - '1' as usize].clone(),
            Some('-') => self.small.clone(),
            Some('.') => self.inserted.clone(),
            Some(':') => self.command.clone(),
            Some('/') => self.search.clone(),
            Some('_') => None,
            Some('+') => self.clipboard_in.as_deref().map(|t| Reg::new(t, false)),
            Some(c) if c.is_ascii_alphabetic() => self.named[c.to_ascii_lowercase() as usize - 'a' as usize].clone(),
            Some(_) => None,
        }
    }

    /// Whether `name` is one a command may name: the punctuation ones and
    /// every word character.
    pub fn valid(name: char) -> bool {
        matches!(name, '"' | '-' | '.' | ':' | '_' | '/' | '+') || name.is_ascii_alphanumeric()
    }

    /// Writes `reg` to a named register; a capital appends.
    fn put_named(&mut self, name: char, reg: Reg) {
        match name {
            '_' | '.' | ':' | '/' => {}
            '0' => {
                self.yanked = Some(reg.clone());
                self.unnamed = Some(reg);
            }
            c @ '1'..='9' => {
                self.shifted[c as usize - '1' as usize] = Some(reg.clone());
                self.unnamed = Some(reg);
            }
            '-' => {
                self.small = Some(reg.clone());
                self.unnamed = Some(reg);
            }
            '+' => {
                self.clipboard_out = Some(reg.text.clone());
                self.clipboard_in = Some(reg.text.clone());
                self.unnamed = Some(reg);
            }
            c if c.is_ascii_alphabetic() => {
                let slot = &mut self.named[c.to_ascii_lowercase() as usize - 'a' as usize];
                let reg = match (c.is_ascii_uppercase(), slot.take()) {
                    (true, Some(old)) => append(old, reg),
                    _ => reg,
                };
                *slot = Some(reg.clone());
                self.unnamed = Some(reg);
            }
            _ => {}
        }
    }

    /// A yank: to the register named, or to `0`; either way the unnamed
    /// one is what was yanked.
    pub fn yank(&mut self, name: Option<char>, reg: Reg) {
        match canon(name) {
            Some('_') => {}
            Some(n) if Registers::valid(n) => self.put_named(n, reg),
            _ => {
                self.yanked = Some(reg.clone());
                self.unnamed = Some(reg);
            }
        }
    }

    /// A delete or change. Named, it goes there. Unnamed, text of a line or
    /// more shifts through `1`-`9` and a smaller one is the small-delete
    /// register `-`; `far` is a motion Vim counts as a line's worth whatever
    /// it spans (`%`, `{`, `}`).
    pub fn delete(&mut self, name: Option<char>, reg: Reg, far: bool) {
        match canon(name) {
            Some('_') => {}
            Some(n) if Registers::valid(n) => self.put_named(n, reg),
            _ => {
                if reg.linewise || reg.text.contains('\n') || far {
                    self.shifted.rotate_right(1);
                    self.shifted[0] = Some(Reg { blockwise: false, ..reg.clone() });
                } else {
                    self.small = Some(reg.clone());
                }
                self.unnamed = Some(reg);
            }
        }
    }

    /// The text of the insert just made, for `".`.
    pub fn typed(&mut self, text: String) {
        self.inserted = Some(Reg::new(&text, false));
    }

    /// The ex command just run, for `":` and `@:`.
    pub fn ran(&mut self, line: &str) {
        self.command = Some(Reg::new(line, false));
    }

    /// The pattern last searched for, for `"/`.
    pub fn searched(&mut self, pattern: &str) {
        self.search = Some(Reg::new(pattern, false));
    }

    /// The unnamed register made to hold `reg`, as a paste over a selection
    /// leaves what it replaced.
    pub fn set_unnamed(&mut self, reg: Reg) {
        self.unnamed = Some(reg);
    }

    /// A macro: the register `name` is emptied to record into it, and
    /// `text` is what a recording left there. Capitals and lower case are
    /// one register.
    pub fn set_macro(&mut self, name: char, text: &str) {
        let reg = Reg::new(text, false);
        match name {
            '0' => self.yanked = Some(reg),
            c @ '1'..='9' => self.shifted[c as usize - '1' as usize] = Some(reg),
            c if c.is_ascii_alphabetic() => self.named[c.to_ascii_lowercase() as usize - 'a' as usize] = Some(reg),
            '_' => self.other[0] = Some(reg),
            _ => {}
        }
    }

    /// What `:registers` lists: each register that holds text, the numbered
    /// ones first as the library's object lists them.
    pub fn listing(&self, names: Option<&str>) -> String {
        let mut out = String::from("----------Registers----------\n\n");
        let all = ('0'..='9').chain(['"', '.', ':', '/', '+', '-']).chain('a'..='z');
        match names {
            None => {
                for n in all {
                    let text = self.get(Some(n)).map(|r| r.text).unwrap_or_default();
                    let text = if n == '"' { self.get(None).map(|r| r.text).unwrap_or_default() } else { text };
                    if !text.is_empty() {
                        out.push_str(&format!("\"{n}    {text}\n"));
                    }
                }
            }
            Some(names) => {
                for n in names.chars().filter(|c| !c.is_whitespace() && Registers::valid(*c)) {
                    let reg = if n == '"' { self.get(None) } else { self.get(Some(n)) };
                    out.push_str(&format!("\"{n}    {}\n", reg.map(|r| r.text).unwrap_or_default()));
                }
            }
        }
        out
    }
}

/// `new` after `old`; if either is lines, the result is lines.
fn append(old: Reg, new: Reg) -> Reg {
    if old.linewise || new.linewise {
        Reg { text: old.as_lines() + &new.as_lines(), linewise: true, blockwise: false }
    } else {
        Reg { text: old.text + &new.text, linewise: false, blockwise: false }
    }
}

#[cfg(test)]
mod tests;
