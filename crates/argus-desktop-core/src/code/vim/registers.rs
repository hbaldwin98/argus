//! What yanks and deletes leave, and what a paste reads: the unnamed
//! register, `0`, `1`-`9`, `a`-`z` (a capital appends), `-` (a small
//! delete), `.` (the last typed text), `_` (nothing) and `+` (the system
//! clipboard). One set is shared by every editor, so a yank in one file is
//! pasted in another (EDITOR-VIM-17); the window holds it.
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
}

impl Reg {
    pub fn new(text: &str, linewise: bool) -> Reg {
        let mut text = text.to_owned();
        if linewise && !text.ends_with('\n') {
            text.push('\n');
        }
        Reg { text, linewise }
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
    clipboard_in: Option<String>,
    clipboard_out: Option<String>,
}

/// A register's name, or none for the unnamed one; `*` is `+`.
fn canon(name: Option<char>) -> Option<char> {
    match name {
        Some('"') | None => None,
        Some('*') => Some('+'),
        other => other,
    }
}

impl Registers {
    /// What the system clipboard holds now, for `"+p` to read.
    pub fn offer_clipboard(&mut self, text: Option<String>) {
        self.clipboard_in = text;
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
            Some('_') => None,
            Some('+') => self.clipboard_in.as_deref().map(|t| Reg::new(t, false)),
            Some(c) if c.is_ascii_alphabetic() => self.named[c.to_ascii_lowercase() as usize - 'a' as usize].clone(),
            Some(_) => None,
        }
    }

    /// Whether `name` is one a command may name.
    pub fn valid(name: char) -> bool {
        matches!(name, '"' | '0'..='9' | '-' | '.' | '_' | '+' | '*') || name.is_ascii_alphabetic()
    }

    /// Writes `reg` to a named register; a capital appends.
    fn put_named(&mut self, name: char, reg: Reg) {
        match name {
            '_' | '.' | '0'..='9' | '-' => {}
            '+' => {
                self.clipboard_out = Some(reg.text.clone());
                self.clipboard_in = Some(reg.text.clone());
                self.unnamed = Some(reg);
            }
            c => {
                let slot = &mut self.named[c.to_ascii_lowercase() as usize - 'a' as usize];
                let reg = match (c.is_ascii_uppercase(), slot.take()) {
                    (true, Some(old)) => append(old, reg),
                    _ => reg,
                };
                *slot = Some(reg.clone());
                self.unnamed = Some(reg);
            }
        }
    }

    /// A yank: to the register named, or to `0`; either way the unnamed
    /// one is what was yanked.
    pub fn yank(&mut self, name: Option<char>, reg: Reg) {
        match canon(name) {
            Some('_') => {}
            Some(n) => self.put_named(n, reg),
            None => {
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
            Some(n) => self.put_named(n, reg),
            None => {
                if reg.linewise || reg.text.contains('\n') || far {
                    self.shifted.rotate_right(1);
                    self.shifted[0] = Some(reg.clone());
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
}

/// `new` after `old`; if either is lines, the result is lines.
fn append(old: Reg, new: Reg) -> Reg {
    if old.linewise || new.linewise {
        Reg { text: old.as_lines() + &new.as_lines(), linewise: true }
    } else {
        Reg { text: old.text + &new.text, linewise: false }
    }
}

#[cfg(test)]
mod tests;
