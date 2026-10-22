//! The line typed after `:`, `/` or `?`: a text with a caret that the keys
//! edit, Up and Down walking the history of lines, Enter running it and
//! Escape (or a Backspace on an empty line) giving it up. The page draws
//! it with an input in the library's panel; here the host draws `Vim::prompt`.

use super::command::Command;
use super::key::{Code, Key};
use super::{Mode, Shared, Vim};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Ex,
    Search { forward: bool },
}

/// What is done with the line when it is run.
#[derive(Debug, Clone, Copy)]
pub enum Then {
    /// An ex command.
    Run,
    /// A search that moves the cursor, or extends the selection.
    Move { count: Option<usize> },
    /// A search that an operator waits for: `d/foo`.
    Operate(Command),
}

#[derive(Debug, Clone)]
pub struct Prompt {
    pub kind: Kind,
    pub text: String,
    /// The caret, in characters.
    pub caret: usize,
    pub then: Then,
}

impl Prompt {
    pub fn prefix(&self) -> char {
        match self.kind {
            Kind::Ex => ':',
            Kind::Search { forward: true } => '/',
            Kind::Search { forward: false } => '?',
        }
    }

    fn len(&self) -> usize {
        self.text.chars().count()
    }

    fn byte(&self, caret: usize) -> usize {
        self.text.char_indices().nth(caret).map_or(self.text.len(), |(i, _)| i)
    }
}

impl Vim {
    /// Opens the `:` line with `text` already in it (a range, for a count or
    /// a selection).
    pub(super) fn open_ex(&mut self, text: &str) {
        let caret = text.chars().count();
        self.prompt = Some(Prompt { kind: Kind::Ex, text: text.to_owned(), caret, then: Then::Run });
        self.mode = Mode::Command;
    }

    /// Opens the `/` or `?` line, from normal or visual mode, which it
    /// returns to.
    pub(super) fn open_search(&mut self, sh: &mut Shared, forward: bool, then: Then) {
        sh.search.reversed = !forward;
        self.back_to = if self.is_visual() { self.mode } else { Mode::Normal };
        self.prompt = Some(Prompt { kind: Kind::Search { forward }, text: String::new(), caret: 0, then });
        self.mode = Mode::Search;
    }

    pub(super) fn prompt_key(&mut self, sh: &mut Shared, key: Key) -> bool {
        let Some(p) = self.prompt.as_mut() else {
            self.mode = Mode::Normal;
            return false;
        };
        let recall = match p.kind {
            Kind::Ex => &mut sh.ex_history,
            Kind::Search { .. } => &mut sh.search_history,
        };
        let walking = matches!(key.code, Code::Left | Code::Right | Code::Up | Code::Down) && !key.ctrl;
        match (key.code, key.ctrl) {
            (Code::Esc, _) | (Code::Backspace, false) if key.code == Code::Esc || p.text.is_empty() => {
                recall.push(&p.text);
                recall.reset();
                let to = if matches!(p.kind, Kind::Search { .. }) { self.back_to } else { Mode::Normal };
                self.prompt = None;
                self.mode = to;
                return true;
            }
            (Code::Enter, false) => {
                let p = self.prompt.take().expect("the prompt is open");
                let recall = match p.kind {
                    Kind::Ex => &mut sh.ex_history,
                    Kind::Search { .. } => &mut sh.search_history,
                };
                recall.push(&p.text);
                recall.reset();
                match p.kind {
                    Kind::Ex => {
                        self.mode = Mode::Normal;
                        self.ex_run(sh, &p.text);
                        sh.search.highlight = false;
                    }
                    Kind::Search { .. } => {
                        self.mode = self.back_to;
                        self.submit_search(sh, &p.text, p.then);
                    }
                }
                return true;
            }
            (Code::Up | Code::Down, false) => {
                let text = recall.next_match(&p.text, key.code == Code::Up);
                p.text = text;
                p.caret = p.caret.min(p.len());
            }
            (Code::Char('u'), true) => {
                p.text.clear();
                p.caret = 0;
            }
            (Code::Left, false) => p.caret = p.caret.saturating_sub(1),
            (Code::Right, false) => p.caret = (p.caret + 1).min(p.len()),
            (Code::Home, false) => p.caret = 0,
            (Code::End, false) => p.caret = p.len(),
            (Code::Backspace, false) => {
                if p.caret > 0 {
                    let (from, to) = (p.byte(p.caret - 1), p.byte(p.caret));
                    p.text.replace_range(from..to, "");
                    p.caret -= 1;
                }
            }
            (Code::Delete, false) => {
                if p.caret < p.len() {
                    let (from, to) = (p.byte(p.caret), p.byte(p.caret + 1));
                    p.text.replace_range(from..to, "");
                }
            }
            (Code::Char(c), false) => {
                let at = p.byte(p.caret);
                p.text.insert(at, c);
                p.caret += 1;
            }
            _ => return false,
        }
        if !walking {
            recall.reset();
        }
        true
    }
}

#[cfg(test)]
mod tests;
