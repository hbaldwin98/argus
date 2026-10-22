//! The questions Vim asks of the rope: which character is where, where a
//! line starts and ends, and what class a character is. Positions are bytes
//! on a character boundary; lines are split on `\n` alone, as CodeMirror
//! splits the buffer, so a text ending in a break has an empty last line.

use ropey::{LineType, Rope};

/// What a character is to a word motion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Space,
    Word,
    Punct,
}

#[derive(Clone, Copy)]
pub struct Text<'a>(pub &'a Rope);

impl<'a> Text<'a> {
    pub fn len(self) -> usize {
        self.0.len()
    }

    pub fn is_empty(self) -> bool {
        self.0.len() == 0
    }

    pub fn char_at(self, at: usize) -> Option<char> {
        (at < self.0.len()).then(|| self.0.char(at))
    }

    /// The character start after `at`, or the end of the text.
    pub fn next(self, at: usize) -> usize {
        match self.char_at(at) {
            Some(c) => at + c.len_utf8(),
            None => self.len(),
        }
    }

    /// The character start before `at`, or 0.
    pub fn prev(self, at: usize) -> usize {
        let mut p = at.min(self.len());
        if p == 0 {
            return 0;
        }
        p -= 1;
        while !self.0.is_char_boundary(p) {
            p -= 1;
        }
        p
    }

    pub fn line_count(self) -> usize {
        self.0.len_lines(LineType::LF)
    }

    pub fn line_of(self, at: usize) -> usize {
        self.0.byte_to_line_idx(at.min(self.len()), LineType::LF)
    }

    pub fn line_start(self, line: usize) -> usize {
        if line >= self.line_count() {
            return self.len();
        }
        self.0.line_to_byte_idx(line, LineType::LF)
    }

    /// Where the line's text ends: at its `\n`, or the end of the text.
    pub fn line_end(self, line: usize) -> usize {
        if line + 1 >= self.line_count() {
            return self.len();
        }
        self.line_start(line + 1) - 1
    }

    /// The line and its break: up to the start of the next line.
    pub fn line_span(self, line: usize) -> (usize, usize) {
        let next = if line + 1 >= self.line_count() { self.len() } else { self.line_start(line + 1) };
        (self.line_start(line), next)
    }

    pub fn last_line(self) -> usize {
        self.line_count() - 1
    }

    pub fn is_blank_line(self, line: usize) -> bool {
        self.line_start(line) == self.line_end(line)
    }

    /// The first character of the line that is not a space or tab, or the
    /// line's end if it has none.
    pub fn first_nonblank(self, line: usize) -> usize {
        let end = self.line_end(line);
        let mut p = self.line_start(line);
        while p < end && matches!(self.char_at(p), Some(' ' | '\t')) {
            p += 1;
        }
        p
    }

    /// The start of the line's last character, or the line's start if it
    /// is empty: the furthest a normal-mode cursor goes.
    pub fn last_char(self, line: usize) -> usize {
        let (start, end) = (self.line_start(line), self.line_end(line));
        if end == start {
            start
        } else {
            self.prev(end)
        }
    }

    /// How many characters `at` is from the start of its line.
    pub fn col(self, at: usize) -> usize {
        let start = self.line_start(self.line_of(at));
        self.0.slice(start..at).chars().count()
    }

    /// The `col`th character of the line, or the last one if it is shorter.
    pub fn at_col(self, line: usize, col: usize) -> usize {
        let (start, end) = (self.line_start(line), self.line_end(line));
        let mut p = start;
        for _ in 0..col {
            let n = self.next(p);
            if n >= end {
                return self.last_char(line);
            }
            p = n;
        }
        p.min(self.last_char(line).max(start))
    }

    /// The line and the column, in characters, of `at`.
    pub fn pos(self, at: usize) -> (usize, usize) {
        (self.line_of(at), self.col(at))
    }

    /// The byte at a line and a column in characters; a column past the
    /// line's end is the end of the line, which is not on a character.
    pub fn at(self, line: usize, col: usize) -> usize {
        let line = line.min(self.last_line());
        let (start, end) = (self.line_start(line), self.line_end(line));
        let mut p = start;
        for _ in 0..col {
            if p >= end {
                break;
            }
            p = self.next(p);
        }
        p.min(end)
    }

    /// The line's text without its break.
    pub fn line_text(self, line: usize) -> String {
        if line > self.last_line() {
            return String::new();
        }
        self.slice(self.line_start(line), self.line_end(line))
    }

    /// How many characters the line has.
    pub fn line_len(self, line: usize) -> usize {
        if line > self.last_line() {
            return 0;
        }
        self.0.slice(self.line_start(line)..self.line_end(line)).chars().count()
    }

    pub fn slice(self, from: usize, to: usize) -> String {
        self.0.slice(from..to).to_string()
    }

    /// A character's class; with `big` anything but space is one class.
    pub fn class(c: char, big: bool) -> Class {
        if c.is_whitespace() {
            Class::Space
        } else if big || c.is_alphanumeric() || c == '_' {
            Class::Word
        } else {
            Class::Punct
        }
    }

    /// The class at `at`; the end of the text is space.
    pub fn class_at(self, at: usize, big: bool) -> Class {
        self.char_at(at).map_or(Class::Space, |c| Text::class(c, big))
    }
}

#[cfg(test)]
mod tests;
