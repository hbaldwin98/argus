//! Notes: reading them, writing them, and counting what is in them.
//!
//! The daemon's job here is translation and arbitration. Clients speak in
//! ids, which are handed out fresh on every start; the store speaks in
//! names and paths, which are not. Everything in this module exists on one
//! side of that line or the other, and nothing else in the process has to
//! know both.

use std::collections::HashMap;

use argus_protocol::{
    AgentContext, ContextNote, ContextScope, Note, NoteCounts, NoteTarget, TodoAudit, TodoState,
    TodoWrite, MAX_NOTE_BYTES,
};

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
        // The audit rides along on the read a person asked for, and only
        // there: it exists to be looked at next to the lines it explains.
        let audit = self.store.note_audit(&key).unwrap_or_else(|e| {
            tracing::warn!("reading a note's audit record: {e:#}");
            Vec::new()
        });
        Ok(Note::new(target, body).with_audit(audit))
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

    /// The notes a live agent pane may read: its project's, then its
    /// checkout's.
    ///
    /// Read straight from the store by durable key rather than through
    /// [`Daemon::note`], because an agent has no ids to name a target with
    /// and should not be handed any — the pane it asks from is what decides
    /// which two notes exist for it. Nothing outside that pair is
    /// reachable, which is the whole of the scoping.
    ///
    /// Notes are optional and usually absent, so an unwritten one is left
    /// out instead of arriving as an empty document.
    pub fn context_for_agent(&self, pane_id: PaneId) -> anyhow::Result<AgentContext> {
        let scope = self.agent_scope(pane_id)?;
        let sources = [
            (
                ContextScope::Project,
                scope.project_name.clone(),
                NoteKey::Project(scope.project_name),
            ),
            (
                ContextScope::Checkout,
                scope.checkout_path.to_string_lossy().to_string(),
                NoteKey::checkout(&scope.checkout_path),
            ),
        ];
        let mut notes = Vec::new();
        for (context_scope, name, key) in sources {
            let Some(body) = self.store.note(&key)? else {
                continue;
            };
            if body.trim().is_empty() {
                continue;
            }
            notes.push(ContextNote::new(context_scope, name, body));
        }
        Ok(AgentContext { notes })
    }

    /// One agent's change to its own checkout's note.
    ///
    /// Three gates, in the order they cost the least to fail: the caller
    /// must be a live agent pane (`agent_scope`), its project must have
    /// asked for this (`agent_todos`), and the change itself must be one
    /// the note can take. Only the checkout note is reachable — an agent
    /// has no way to name the project note, and would be refused if it
    /// could, because standing instructions are the human's side of this
    /// conversation.
    ///
    /// The record and the write commit together; see
    /// [`crate::store::Store::set_note_as_agent`].
    pub fn write_agent_todo(
        &self,
        pane_id: PaneId,
        session: Option<&str>,
        write: &TodoWrite,
    ) -> anyhow::Result<NoteCounts> {
        let scope = self.agent_scope(pane_id)?;
        if !scope.todos_allowed {
            anyhow::bail!(
                "project {} does not allow agents to write notes; \
                 a human can set agent_todos = true on it",
                scope.project_name
            );
        }
        let key = NoteKey::checkout(&scope.checkout_path);
        let body = self.store.note(&key)?.unwrap_or_default();
        let (body, detail) = match write {
            TodoWrite::Add { text } => {
                let text = text.trim();
                if text.is_empty() {
                    anyhow::bail!("an item needs some text");
                }
                let body = argus_protocol::append_todo(&body, text);
                if body.len() > MAX_NOTE_BYTES {
                    anyhow::bail!("the note is full at {MAX_NOTE_BYTES} bytes");
                }
                (body, text.to_string())
            }
            TodoWrite::Set { line, state } => {
                // Pinned lines are standing instructions, and ticking one
                // off would delete the instruction rather than complete a
                // task. An agent may only move the two states that are
                // about work.
                if *state == TodoState::Pinned {
                    anyhow::bail!("only a human pins an item");
                }
                let todo = argus_protocol::parse_todos(&body)
                    .into_iter()
                    .find(|todo| todo.line == *line)
                    .ok_or_else(|| anyhow::anyhow!("line {} is not a checkbox", line + 1))?;
                if todo.state == TodoState::Pinned {
                    anyhow::bail!("line {} is a standing instruction", line + 1);
                }
                let body = argus_protocol::set_todo_state(&body, *line, *state)
                    .ok_or_else(|| anyhow::anyhow!("line {} is not a checkbox", line + 1))?;
                (body, todo.text)
            }
        };
        let entry = TodoAudit {
            at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or_default(),
            session: session.map(str::to_string),
            action: write.action().to_string(),
            detail,
        };
        self.store.set_note_as_agent(&key, &body, &entry)?;
        // A checkbox appearing or being ticked changes the counts every
        // client's columns are drawn from.
        self.broadcast_tree();
        Ok(argus_protocol::note_counts(&argus_protocol::parse_todos(
            &body,
        )))
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
