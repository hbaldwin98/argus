//! Plain Markdown notes, and the one thing Argus reads out of them.
//!
//! A note is text the user owns; Argus does not parse Markdown and has no
//! opinion about headings, emphasis, or tables. It reads exactly one
//! construct — the checkbox line — because that is the construct with a
//! state worth counting, and counting is what lets a note say something
//! from a column too narrow to show it.

use serde::{Deserialize, Serialize};

use crate::ids::{CheckoutId, ProjectId};

/// Past this a note stops being a note. Large enough for a working
/// document, small enough that the whole thing rides one message.
pub const MAX_NOTE_BYTES: usize = 64 * 1024;

/// What a note is attached to. Projects and checkouts hold notes; panes do
/// not, because a pane is a process and outlives nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NoteTarget {
    Project(ProjectId),
    Checkout(CheckoutId),
}

/// The state of one checkbox line.
///
/// `Pinned` is a third state rather than a flag beside the first two: a
/// pinned item is one an agent should be told about without being asked,
/// which is a different thing to do about a line, not a decoration on
/// being open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TodoState {
    /// `- [ ]`
    Open,
    /// `- [x]`
    Done,
    /// `- [!]`
    Pinned,
}

impl TodoState {
    /// The character inside the brackets, for writing a line back out.
    pub fn marker(self) -> char {
        match self {
            TodoState::Open => ' ',
            TodoState::Done => 'x',
            TodoState::Pinned => '!',
        }
    }

    /// What toggling a checkbox does: open becomes done, done becomes open
    /// again. Pinned is left alone — it was set deliberately, and a stray
    /// keypress should not silently unpin something an agent is reading.
    pub fn toggled(self) -> TodoState {
        match self {
            TodoState::Open => TodoState::Done,
            TodoState::Done => TodoState::Open,
            TodoState::Pinned => TodoState::Pinned,
        }
    }
}

/// One checkbox line, located well enough to rewrite it in place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Todo {
    /// Zero-based line number within the note body.
    pub line: usize,
    pub state: TodoState,
    /// The text after the checkbox, trimmed. What a rollup or an injected
    /// pinned item actually says.
    pub text: String,
}

/// How many of each state a note holds, summed up the tree.
///
/// Derived rather than stored: the note body is the truth, and a count kept
/// beside it is a second truth waiting to disagree with the first.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteCounts {
    pub open: usize,
    pub done: usize,
    pub pinned: usize,
}

impl NoteCounts {
    /// Nothing to show. A column suppresses the whole indicator on this
    /// rather than drawing three zeroes.
    pub fn is_empty(self) -> bool {
        self.open == 0 && self.done == 0 && self.pinned == 0
    }

    /// What a row still owes. Done items are history and pinned items are
    /// standing instructions, so only open items are outstanding.
    pub fn outstanding(self) -> usize {
        self.open
    }
}

impl std::ops::Add for NoteCounts {
    type Output = NoteCounts;

    fn add(self, rhs: NoteCounts) -> NoteCounts {
        NoteCounts {
            open: self.open + rhs.open,
            done: self.done + rhs.done,
            pinned: self.pinned + rhs.pinned,
        }
    }
}

impl std::iter::Sum for NoteCounts {
    fn sum<I: Iterator<Item = NoteCounts>>(iter: I) -> NoteCounts {
        iter.fold(NoteCounts::default(), |a, b| a + b)
    }
}

/// A note as it stands, with what was read out of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub target: NoteTarget,
    pub body: String,
    pub todos: Vec<Todo>,
}

impl Note {
    pub fn new(target: NoteTarget, body: String) -> Note {
        let todos = parse_todos(&body);
        Note {
            target,
            body,
            todos,
        }
    }

    pub fn counts(&self) -> NoteCounts {
        counts(&self.todos)
    }

    /// The standing instructions, in document order. What a template's
    /// opt-in injection sends, and what an agent asking for context reads.
    pub fn pinned(&self) -> impl Iterator<Item = &Todo> {
        self.todos.iter().filter(|t| t.state == TodoState::Pinned)
    }
}

pub fn counts(todos: &[Todo]) -> NoteCounts {
    let mut counts = NoteCounts::default();
    for todo in todos {
        match todo.state {
            TodoState::Open => counts.open += 1,
            TodoState::Done => counts.done += 1,
            TodoState::Pinned => counts.pinned += 1,
        }
    }
    counts
}

/// Every checkbox line in a note body.
///
/// The grammar is GitHub's task list, plus `!`: optional indent, a bullet
/// (`-`, `*`, or `+`), a space, `[`, one state character, `]`, and then
/// either end of line or a space before the text. That trailing space is
/// required so `- [x]done` — which no Markdown renderer treats as a
/// checkbox — is not counted as one here either.
pub fn parse_todos(body: &str) -> Vec<Todo> {
    body.lines()
        .enumerate()
        .filter_map(|(line, text)| parse_todo(line, text))
        .collect()
}

fn parse_todo(line: usize, text: &str) -> Option<Todo> {
    let rest = text.trim_start();
    let rest = rest
        .strip_prefix("- ")
        .or_else(|| rest.strip_prefix("* "))
        .or_else(|| rest.strip_prefix("+ "))?;
    let rest = rest.trim_start().strip_prefix('[')?;
    let mut chars = rest.chars();
    let state = match chars.next()? {
        ' ' => TodoState::Open,
        'x' | 'X' => TodoState::Done,
        '!' => TodoState::Pinned,
        _ => return None,
    };
    let rest = chars.as_str().strip_prefix(']')?;
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None;
    }
    Some(Todo {
        line,
        state,
        text: rest.trim().to_string(),
    })
}

/// Rewrites one checkbox line's state, leaving every other byte alone.
///
/// The body is the user's document, so this edits the marker in place
/// rather than regenerating the line from the parsed [`Todo`]: indentation,
/// bullet character, and trailing whitespace are theirs to keep. Returns
/// `None` when that line is not a checkbox, which is what a toggle aimed at
/// an ordinary line should do.
pub fn set_todo_state(body: &str, line: usize, state: TodoState) -> Option<String> {
    let mut lines: Vec<&str> = body.lines().collect();
    let text = *lines.get(line)?;
    parse_todo(line, text)?;
    // The checkbox opens at the first `[` on the line; everything before it
    // is indent and bullet, which the parse just accepted and this must not
    // disturb. The marker is one character, so the tail resumes two past it.
    let open = text.find('[')?;
    let edited = format!("{}[{}{}", &text[..open], state.marker(), &text[open + 2..]);
    lines[line] = &edited;
    let mut out = lines.join("\n");
    // `lines()` drops a trailing newline; a note that ended with one is
    // still meant to end with one.
    if body.ends_with('\n') {
        out.push('\n');
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_three_checkbox_states() {
        let todos = parse_todos("- [ ] open\n- [x] done\n- [!] pinned\n");
        assert_eq!(
            todos.iter().map(|t| t.state).collect::<Vec<_>>(),
            [TodoState::Open, TodoState::Done, TodoState::Pinned]
        );
        assert_eq!(todos[2].text, "pinned");
        assert_eq!(todos[2].line, 2);
    }

    #[test]
    fn counts_roll_up_by_state() {
        let note = Note::new(
            NoteTarget::Project(ProjectId(1)),
            "- [ ] a\n- [ ] b\n- [x] c\n- [!] d\n".to_string(),
        );
        assert_eq!(
            note.counts(),
            NoteCounts {
                open: 2,
                done: 1,
                pinned: 1
            }
        );
        assert_eq!(note.counts().outstanding(), 2);
    }

    #[test]
    fn sums_counts_across_children() {
        let a = NoteCounts {
            open: 1,
            done: 2,
            pinned: 3,
        };
        let b = NoteCounts {
            open: 10,
            done: 20,
            pinned: 30,
        };
        assert_eq!(
            a + b,
            NoteCounts {
                open: 11,
                done: 22,
                pinned: 33
            }
        );
        assert_eq!([a, b].into_iter().sum::<NoteCounts>(), a + b);
        assert!(NoteCounts::default().is_empty());
        assert!(!a.is_empty());
    }

    #[test]
    fn accepts_indented_lines_and_every_bullet() {
        let todos = parse_todos("    - [ ] indented\n* [x] star\n+ [!] plus\n");
        assert_eq!(todos.len(), 3);
        assert_eq!(todos[0].text, "indented");
        assert_eq!(todos[1].text, "star");
        assert_eq!(todos[2].text, "plus");
    }

    #[test]
    fn ignores_prose_and_malformed_checkboxes() {
        let body = "# Heading\n\nordinary prose\n- a plain bullet\n- [] empty\n\
                    - [q] unknown\n- [x]no space\n[ ] no bullet\n";
        assert_eq!(parse_todos(body), []);
    }

    #[test]
    fn an_empty_checkbox_line_is_still_a_checkbox() {
        let todos = parse_todos("- [ ]\n");
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].text, "");
    }

    #[test]
    fn toggling_rewrites_only_the_marker() {
        let body = "  - [ ] keep my indent   \nprose\n";
        let out = set_todo_state(body, 0, TodoState::Done).unwrap();
        assert_eq!(out, "  - [x] keep my indent   \nprose\n");
    }

    #[test]
    fn toggling_preserves_a_missing_trailing_newline() {
        assert_eq!(
            set_todo_state("- [x] done", 0, TodoState::Open).unwrap(),
            "- [ ] done"
        );
    }

    #[test]
    fn toggling_a_line_that_is_not_a_checkbox_changes_nothing() {
        assert_eq!(set_todo_state("prose\n", 0, TodoState::Done), None);
        assert_eq!(set_todo_state("- [ ] a\n", 5, TodoState::Done), None);
    }

    #[test]
    fn open_and_done_cycle_but_pinned_holds() {
        assert_eq!(TodoState::Open.toggled(), TodoState::Done);
        assert_eq!(TodoState::Done.toggled(), TodoState::Open);
        assert_eq!(TodoState::Pinned.toggled(), TodoState::Pinned);
    }

    #[test]
    fn pinned_items_read_out_in_document_order() {
        let note = Note::new(
            NoteTarget::Checkout(CheckoutId(2)),
            "- [!] first\n- [ ] middle\n- [!] second\n".to_string(),
        );
        assert_eq!(
            note.pinned().map(|t| t.text.as_str()).collect::<Vec<_>>(),
            ["first", "second"]
        );
    }
}
