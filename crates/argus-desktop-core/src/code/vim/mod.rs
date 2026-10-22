//! Vim as a state machine over the editor's rope: keys in, the changes
//! they make out, with no window, no timer and no clipboard of its own.
//! The host feeds `Vim::key` the keys it has and applies what comes back —
//! the change set to the buffer it shows and to the live sync, the effects
//! it cannot do itself (write the file, step to the next change mark).
//!
//! This is the subset the page's editor offers that is built here
//! (EDITOR-VIM-22 leaves which subset is promised open; this follows main as
//! far as it goes). Modes: normal, insert, replace, visual and visual-line.
//! Counts. Motions: `h j k l 0 ^ $ |`, `w W e E b B ge gE`, `gg G`,
//! `f F t T ; ,`, `{ }`, `%` and `+ - _`. Operators: `d y c > < g~ gu gU`,
//! doubled for lines, with motions and the objects `iw aw iW aW`, quotes,
//! brackets and `ip ap`. The shortcuts `x X D Y C s S ~`, and `r J gJ p P o O
//! a A i I gi gI R`; `.`, `u` and `Ctrl-r`; the registers `" 0-9 a-z A-Z - .
//! _ +`; `]c` and `[c`; and the ex commands `:w`, `:write`, `:u`, `:red`,
//! `:noh`, `:startinsert`, `:<N>` and `:d :y :pu :j` over a range; `Ctrl-a`
//! and `Ctrl-x` on a decimal number; `Ctrl-n` and `Ctrl-p` as `j` and `k`.
//! What is left out does nothing and says
//! nothing, like an unbound key: macros, marks, search, `H M L`, `z`
//! scrolling, `gc`, `=`, `g? gq gw`, `( )`, the `s` and `t` objects, `:s :g :v
//! :norm :sor :reg`. The chords Vim owns but that need the window (paging,
//! scroll, jump list, `Ctrl-v` block, `Ctrl-w`) are taken and do nothing.
//! Any other Ctrl chord goes back to the host unhandled.

pub mod buffer;
pub mod command;
pub mod ex;
pub mod insert;
pub mod key;
pub mod keymap;
pub mod motion;
pub mod normal;
pub mod number;
pub mod object;
pub mod operate;
pub mod paste;
pub mod range;
pub mod region;
pub mod registers;
pub mod rewrite;
pub mod text;
pub mod visual;
pub mod word;

use ropey::Rope;

use super::change::ChangeSet;
use buffer::Buffer;
use command::{Command, Parse};
use key::{Code, Key};
use motion::Find;
use registers::Registers;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Replace,
    Visual,
    VisualLine,
    /// The `:` line is being typed.
    Command,
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
/// began, if any.
#[derive(Debug, Clone)]
struct Dot {
    command: Command,
    typed: Vec<Key>,
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
}

pub struct Vim {
    buf: Buffer,
    mode: Mode,
    /// A byte on a character; in insert mode, the caret between characters.
    cursor: usize,
    /// The other end of the visual selection.
    anchor: usize,
    /// The column `j` and `k` aim for, in characters; `usize::MAX` after `$`.
    want: usize,
    pending: Vec<Key>,
    last_find: Option<(Find, char)>,
    dot: Option<Dot>,
    /// Keys are being added to `dot.typed`.
    recording: bool,
    replaying: bool,
    insert: Option<Insert>,
    last_visual: Option<(Mode, usize, usize)>,
    last_insert_end: Option<usize>,
    cmdline: String,
    message: Option<String>,
    effects: Vec<Effect>,
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
            last_find: None,
            dot: None,
            recording: false,
            replaying: false,
            insert: None,
            last_visual: None,
            last_insert_end: None,
            cmdline: String::new(),
            message: None,
            effects: Vec::new(),
        }
    }

    pub fn doc(&self) -> &Rope {
        self.buf.doc()
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// The cursor, as a byte of the text.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// The selected span in visual modes, `from..to` with the character at
    /// the far end included, whole lines in visual-line mode.
    pub fn selection(&self) -> Option<(usize, usize)> {
        let t = self.buf.text();
        match self.mode {
            Mode::Visual => {
                let r = region::of_selection(t, self.anchor, self.cursor, false);
                Some((r.from, r.to))
            }
            Mode::VisualLine => {
                let r = region::of_selection(t, self.anchor, self.cursor, true);
                Some((r.from, r.to))
            }
            _ => None,
        }
    }

    /// The keys of a command not yet finished, for the host to show.
    pub fn pending(&self) -> &[Key] {
        &self.pending
    }

    /// The `:` line as typed, while in command mode.
    pub fn command_line(&self) -> Option<&str> {
        (self.mode == Mode::Command).then_some(self.cmdline.as_str())
    }

    /// The line the last key left at the bottom, if any.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// The file is read-only (EDITOR-VIM-18): no edit is made and insert
    /// mode is not entered.
    pub fn set_read_only(&mut self, on: bool) {
        self.buf.set_read_only(on);
    }

    /// The cursor put where the pointer clicked.
    pub fn place(&mut self, at: usize) {
        self.pending.clear();
        self.cursor = self.buf.doc().floor_char_boundary(at.min(self.buf.doc().len()));
        if self.mode == Mode::Command {
            self.mode = Mode::Normal;
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

    pub fn key(&mut self, regs: &mut Registers, key: Key) -> Step {
        self.message = None;
        self.effects.clear();
        // Ctrl-[ and Ctrl-c are Escape.
        let key = if key.is_esc() { Key::of(Code::Esc) } else { key };
        let handled = match self.mode {
            Mode::Insert | Mode::Replace => self.insert_key(regs, key),
            Mode::Command => self.ex_key(regs, key),
            _ => self.command_key(regs, key),
        };
        if !matches!(self.mode, Mode::Insert | Mode::Replace) {
            self.buf.commit();
            self.clamp();
        }
        Step { handled, change: self.buf.take_step(), effects: std::mem::take(&mut self.effects) }
    }

    fn command_key(&mut self, regs: &mut Registers, key: Key) -> bool {
        self.pending.push(key);
        match command::parse(&self.pending, self.is_visual()) {
            Parse::Need => true,
            Parse::Bad => {
                // A chord or a key of the host's, met at the start of a
                // command, is the host's; any other key that begins nothing
                // is swallowed.
                let host = self.pending.len() == 1
                    && (key.ctrl || matches!(key.code, Code::Tab | Code::PageUp | Code::PageDown));
                self.pending.clear();
                !host
            }
            Parse::Done(cmd) => {
                self.pending.clear();
                self.run(regs, cmd);
                true
            }
        }
    }

    pub fn is_visual(&self) -> bool {
        matches!(self.mode, Mode::Visual | Mode::VisualLine)
    }

    /// Puts the cursor where the mode lets it rest: on a character, or on
    /// the empty line; the caret of insert mode may be at a line's end.
    fn clamp(&mut self) {
        let t = self.buf.text();
        self.cursor = self.cursor.min(t.len());
        self.anchor = self.anchor.min(t.len());
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
