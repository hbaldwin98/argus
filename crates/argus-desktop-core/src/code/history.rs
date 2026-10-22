//! What an editor can take back: each unit of typing kept as the set that
//! undoes it, and the disk's changes folded past them so an undo never
//! reaches an agent's write (EDITOR-VIM-10: the agent's change is not in
//! the history). The units are the host's to cut; this only keeps them.

use ropey::Rope;

use super::change::ChangeSet;

#[derive(Default)]
pub struct History {
    /// Each set undoes one unit, made on the text that unit made; the last
    /// is the latest.
    undo: Vec<ChangeSet>,
    /// Each set redoes one undone unit; the last is the next.
    redo: Vec<ChangeSet>,
}

impl History {
    /// A unit was made: `inverse` undoes it. Anything undone before is
    /// gone for good.
    pub fn push(&mut self, inverse: ChangeSet) {
        self.undo.push(inverse);
        self.redo.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Takes back the latest unit from `doc`; the change made to it.
    pub fn undo(&mut self, doc: &mut Rope) -> Option<ChangeSet> {
        let set = self.undo.pop()?;
        self.redo.push(set.invert(doc));
        set.apply_in_place(doc);
        Some(set)
    }

    /// Puts back the unit last undone; the change made to `doc`.
    pub fn redo(&mut self, doc: &mut Rope) -> Option<ChangeSet> {
        let set = self.redo.pop()?;
        self.undo.push(set.invert(doc));
        set.apply_in_place(doc);
        Some(set)
    }

    /// `remote` was made on the text now held, by something that is not in
    /// the history: every unit is moved past it, so undoing them leaves it.
    pub fn past(&mut self, remote: &ChangeSet) {
        for stack in [&mut self.undo, &mut self.redo] {
            let mut r = remote.clone();
            for set in stack.iter_mut().rev() {
                let moved = set.map(&r, false);
                r = r.map(set, true);
                *set = moved;
            }
        }
    }
}

#[cfg(test)]
mod tests;
