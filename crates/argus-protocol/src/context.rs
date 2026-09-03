//! What an agent is allowed to read about where it is running.
//!
//! An agent knows its own checkout directory and nothing else: the notes a
//! human writes live in `runtime.db`, not on disk, and the ids the tree is
//! drawn from are handed out fresh on every start. This is the shape those
//! notes travel in when an agent asks for them.
//!
//! Scope is the whole design. A pane belongs to one checkout inside one
//! project, and those two notes are what it may read — not another
//! checkout's, not another project's. Access is checkout-scoped for the
//! same reason review comments are: a pane id is runtime-only, so anything
//! durable has to key off the directory the agent is actually working in.
//!
//! Reads only. Writes are policy-gated and audited, and are not this.

use serde::{Deserialize, Serialize};

use crate::notes::{parse_todos, Todo, TodoState};

/// Which note a piece of context came from, so a reader can tell a
/// standing instruction for the whole project from one for this checkout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContextScope {
    Project,
    Checkout,
}

impl ContextScope {
    pub fn label(self) -> &'static str {
        match self {
            ContextScope::Project => "project",
            ContextScope::Checkout => "checkout",
        }
    }
}

/// One note, named by the thing that holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextNote {
    pub scope: ContextScope,
    /// The project's name or the checkout's path — what the human calls it,
    /// not an id that means nothing after a restart.
    pub name: String,
    pub body: String,
    pub todos: Vec<Todo>,
}

impl ContextNote {
    pub fn new(scope: ContextScope, name: String, body: String) -> ContextNote {
        let todos = parse_todos(&body);
        ContextNote {
            scope,
            name,
            body,
            todos,
        }
    }

    pub fn pinned(&self) -> impl Iterator<Item = &Todo> {
        self.todos.iter().filter(|t| t.state == TodoState::Pinned)
    }
}

/// Everything one agent may read, outermost scope first.
///
/// An empty note is dropped rather than carried, so "no context" and "a
/// note that says nothing" arrive as the same thing — which they are.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentContext {
    pub notes: Vec<ContextNote>,
}

impl AgentContext {
    /// The standing instructions across every scope, outermost first. What
    /// an agent is meant to read without being asked.
    pub fn pinned(&self) -> impl Iterator<Item = (ContextScope, &Todo)> {
        self.notes
            .iter()
            .flat_map(|note| note.pinned().map(|todo| (note.scope, todo)))
    }

    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn note(scope: ContextScope, body: &str) -> ContextNote {
        ContextNote::new(scope, scope.label().to_string(), body.to_string())
    }

    #[test]
    fn pinned_items_read_outermost_scope_first() {
        let context = AgentContext {
            notes: vec![
                note(ContextScope::Project, "- [!] house style\n- [ ] not this\n"),
                note(ContextScope::Checkout, "- [!] this branch only\n"),
            ],
        };
        assert_eq!(
            context
                .pinned()
                .map(|(scope, todo)| (scope, todo.text.as_str()))
                .collect::<Vec<_>>(),
            [
                (ContextScope::Project, "house style"),
                (ContextScope::Checkout, "this branch only"),
            ]
        );
    }

    #[test]
    fn a_context_with_no_notes_says_so() {
        assert!(AgentContext::default().is_empty());
        assert_eq!(AgentContext::default().pinned().count(), 0);
    }

    #[test]
    fn a_note_carries_its_whole_body_not_just_its_checkboxes() {
        let note = note(ContextScope::Checkout, "# Heading\n\nprose\n- [ ] a\n");
        assert!(note.body.contains("prose"));
        assert_eq!(note.todos.len(), 1);
        assert_eq!(note.pinned().count(), 0);
    }
}
