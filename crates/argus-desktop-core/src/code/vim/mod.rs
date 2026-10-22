//! Vim as a state machine over the editor's rope: keys in, the changes
//! they make out, with no window, no timer and no clipboard of its own.
//! The host feeds `Vim::key` the keys it has and applies what comes back —
//! the change set to the buffer it shows and to the live sync, the effects
//! it cannot do itself (write the file, scroll the view, step to the next
//! change mark). What every editor shares (registers, the search, macros,
//! mappings, options) is `Shared`, which the host holds and passes in.
//!
//! The subset is the one the page's editor offers, `@replit/codemirror-vim`
//! as the page configures it (EDITOR-VIM-5 to -22), all of it: normal,
//! insert, replace, visual, visual-line and visual-block modes; counts and
//! registers; every motion, operator and text object the library's keymap
//! names, including search (`/ ? n N * # g* g# gn`), marks and the jump
//! list, sentences and tags, `H M L`, paging and the `z` family; macros and
//! `.`; insert mode's `Ctrl-o`, `Ctrl-r`, `Ctrl-w` and its kin; and the ex
//! line with ranges, `:s :g :v :norm :sor :reg :marks :delm :set :map` and
//! the rest of the library's table. Where the library does something the
//! window must do (scroll, show the matches, write the file) it is an
//! `Effect`. A key Vim has no use for is taken and does nothing, as an
//! unbound key is in the library; a chord it does not own goes back to the
//! host unhandled.

pub mod block;
pub mod buffer;
pub mod command;
pub mod comment;
pub mod dispatch;
pub mod display;
pub mod ex;
pub mod host;
pub mod insert;
pub mod jump;
pub mod key;
pub mod keymap;
pub mod macros;
pub mod maps;
pub mod marks;
pub mod motion;
pub mod normal;
pub mod number;
pub mod object;
pub mod operate;
pub mod options;
pub mod page;
pub mod paste;
pub mod prompt;
pub mod recall;
pub mod region;
pub mod registers;
pub mod replay;
pub mod rewrite;
pub mod said;
pub mod screen;
pub mod search;
pub mod seek;
pub mod sentence;
pub mod shared;
pub mod symbol;
pub mod tag;
pub mod text;
pub mod visual;
pub mod word;
pub mod wrap;

use super::change::ChangeSet;
use buffer::Buffer;
use command::Command;
use key::{Code, Key};
use options::Locals;
use prompt::Prompt;
use screen::{Place, View};
pub use shared::Shared;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Replace,
    Visual,
    VisualLine,
    VisualBlock,
    /// The `:` line is being typed.
    Command,
    /// The `/` or `?` line is being typed.
    Search,
    /// `:s///c` is asking whether to replace a match: `y n a l q`.
    Confirm,
}

/// What a key asks of the host beyond the change to the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// `:w`: write the buffer now.
    Write,
    /// `]c`
    NextChange,
    /// `[c`
    PreviousChange,
    /// A line for the bottom of the editor, such as an ex error.
    Message(String),
    /// Scroll so `line` is at `at` in the view; `zt`, `Ctrl-e`, `Ctrl-d`.
    Reveal { line: usize, at: Place },
    /// `Ctrl-c` in a selection: its text goes on the clipboard, as the
    /// browser's copy does it, and the selection ends.
    Copy(String),
}

/// What a key did.
#[derive(Debug, Default)]
pub struct Step {
    /// Whether Vim took the key; if not the host's keymap hears it.
    pub handled: bool,
    /// The edits it made, as one set on the text before the key.
    pub change: Option<ChangeSet>,
    pub effects: Vec<Effect>,
}

/// The last change, for `.`: the command and the keys typed in the insert it
/// began, if any; and, for an operator on a selection, how big the
/// selection was, so `.` makes one of that size from the cursor.
#[derive(Debug, Clone)]
struct Dot {
    command: Command,
    typed: Vec<Key>,
    shape: Option<Shape>,
}

/// A selection's size: its mode, and how many lines and columns apart its
/// ends were.
#[derive(Debug, Clone, Copy)]
struct Shape {
    mode: Mode,
    lines: usize,
    cols: usize,
}

/// An insert from its first key to Escape.
struct Insert {
    /// How many times the text is entered in all (`3ia`).
    count: usize,
    /// What `o` and `O` repeat after: a line break.
    lines: bool,
    text: String,
    /// `Ctrl-r` was pressed: the next key names a register.
    register: bool,
    /// A visual block's `I`, `A` and `c`: the text typed goes into each row.
    block: Option<block::BlockInsert>,
}

/// What a selection was, for `gv`; where it was is in the marks.
#[derive(Debug, Clone, Copy)]
struct LastVisual {
    mode: Mode,
    /// A block that ran to every line's end.
    eol: bool,
}

/// Where `Ctrl-o` in insert mode is: pressed, or running its one command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OneCommand {
    No,
    Armed,
    Waiting,
}

pub struct Vim {
    buf: Buffer,
    mode: Mode,
    /// A byte on a character; in insert mode, the caret between characters.
    cursor: usize,
    /// The other end of the visual selection.
    anchor: usize,
    /// The column `j` and `k` aim for, in characters; `usize::MAX` after
    /// `$`, which in a visual block runs it to every line's end.
    want: usize,
    pending: Vec<Key>,
    dot: Option<Dot>,
    /// Keys are being added to `dot.typed`.
    recording: bool,
    replaying: bool,
    insert: Option<Insert>,
    last_visual: Option<LastVisual>,
    prompt: Option<Prompt>,
    /// The mode a `/` line returns to: visual if it was typed in a selection.
    back_to: Mode,
    one_command: OneCommand,
    /// Where the cursor was before `*` moved it to the word: the jump the
    /// search that follows records as its start.
    jump_from: Option<usize>,
    /// A `:g` is running its command on each line: a `:s` in it says nothing.
    in_global: bool,
    /// `:s///c` is waiting on a key.
    confirm: Option<ex::substitute::Confirm>,
    message: Option<String>,
    effects: Vec<Effect>,
    view: View,
    wrap: Option<usize>,
    locals: Locals,
    /// The editor's line comment token, for `gc`; none is the page's case,
    /// where no language is loaded.
    comment: Option<String>,
    /// Mappings are applied to the keys fed: false inside a `noremap`.
    remap: bool,
    /// The mappings being expanded, so one that maps to itself ends.
    stack: Vec<usize>,
    /// An insert-mode mapping that is partly typed, and what it typed.
    imap: replay::InsertMap,
}

impl Vim {
    /// An editor on `text`, in normal mode with the cursor at the start.
    pub fn new(text: &str) -> Vim {
        Vim {
            buf: Buffer::new(text),
            mode: Mode::Normal,
            cursor: 0,
            anchor: 0,
            want: 0,
            pending: Vec::new(),
            dot: None,
            recording: false,
            replaying: false,
            insert: None,
            last_visual: None,
            prompt: None,
            back_to: Mode::Normal,
            one_command: OneCommand::No,
            jump_from: None,
            in_global: false,
            confirm: None,
            message: None,
            effects: Vec::new(),
            view: View::default(),
            wrap: None,
            locals: Locals::default(),
            comment: None,
            remap: true,
            stack: Vec::new(),
            imap: replay::InsertMap::default(),
        }
    }

    /// The cursor put where the pointer clicked.
    pub fn place(&mut self, at: usize) {
        self.pending.clear();
        self.cursor = self.buf.doc().floor_char_boundary(at.min(self.buf.doc().len()));
        if matches!(self.mode, Mode::Command | Mode::Search | Mode::Confirm) {
            self.prompt = None;
            self.mode = if self.mode == Mode::Search { self.back_to } else { Mode::Normal };
        }
        self.want = self.buf.text().col(self.cursor);
        self.clamp();
    }

    /// `remote` was made on the text held — the disk's change, or an edit the
    /// host made itself — and is taken in; the cursor and the selection move
    /// past it, and undo never reaches it.
    pub fn past(&mut self, remote: &ChangeSet) {
        self.buf.past(remote);
        self.cursor = remote.map_pos(self.cursor, -1);
        self.anchor = remote.map_pos(self.anchor, -1);
        self.clamp();
    }

    /// The time set by `insertModeEscKeysTimeout` passed with an insert
    /// mapping half typed: what was typed stands.
    pub fn timeout(&mut self) {
        self.imap.clear();
    }

    pub fn key(&mut self, sh: &mut Shared, key: Key) -> Step {
        self.message = None;
        self.effects.clear();
        sh.macros.left = macros::MOST_KEYS;
        let handled = self.feed(sh, key, true);
        Step { handled, change: self.buf.take_step(), effects: std::mem::take(&mut self.effects) }
    }

    /// One key through macro recording, mappings and the mode it finds the
    /// editor in. `typed` is whether the person typed it, as opposed to a
    /// mapping, a macro or `:normal`.
    pub(super) fn feed(&mut self, sh: &mut Shared, key: Key, typed: bool) -> bool {
        // Ctrl-[ and Ctrl-c are Escape; in a selection Ctrl-c first copies it,
        // as the browser's copy does on the page.
        if key == Key::ctrl('c') && self.is_visual() {
            let t = self.buf.text();
            let rows: Vec<String> = self.selections().iter().map(|&(a, b)| t.slice(a, b)).collect();
            let mut text = rows.join("\n");
            // A line selection of the library ends at the last line's end, not past its break.
            if self.mode == Mode::VisualLine && text.ends_with('\n') {
                text.pop();
            }
            self.effects.push(Effect::Copy(text));
        }
        let key = if key.is_esc() { Key::of(Code::Esc) } else { key };
        if typed {
            if self.recorded(sh, key) {
                return true;
            }
        } else {
            if sh.macros.left == 0 {
                return false;
            }
            sh.macros.left -= 1;
        }
        let handled = match self.mode {
            Mode::Insert | Mode::Replace => self.insert_key(sh, key),
            Mode::Command | Mode::Search => self.prompt_key(sh, key),
            Mode::Confirm => self.confirm_key(sh, key),
            _ => self.command_key(sh, key),
        };
        self.settle();
        handled
    }

    /// After every key: the open unit is closed, the cursor is put where
    /// the mode lets it rest, and a `Ctrl-o` command that is done returns to
    /// insert mode.
    fn settle(&mut self) {
        if matches!(self.mode, Mode::Insert | Mode::Replace) {
            return;
        }
        self.buf.commit();
        match self.one_command {
            // The cursor may stand past the last character for the command.
            OneCommand::Armed => self.one_command = OneCommand::Waiting,
            OneCommand::Waiting if self.mode == Mode::Normal && self.pending.is_empty() => {
                self.clamp();
                self.one_command = OneCommand::No;
                self.start_insert(1, false);
            }
            OneCommand::Waiting if self.is_visual() => {
                self.one_command = OneCommand::No;
                self.clamp();
            }
            _ => self.clamp(),
        }
    }


    pub fn is_visual(&self) -> bool {
        matches!(self.mode, Mode::Visual | Mode::VisualLine | Mode::VisualBlock)
    }

    /// Puts the cursor where the mode lets it rest: on a character, or on
    /// the empty line; the caret of insert mode may be at a line's end.
    fn clamp(&mut self) {
        let t = self.buf.text();
        self.cursor = self.cursor.min(t.len());
        self.anchor = self.anchor.min(t.len());
        // A command that leaves the cursor inside a character has a bug; the text is spared it.
        // (The anchor means nothing outside a selection, and edits move it into a character.)
        debug_assert!(t.0.is_char_boundary(self.cursor) && (!self.is_visual() || t.0.is_char_boundary(self.anchor)));
        self.cursor = t.0.floor_char_boundary(self.cursor);
        self.anchor = t.0.floor_char_boundary(self.anchor);
        if matches!(self.mode, Mode::Insert | Mode::Replace) {
            return;
        }
        let line = t.line_of(self.cursor);
        let last = t.last_char(line);
        if self.cursor > last {
            self.cursor = last;
        }
        if self.is_visual() {
            let line = t.line_of(self.anchor);
            self.anchor = self.anchor.min(t.last_char(line));
        }
    }
}

#[cfg(test)]
mod tests;
