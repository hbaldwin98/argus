//! The text Vim edits, with its undo: every edit lands in the rope at once,
//! is gathered into the unit that is open, and is told to the host as one
//! change set per key. A unit is what `u` takes back — a command, or an
//! insert from `i` to Escape.

use ropey::Rope;

use crate::code::change::{Change, ChangeSet};
use crate::code::history::History;

use super::text::Text;

struct Unit {
    /// The text before the unit's first edit.
    base: Rope,
    set: ChangeSet,
}

pub struct Buffer {
    doc: Rope,
    history: History,
    unit: Option<Unit>,
    /// Everything edited since the host last took it.
    step: Option<ChangeSet>,
    read_only: bool,
}

impl Buffer {
    pub fn new(text: &str) -> Buffer {
        Buffer {
            doc: Rope::from_str(&crate::code::sync::lf(text)),
            history: History::default(),
            unit: None,
            step: None,
            read_only: false,
        }
    }

    pub fn doc(&self) -> &Rope {
        &self.doc
    }

    pub fn text(&self) -> Text<'_> {
        Text(&self.doc)
    }

    pub fn read_only(&self) -> bool {
        self.read_only
    }

    pub fn set_read_only(&mut self, on: bool) {
        self.read_only = on;
    }

    /// Makes `changes`, sorted and apart, in the text as it stands; nothing
    /// if the file is read-only.
    pub fn edit(&mut self, changes: &[Change]) {
        if self.read_only || changes.is_empty() {
            return;
        }
        let set = ChangeSet::of(changes, self.doc.len()).expect("Vim edits fit the text");
        self.apply(&set);
    }

    pub fn replace(&mut self, from: usize, to: usize, insert: &str) {
        self.edit(&[Change { from, to, insert: insert.to_owned() }]);
    }

    fn apply(&mut self, set: &ChangeSet) {
        if set.is_empty() {
            return;
        }
        match &mut self.unit {
            Some(unit) => unit.set = unit.set.compose(set),
            None => self.unit = Some(Unit { base: self.doc.clone(), set: set.clone() }),
        }
        self.step = Some(match self.step.take() {
            Some(step) => step.compose(set),
            None => set.clone(),
        });
        set.apply_in_place(&mut self.doc);
    }

    /// Closes the open unit, so the next edit begins another.
    pub fn commit(&mut self) {
        if let Some(unit) = self.unit.take() {
            if !unit.set.is_empty() {
                self.history.push(unit.set.invert(&unit.base));
            }
        }
    }

    pub fn open(&self) -> bool {
        self.unit.is_some()
    }

    /// Takes back the latest unit; the change made.
    pub fn undo(&mut self) -> Option<ChangeSet> {
        self.commit();
        let set = self.history.undo(&mut self.doc)?;
        self.told(&set);
        Some(set)
    }

    pub fn redo(&mut self) -> Option<ChangeSet> {
        self.commit();
        let set = self.history.redo(&mut self.doc)?;
        self.told(&set);
        Some(set)
    }

    fn told(&mut self, set: &ChangeSet) {
        self.step = Some(match self.step.take() {
            Some(step) => step.compose(set),
            None => set.clone(),
        });
    }

    /// What was edited since the last call, as one change set made on the
    /// text then; the host passes it on to the sync.
    pub fn take_step(&mut self) -> Option<ChangeSet> {
        self.step.take().filter(|s| !s.is_empty())
    }

    /// `remote` was made on the text now held by something other than Vim:
    /// the disk, or the host. It is taken in without a unit and the history
    /// is moved past it.
    pub fn past(&mut self, remote: &ChangeSet) {
        assert!(self.step.is_none(), "the step is taken after every key, before anything else is told");
        self.commit();
        self.history.past(remote);
        remote.apply_in_place(&mut self.doc);
    }
}

#[cfg(test)]
mod tests;
