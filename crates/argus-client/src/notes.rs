//! The note editing surface: a small modal text editor over one note.
//!
//! Modal because a note is two things at once. Most of the time it is read
//! and ticked off, which wants single-key navigation like every other
//! column here; occasionally it is written, which wants every key to be a
//! character. A mode is what lets `space` mean "tick this box" in the first
//! job and " " in the second.
//!
//! Everything below is the model — no widgets, no terminal. The view draws
//! it and `App` decides when to send it, which keeps the editing rules
//! testable without a screen.

use argus_protocol::{parse_todos, Note, NoteCounts, NoteTarget, Todo, TodoState};

/// Whether keys navigate or type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteMode {
    /// `j`/`k` move, `space` ticks a box, `i`/`o` start typing.
    View,
    /// Keys are text. `Esc` returns to [`NoteMode::View`] and saves.
    Insert,
}

/// One note, open.
pub struct NoteView {
    pub target: NoteTarget,
    /// What the note is about, for the window title. Carried rather than
    /// looked up so the title survives the row leaving the tree.
    pub title: String,
    /// The body as editable lines. Always at least one, so a cursor always
    /// has somewhere to be.
    pub lines: Vec<String>,
    pub mode: NoteMode,
    /// Cursor line, and column as a character offset within it.
    pub line: usize,
    pub column: usize,
    /// First visible line, moved only to keep the cursor on screen.
    pub scroll: usize,
    /// Edited since the last write went out.
    pub dirty: bool,
    /// Why the last write did not land, if it did not.
    pub error: Option<String>,
}

impl NoteView {
    pub fn new(note: &Note, title: String) -> NoteView {
        NoteView {
            target: note.target,
            title,
            lines: split(&note.body),
            mode: NoteMode::View,
            line: 0,
            column: 0,
            scroll: 0,
            dirty: false,
            error: None,
        }
    }

    /// Takes a note that arrived from the daemon.
    ///
    /// Unsaved edits win: the daemon echoes back every write, so a note
    /// arriving while the user is mid-sentence is either the echo of what
    /// they already have or someone else's write, and neither is worth
    /// throwing away what has not been sent yet.
    pub fn adopt(&mut self, note: &Note) {
        if self.dirty {
            return;
        }
        self.lines = split(&note.body);
        self.error = None;
        self.clamp();
    }

    pub fn body(&self) -> String {
        self.lines.join("\n")
    }

    pub fn todos(&self) -> Vec<Todo> {
        parse_todos(&self.body())
    }

    pub fn counts(&self) -> NoteCounts {
        argus_protocol::note_counts(&self.todos())
    }

    /// The checkbox on the cursor's line, if that line has one.
    pub fn todo_here(&self) -> Option<Todo> {
        self.todos().into_iter().find(|t| t.line == self.line)
    }

    /// What ticking the cursor's box would set it to, and where. `None`
    /// when the cursor is not on a checkbox, which is when the key should
    /// do nothing rather than something surprising.
    pub fn toggle_here(&self) -> Option<(usize, TodoState)> {
        let todo = self.todo_here()?;
        Some((todo.line, todo.state.toggled()))
    }

    // ---- moving ------------------------------------------------------

    pub fn move_by(&mut self, delta: isize) {
        let last = self.lines.len().saturating_sub(1);
        self.line = self.line.saturating_add_signed(delta).min(last);
        self.clamp_column();
    }

    pub fn top_of_note(&mut self) {
        self.line = 0;
        self.column = 0;
    }

    pub fn bottom_of_note(&mut self) {
        self.line = self.lines.len().saturating_sub(1);
        self.clamp_column();
    }

    pub fn move_column(&mut self, delta: isize) {
        let width = self.lines[self.line].chars().count();
        self.column = self.column.saturating_add_signed(delta).min(width);
    }

    pub fn start_of_line(&mut self) {
        self.column = 0;
    }

    pub fn end_of_line(&mut self) {
        self.column = self.lines[self.line].chars().count();
    }

    /// Keeps the cursor line on screen for a viewport this tall.
    ///
    /// Called by the view because only the view knows the height, and a
    /// scroll position computed against last frame's height would be wrong
    /// the first frame after a resize.
    pub fn follow_cursor(&mut self, height: usize) {
        if height == 0 {
            return;
        }
        if self.line < self.scroll {
            self.scroll = self.line;
        } else if self.line >= self.scroll + height {
            self.scroll = self.line + 1 - height;
        }
    }

    // ---- editing -----------------------------------------------------

    pub fn insert_mode(&mut self) {
        self.mode = NoteMode::Insert;
    }

    /// Leaves insert mode. The caller saves if this says the body changed.
    pub fn view_mode(&mut self) {
        self.mode = NoteMode::View;
    }

    /// Opens a blank line below the cursor and starts typing on it, with
    /// the current line's indent carried down — a list stays a list.
    pub fn open_below(&mut self) {
        let indent: String = self.lines[self.line]
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect();
        self.column = indent.chars().count();
        self.lines.insert(self.line + 1, indent);
        self.line += 1;
        self.mode = NoteMode::Insert;
        self.dirty = true;
    }

    pub fn insert_char(&mut self, c: char) {
        let at = self.byte_offset();
        self.lines[self.line].insert(at, c);
        self.column += 1;
        self.dirty = true;
    }

    pub fn newline(&mut self) {
        let at = self.byte_offset();
        let tail = self.lines[self.line].split_off(at);
        self.lines.insert(self.line + 1, tail);
        self.line += 1;
        self.column = 0;
        self.dirty = true;
    }

    /// Backspace: deletes the character before the cursor, or joins this
    /// line to the one above when there is nothing before it.
    pub fn backspace(&mut self) {
        if self.column > 0 {
            self.column -= 1;
            let at = self.byte_offset();
            self.lines[self.line].remove(at);
            self.dirty = true;
            return;
        }
        if self.line == 0 {
            return;
        }
        let tail = self.lines.remove(self.line);
        self.line -= 1;
        self.column = self.lines[self.line].chars().count();
        self.lines[self.line].push_str(&tail);
        self.dirty = true;
    }

    /// Marks the body as sent. The caller does this once a write is on the
    /// wire, so a later note from the daemon is allowed to land.
    pub fn saved(&mut self) {
        self.dirty = false;
    }

    fn byte_offset(&self) -> usize {
        self.lines[self.line]
            .char_indices()
            .nth(self.column)
            .map(|(i, _)| i)
            .unwrap_or(self.lines[self.line].len())
    }

    fn clamp(&mut self) {
        self.line = self.line.min(self.lines.len().saturating_sub(1));
        self.clamp_column();
    }

    fn clamp_column(&mut self) {
        self.column = self.column.min(self.lines[self.line].chars().count());
    }
}

/// A body as editable lines, always at least one.
///
/// `lines()` gives nothing at all for an empty note and drops a trailing
/// newline, and an editor with no lines has nowhere to put the cursor.
fn split(body: &str) -> Vec<String> {
    if body.is_empty() {
        return vec![String::new()];
    }
    let mut lines: Vec<String> = body.lines().map(str::to_string).collect();
    if body.ends_with('\n') {
        lines.push(String::new());
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_protocol::CheckoutId;

    fn view(body: &str) -> NoteView {
        let note = Note::new(NoteTarget::Checkout(CheckoutId(1)), body.to_string());
        NoteView::new(&note, "repo".to_string())
    }

    #[test]
    fn an_empty_note_still_has_a_line_to_stand_on() {
        let v = view("");
        assert_eq!(v.lines, [""]);
        assert_eq!((v.line, v.column), (0, 0));
        assert_eq!(v.body(), "");
    }

    #[test]
    fn a_trailing_newline_is_a_line_you_can_type_on() {
        let v = view("- [ ] one\n");
        assert_eq!(v.lines, ["- [ ] one", ""]);
    }

    #[test]
    fn moving_stops_at_both_ends() {
        let mut v = view("a\nb\nc");
        v.move_by(-1);
        assert_eq!(v.line, 0);
        v.move_by(99);
        assert_eq!(v.line, 2);
        v.top_of_note();
        assert_eq!(v.line, 0);
        v.bottom_of_note();
        assert_eq!(v.line, 2);
    }

    #[test]
    fn the_column_never_hangs_off_a_shorter_line() {
        let mut v = view("a long line\nx");
        v.end_of_line();
        assert_eq!(v.column, 11);
        v.move_by(1);
        assert_eq!(v.column, 1, "clamped to the shorter line");
    }

    #[test]
    fn typing_inserts_at_the_cursor_and_marks_the_note_dirty() {
        let mut v = view("ac");
        v.move_column(1);
        v.insert_char('b');
        assert_eq!(v.body(), "abc");
        assert_eq!(v.column, 2);
        assert!(v.dirty);
        v.saved();
        assert!(!v.dirty);
    }

    #[test]
    fn enter_splits_a_line_at_the_cursor() {
        let mut v = view("abcd");
        v.move_column(2);
        v.newline();
        assert_eq!(v.body(), "ab\ncd");
        assert_eq!((v.line, v.column), (1, 0));
    }

    #[test]
    fn backspace_joins_a_line_to_the_one_above_it() {
        let mut v = view("ab\ncd");
        v.move_by(1);
        v.backspace();
        assert_eq!(v.body(), "abcd");
        assert_eq!((v.line, v.column), (0, 2));
    }

    #[test]
    fn backspace_at_the_very_start_does_nothing() {
        let mut v = view("ab");
        v.backspace();
        assert_eq!(v.body(), "ab");
        assert!(!v.dirty);
    }

    #[test]
    fn opening_a_line_below_carries_the_indent_down() {
        let mut v = view("    - [ ] one");
        v.open_below();
        assert_eq!(v.lines, ["    - [ ] one", "    "]);
        assert_eq!((v.line, v.column), (1, 4));
        assert_eq!(v.mode, NoteMode::Insert);
    }

    #[test]
    fn ticking_a_box_reports_the_line_and_its_next_state() {
        let mut v = view("# Plan\n- [ ] one\n- [x] two\n- [!] three");
        assert_eq!(v.toggle_here(), None, "a heading has no box");
        v.move_by(1);
        assert_eq!(v.toggle_here(), Some((1, TodoState::Done)));
        v.move_by(1);
        assert_eq!(v.toggle_here(), Some((2, TodoState::Open)));
        v.move_by(1);
        assert_eq!(
            v.toggle_here(),
            Some((3, TodoState::Pinned)),
            "a pinned item is not unpinned by a stray tick"
        );
    }

    #[test]
    fn counts_come_off_the_text_being_edited() {
        let mut v = view("- [ ] one");
        assert_eq!(v.counts().open, 1);
        v.bottom_of_note();
        v.end_of_line();
        v.newline();
        for c in "- [x] two".chars() {
            v.insert_char(c);
        }
        assert_eq!(v.counts(), NoteCounts { open: 1, done: 1, pinned: 0 });
    }

    #[test]
    fn a_note_from_the_daemon_lands_when_nothing_is_unsaved() {
        let mut v = view("old");
        let fresh = Note::new(NoteTarget::Checkout(CheckoutId(1)), "new\nlonger".into());
        v.adopt(&fresh);
        assert_eq!(v.body(), "new\nlonger");
    }

    #[test]
    fn a_note_from_the_daemon_never_overwrites_unsent_typing() {
        let mut v = view("mine");
        v.end_of_line();
        v.insert_char('!');
        let theirs = Note::new(NoteTarget::Checkout(CheckoutId(1)), "theirs".into());
        v.adopt(&theirs);
        assert_eq!(v.body(), "mine!", "unsaved edits win");
    }

    #[test]
    fn adopting_a_shorter_note_brings_the_cursor_back_in() {
        let mut v = view("one\ntwo\nthree");
        v.bottom_of_note();
        v.end_of_line();
        let shorter = Note::new(NoteTarget::Checkout(CheckoutId(1)), "one".into());
        v.adopt(&shorter);
        assert_eq!((v.line, v.column), (0, 3));
    }

    #[test]
    fn scrolling_follows_the_cursor_off_either_edge() {
        let mut v = view("a\nb\nc\nd\ne\nf");
        v.follow_cursor(3);
        assert_eq!(v.scroll, 0);
        v.bottom_of_note();
        v.follow_cursor(3);
        assert_eq!(v.scroll, 3, "the last line is the last row shown");
        v.top_of_note();
        v.follow_cursor(3);
        assert_eq!(v.scroll, 0);
    }

    #[test]
    fn a_multibyte_line_edits_by_character_not_byte() {
        let mut v = view("héllo");
        v.end_of_line();
        assert_eq!(v.column, 5);
        v.backspace();
        assert_eq!(v.body(), "héll");
        v.move_column(-3);
        v.insert_char('x');
        assert_eq!(v.body(), "hxéll");
    }
}
