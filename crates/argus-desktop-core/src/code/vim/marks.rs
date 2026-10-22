//! The places Vim remembers in a text: the marks `m` sets, the jump list
//! `Ctrl-o` and `Ctrl-i` walk, and where the last edit ended. Each is a byte
//! position that follows the text as it is edited, as the library's
//! bookmarks do: an insertion at one leaves it before the new text, and an
//! edit that replaces text with the position strictly inside it takes the
//! position away.

use std::collections::BTreeMap;

use crate::code::change::ChangeSet;

/// The most places the jump list holds.
const JUMPS: usize = 100;

/// Where `pos` of the text before `set` falls in the text after, or none if
/// a replaced run has it strictly inside. At an insertion it stays before
/// the text, as a bookmark made with no `insertLeft` does.
pub fn follow(set: &ChangeSet, pos: usize) -> Option<usize> {
    let mut delta: isize = 0;
    for c in set.changes() {
        if c.from > pos {
            break;
        }
        let (len, ins) = (c.to - c.from, c.insert.len());
        if c.from < pos && c.to > pos {
            return None;
        }
        let start = (c.from as isize + delta) as usize;
        if c.to > pos || (c.to == pos && len == 0) {
            return Some(start);
        }
        delta += ins as isize - len as isize;
    }
    Some((pos as isize + delta) as usize)
}

/// One place in the ring: never used, used and since edited away, or held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Empty,
    Gone,
    At(usize),
}

impl Slot {
    fn place(self) -> Option<usize> {
        match self {
            Slot::At(p) => Some(p),
            _ => None,
        }
    }
}

/// Where the start of a line, held by `Marks::track`, falls after `set`:
/// none if a change replaced it, and after any text put at it.
fn follow_handle(set: &ChangeSet, pos: usize) -> Option<usize> {
    let mut delta: isize = 0;
    for c in set.changes() {
        if c.from > pos {
            break;
        }
        let (len, ins) = (c.to - c.from, c.insert.len());
        if c.from <= pos && pos < c.to {
            return None;
        }
        delta += ins as isize - len as isize;
    }
    Some((pos as isize + delta) as usize)
}

/// The circular list of places the cursor jumped from and to, as the
/// library keeps it: a jump records where the cursor was and where it went,
/// and the pointer walks between them.
#[derive(Debug, Clone)]
pub struct Jumps {
    slots: Vec<Slot>,
    /// Counted from the first slot ever used, so it can run past the ring.
    pointer: isize,
    head: isize,
    tail: isize,
}

impl Default for Jumps {
    fn default() -> Jumps {
        Jumps { slots: vec![Slot::Empty; JUMPS], pointer: -1, head: 0, tail: 0 }
    }
}

impl Jumps {
    fn slot(&self, at: isize) -> usize {
        at.rem_euclid(JUMPS as isize) as usize
    }

    fn next_slot(&mut self, pos: usize) {
        self.pointer += 1;
        let at = self.slot(self.pointer);
        self.slots[at] = Slot::At(pos);
    }

    /// A jump from `old` to `new`.
    pub fn add(&mut self, old: usize, new: usize) {
        let current = if self.pointer < 0 { Slot::Empty } else { self.slots[self.slot(self.pointer)] };
        match current {
            Slot::Empty => self.next_slot(old),
            Slot::At(at) if at != old => self.next_slot(old),
            // The place we were last at is the one we left, or was edited away.
            _ => {}
        }
        self.next_slot(new);
        self.head = self.pointer;
        self.tail = (self.pointer - JUMPS as isize + 1).max(0);
    }

    /// Walks `offset` places, going past any edited away and any that is
    /// where the cursor is; the place reached, or none if it was edited away
    /// or never made.
    pub fn walk(&mut self, offset: isize, cursor: usize) -> Option<usize> {
        self.pointer = (self.pointer + offset).clamp(self.tail, self.head);
        let mut mark = self.slots[self.slot(self.pointer)];
        if mark == Slot::Gone {
            let inc = if offset > 0 { 1 } else { -1 };
            loop {
                self.pointer += inc;
                mark = self.slots[self.slot(self.pointer)];
                if mark.place().is_some_and(|m| m != cursor) {
                    break;
                }
                if !(self.pointer < self.head && self.pointer > self.tail) {
                    break;
                }
            }
        }
        mark.place()
    }

    /// Where a walk of `offset` would land, without walking.
    pub fn peek(&self, offset: isize) -> Option<usize> {
        let mut ahead = self.clone();
        ahead.walk(offset, usize::MAX)
    }

    fn follow(&mut self, set: &ChangeSet) {
        for slot in &mut self.slots {
            if let Slot::At(p) = *slot {
                *slot = follow(set, p).map_or(Slot::Gone, Slot::At);
            }
        }
    }
}

/// The two ends of the last selection, kept for `gv`; no name a person can
/// type.
pub const ANCHOR: char = '\u{1}';
pub const HEAD: char = '\u{2}';

#[derive(Debug, Default, Clone)]
pub struct Marks {
    named: BTreeMap<char, usize>,
    pub jumps: Jumps,
    last_edit: Option<usize>,
    /// Lines held for a command that runs on each of them, `:g`.
    handles: Vec<Option<usize>>,
}

impl Marks {
    /// Whether `name` may be set: a word character, or the ends of the last
    /// selection.
    pub fn settable(name: char) -> bool {
        name == '<' || name == '>' || name.is_ascii_alphanumeric() || name == '_'
    }

    pub fn set(&mut self, name: char, at: usize) {
        if Marks::settable(name) {
            self.named.insert(name, at);
        }
    }

    /// Sets a place under a name no one can type.
    pub fn set_hidden(&mut self, name: char, at: usize) {
        self.named.insert(name, at);
    }

    pub fn delete(&mut self, name: char) {
        self.named.remove(&name);
    }

    /// Where the mark is: `'` and `` ` `` are where the last jump came from
    /// (the start if none), and `.` is where the last edit ended.
    pub fn get(&self, name: char) -> Option<usize> {
        match name {
            '\'' | '`' => Some(self.jumps.peek(-1).unwrap_or(0)),
            '.' => self.last_edit,
            _ => self.named.get(&name).copied(),
        }
    }

    /// The lower case marks, for the `]'` family, in text order.
    pub fn lower(&self) -> Vec<usize> {
        let mut at: Vec<usize> = self.named.iter().filter(|(k, _)| k.is_ascii_lowercase()).map(|(_, v)| *v).collect();
        at.sort_unstable();
        at
    }

    /// The marks set, by name.
    pub fn all(&self) -> impl Iterator<Item = (char, usize)> + '_ {
        self.named.iter().filter(|(k, _)| !k.is_control()).map(|(k, v)| (*k, *v))
    }

    /// Where the last edit ended.
    pub fn last_edit(&self) -> Option<usize> {
        self.last_edit
    }

    /// Holds the line that starts at `at`, for a command that edits the text
    /// under it; `line_at` says where it is after.
    pub fn track(&mut self, at: usize) -> usize {
        self.handles.push(Some(at));
        self.handles.len() - 1
    }

    /// Where the tracked line is now, or none if an edit took it away.
    pub fn line_at(&self, handle: usize) -> Option<usize> {
        self.handles.get(handle).copied().flatten()
    }

    /// The tracked lines are let go.
    pub fn release(&mut self) {
        self.handles.clear();
    }

    /// The text was changed by `set`: every place follows it, and the edit
    /// ended where the last of its runs ends.
    pub fn follow(&mut self, set: &ChangeSet) {
        self.named.retain(|_, at| match follow(set, *at) {
            Some(to) => {
                *at = to;
                true
            }
            None => false,
        });
        self.jumps.follow(set);
        for h in &mut self.handles {
            *h = h.and_then(|p| follow_handle(set, p));
        }
        self.last_edit = self.last_edit.and_then(|at| follow(set, at));
        if let Some(&(_, end)) = set.new_ranges().last() {
            self.last_edit = Some(end);
        }
    }

    /// A change made by another hand, such as the disk's: the places follow
    /// it, but it was no edit of ours.
    pub fn follow_remote(&mut self, set: &ChangeSet) {
        let keep = self.last_edit;
        self.follow(set);
        self.last_edit = keep.and_then(|at| follow(set, at));
    }
}

#[cfg(test)]
mod tests;
