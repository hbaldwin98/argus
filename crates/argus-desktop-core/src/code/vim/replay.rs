//! Keys that stand for other keys: recording what is typed into a register
//! (`q`), playing a register back (`@`), the keys a mapping types and the
//! keys `:normal` is given. Each is fed through the same door as a typed
//! key, so what they do is what typing them does; they differ in that a
//! recording does not hear them, and `.` does not remember what a macro did.

use super::key::{self, Code, Key};
use super::macros::MOST_DEPTH;
use super::maps::{Hit, Map, Mode as MapMode};
use super::{Mode, Shared, Vim};

/// An insert-mode mapping that is half typed: the keys so far, and where
/// the first of them began, so that what they typed can be taken back if
/// the rest completes the mapping.
#[derive(Debug, Default)]
pub struct InsertMap {
    keys: Vec<Key>,
    /// Where the cursor was before the first key, and what the rest of the
    /// line held then, which replace mode wrote over.
    from: Option<(usize, String)>,
}

impl InsertMap {
    pub fn clear(&mut self) {
        self.keys.clear();
        self.from = None;
    }
}

impl Vim {
    /// A typed key while recording: it is added to the register, unless it
    /// is the `q` that ends the recording, which is taken. True if taken.
    pub(super) fn recorded(&mut self, sh: &mut Shared, key: Key) -> bool {
        if sh.macros.playing() {
            return false;
        }
        let at_rest = matches!(self.mode, Mode::Normal | Mode::Visual | Mode::VisualLine | Mode::VisualBlock)
            && self.pending.is_empty();
        let Some((register, text)) = &mut sh.macros.recording else { return false };
        if at_rest && key == Key::ch('q') {
            sh.macros.recording = None;
            return true;
        }
        text.push_str(&key::notation(key));
        let (register, text) = (*register, text.clone());
        sh.regs.set_macro(register, &text);
        false
    }

    /// `q<register>`: the register is emptied and what is typed from here
    /// goes into it.
    pub(super) fn start_recording(&mut self, sh: &mut Shared, register: char) {
        sh.regs.set_macro(register, "");
        sh.macros.recording = Some((register, String::new()));
        sh.macros.latest = Some(register);
    }

    /// `@<register>`, `count` times: `@@` is the last one played or
    /// recorded, and `@:` the last ex command.
    pub(super) fn play(&mut self, sh: &mut Shared, name: char, count: usize) {
        let name = if name == '@' {
            let Some(latest) = sh.macros.latest else { return };
            latest
        } else {
            sh.macros.latest = Some(name);
            name
        };
        for _ in 0..count.max(1) {
            if name == ':' {
                if let Some(line) = sh.regs.get(Some(':')) {
                    self.ex_run(sh, &line.text);
                }
                continue;
            }
            let Some(reg) = sh.regs.get(Some(name)) else { return };
            if sh.macros.depth >= MOST_DEPTH {
                return;
            }
            sh.macros.depth += 1;
            for k in key::parse(&reg.text) {
                self.feed(sh, k, false);
            }
            sh.macros.depth -= 1;
        }
    }

    /// Feeds `keys` as if typed, with mappings applied or not.
    pub(super) fn feed_keys(&mut self, sh: &mut Shared, keys: &[Key], remap: bool) {
        let before = std::mem::replace(&mut self.remap, remap);
        for k in keys {
            self.feed(sh, *k, false);
        }
        self.remap = before;
    }

    /// Types what a mapping stands for. A mapping already being expanded is
    /// not expanded again.
    pub(super) fn expand(&mut self, sh: &mut Shared, map: &Map) {
        if self.stack.contains(&map.id) {
            return;
        }
        self.stack.push(map.id);
        self.feed_keys(sh, &map.rhs, !map.noremap);
        self.stack.pop();
    }

    /// Insert mode, before a key is typed: whether it is part of a mapping.
    /// Keys that begin one are typed as they come, and taken back if it
    /// completes; none if the key is the insert's own.
    pub(super) fn insert_mapped(&mut self, sh: &mut Shared, key: Key) -> Option<bool> {
        if key.code == Code::Esc {
            self.imap.clear();
            return None;
        }
        let mut keys = self.imap.keys.clone();
        keys.push(key);
        match sh.maps.lookup(&keys, MapMode::Insert) {
            Hit::Full(map) => {
                let map = map.clone();
                self.take_back_typed();
                self.imap.clear();
                self.expand(sh, &map);
                Some(true)
            }
            Hit::Partial => {
                if self.imap.from.is_none() {
                    let t = self.buf.text();
                    let rest = t.slice(self.cursor, t.line_end(t.line_of(self.cursor)));
                    self.imap.from = Some((self.cursor, rest));
                }
                self.imap.keys = keys;
                // Typed as any key is.
                Some(self.insert_key_plain(sh, key))
            }
            Hit::None => {
                self.imap.clear();
                None
            }
        }
    }

    /// The mapping's keys typed their characters as they came; take them
    /// back, restoring what replace mode wrote over.
    fn take_back_typed(&mut self) {
        let Some((start, rest)) = self.imap.from.clone() else { return };
        let end = self.cursor;
        if end < start {
            return;
        }
        let typed = self.buf.text().slice(start, end).chars().count();
        let restored = if self.mode == Mode::Replace {
            rest.chars().take(typed).collect::<String>()
        } else {
            String::new()
        };
        self.buf.replace(start, end, &restored);
        self.cursor = start;
        if let Some(i) = &mut self.insert {
            for _ in 0..typed {
                i.text.pop();
            }
        }
    }
}

#[cfg(test)]
mod tests;
