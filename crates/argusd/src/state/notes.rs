//! Notes: reading them, writing them, and counting what is in them.
//!
//! The daemon's job here is translation and arbitration. Clients speak in
//! ids, which are handed out fresh on every start; the store speaks in
//! names and paths, which are not. Everything in this module exists on one
//! side of that line or the other, and nothing else in the process has to
//! know both.

use std::collections::HashMap;

use argus_protocol::{Note, NoteCounts, NoteTarget, TodoState, MAX_NOTE_BYTES};

use super::*;
use crate::store::NoteKey;

/// A note's counts and whether it exists at all, for one tree row.
pub(super) type NoteSummary = (NoteCounts, bool);

impl Daemon {
    /// What the client's ids refer to, as the store files it.
    fn note_key(&self, target: NoteTarget) -> anyhow::Result<NoteKey> {
        let inner = self.inner.lock().unwrap();
        match target {
            NoteTarget::Project(id) => inner
                .projects
                .iter()
                .find(|p| p.id == id)
                .map(|p| NoteKey::Project(p.name.clone()))
                .ok_or_else(|| anyhow::anyhow!("no such project")),
            NoteTarget::Checkout(id) => find_checkout_ref(&inner.projects, id)
                .map(|c| NoteKey::checkout(&c.path))
                .ok_or_else(|| anyhow::anyhow!("no such checkout")),
        }
    }

    /// A note that has never been written reads as an empty one rather than
    /// as an error: opening a note that does not exist yet is how a note
    /// comes to exist, and the client should not need a second code path
    /// for it.
    pub fn note(&self, target: NoteTarget) -> anyhow::Result<Note> {
        let key = self.note_key(target)?;
        let body = self.store.note(&key)?.unwrap_or_default();
        Ok(Note::new(target, body))
    }

    pub fn set_note(&self, target: NoteTarget, body: String) -> anyhow::Result<Note> {
        if body.len() > MAX_NOTE_BYTES {
            anyhow::bail!("note exceeds {MAX_NOTE_BYTES} bytes");
        }
        let key = self.note_key(target)?;
        self.store.set_note(&key, &body)?;
        // Counts live on the tree, so every client's columns are stale
        // until this goes out.
        self.broadcast_tree();
        Ok(Note::new(target, body))
    }

    /// Flips one checkbox, reading the body from the store rather than
    /// taking it from the caller.
    ///
    /// A client toggling a box has generally not opened the note — the
    /// count it is acting on arrived with the tree — so accepting a whole
    /// body here would mean writing back a copy that may predate whatever
    /// an agent added in the meantime. Taking the line number instead makes
    /// the smallest possible claim: this line, this state, everything else
    /// as it currently stands.
    pub fn set_todo(
        &self,
        target: NoteTarget,
        line: usize,
        state: TodoState,
    ) -> anyhow::Result<Note> {
        let key = self.note_key(target)?;
        let body = self
            .store
            .note(&key)?
            .ok_or_else(|| anyhow::anyhow!("no note to toggle"))?;
        let body = argus_protocol::set_todo_state(&body, line, state)
            .ok_or_else(|| anyhow::anyhow!("line {} is not a checkbox", line + 1))?;
        self.store.set_note(&key, &body)?;
        self.broadcast_tree();
        Ok(Note::new(target, body))
    }

    /// Every note's counts in one read, keyed for the snapshot to look up.
    ///
    /// A failed read yields no counts rather than failing the snapshot: the
    /// tree is how the user reaches everything else, and it should not go
    /// dark because a note could not be counted.
    pub(super) fn note_summaries(&self) -> HashMap<NoteKey, NoteSummary> {
        let notes = match self.store.notes() {
            Ok(notes) => notes,
            Err(e) => {
                tracing::warn!("reading notes for the tree: {e:#}");
                return HashMap::new();
            }
        };
        notes
            .into_iter()
            .map(|(key, body)| {
                let counts = argus_protocol::note_counts(&argus_protocol::parse_todos(&body));
                (key, (counts, true))
            })
            .collect()
    }
}
