//! Vim driven through its keys, the way the host drives it. A text is
//! written with `|` where the cursor is, and read back the same way.

use ropey::Rope;

use super::key::{self, Key};
use super::shared::Shared;
use super::{Effect, Mode, Step, Vim};

mod block;
mod chords;
mod dot;
mod ex;
mod excommands;
mod insert;
mod insertion;
mod keys;
mod macros;
mod marks;
mod motions;
mod normal;
mod numbers;
mod objects;
mod operators;
mod properties;
mod registers;
mod scripts;
mod scroll;
mod search;
mod visual;

pub struct Ed {
    pub vim: Vim,
    pub sh: Shared,
    /// Everything the effects asked of the host.
    pub effects: Vec<Effect>,
    /// The first text, and every change told to the host, so a test can
    /// check the host's copy stays the buffer.
    pub host: Rope,
}

impl Ed {
    /// An editor on `marked`, whose `|` is the cursor (the start if none).
    pub fn new(marked: &str) -> Ed {
        let (text, at) = match marked.find('|') {
            Some(at) => (format!("{}{}", &marked[..at], &marked[at + 1..]), at),
            None => (marked.to_owned(), 0),
        };
        let mut vim = Vim::new(&text);
        vim.place(at);
        Ed { host: Rope::from_str(&text), vim, sh: Shared::default(), effects: Vec::new() }
    }

    pub fn with_shared(marked: &str, sh: Shared) -> Ed {
        Ed { sh, ..Ed::new(marked) }
    }

    /// Types `notation` (`key::parse`'s) key by key; the last step.
    pub fn keys(&mut self, notation: &str) -> Step {
        let mut last = Step::default();
        for k in key::parse(notation) {
            last = self.key(k);
        }
        last
    }

    pub fn key(&mut self, k: Key) -> Step {
        let step = self.vim.key(&mut self.sh, k);
        if let Some(change) = &step.change {
            change.apply_in_place(&mut self.host);
        }
        self.effects.extend(step.effects.iter().cloned());
        assert_eq!(self.host.to_string(), self.vim.doc().to_string(), "the changes told are the edits made, after {k:?}");
        step
    }

    /// The window shows lines `top` to `bottom`.
    pub fn view(&mut self, top: usize, bottom: usize) {
        self.vim.set_view(top, bottom);
    }

    pub fn text(&self) -> String {
        self.vim.doc().to_string()
    }

    /// The text with `|` at the cursor.
    pub fn shows(&self) -> String {
        let mut s = self.text();
        s.insert(self.vim.cursor(), '|');
        s
    }

    pub fn mode(&self) -> Mode {
        self.vim.mode()
    }
}

/// Starts on `marked`, types `notation`, and says what the editor shows.
pub fn after(marked: &str, notation: &str) -> String {
    let mut ed = Ed::new(marked);
    ed.keys(notation);
    ed.shows()
}

/// `(start, keys, shown)`: each must come out as written.
pub fn table(rows: &[(&str, &str, &str)]) {
    for (start, notation, want) in rows {
        assert_eq!(&after(start, notation), want, "{start:?} then {notation:?}");
    }
}
