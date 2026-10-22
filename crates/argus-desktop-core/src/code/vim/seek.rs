//! What `/`, `?`, `*`, `#`, `n` and `gn` do to the cursor: run the line
//! typed, make the word under the cursor the pattern, and take the next
//! match into a selection or an operator. The patterns and the walk over the
//! matches are `search`'s; this is where they meet the cursor.

use super::command::{Action, Command, Target};
use super::motion::Motion;
use super::prompt::Then;
use super::region::Region;
use super::search::{pattern, scan};
use super::text::Text;
use super::{Mode, Shared, Vim};

/// A word character, as the library counts one: a letter, a digit or `_`.
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The word at or after `col` on the line, as the library finds it for `*`:
/// the first word character at or after the cursor and the run it is in; or
/// with `words_only` off, the first character that is not blank and its run.
/// The bytes it covers, and whether it is a word of word characters.
fn under_cursor(t: Text, at: usize, words_only: bool) -> Option<(usize, usize, bool)> {
    let line = t.line_of(at);
    let chars: Vec<char> = t.line_text(line).chars().collect();
    let mut idx = t.col(at);
    let seek = |c: char| if words_only { is_word(c) } else { !c.is_whitespace() };
    while !chars.get(idx).is_some_and(|c| seek(*c)) {
        idx += 1;
        if idx >= chars.len() {
            return None;
        }
    }
    let keyword = is_word(chars[idx]);
    let test = |c: char| if keyword { is_word(c) } else { !is_word(c) && !c.is_whitespace() };
    let (mut start, mut end) = (idx, idx);
    while start > 0 && test(chars[start - 1]) {
        start -= 1;
    }
    while end < chars.len() && test(chars[end]) {
        end += 1;
    }
    Some((t.at(line, start), t.at(line, end), keyword))
}

impl Vim {
    pub(super) fn pcre(&self, sh: &Shared) -> bool {
        self.locals.pcre.unwrap_or(sh.options.pcre)
    }

    /// The line typed after `/` or `?` was run: it is the pattern, and the
    /// cursor goes to the next match (or the operator that waited takes
    /// the text up to it).
    pub(super) fn submit_search(&mut self, sh: &mut Shared, raw: &str, then: Then) {
        let pcre = self.pcre(sh);
        // A search that is typed goes by capitals; one a macro plays does not.
        if let Err(e) = sh.search.update(&mut sh.regs, raw, true, !sh.macros.playing(), pcre) {
            self.message = Some(e);
            return;
        }
        let motion = Motion::SearchNext { forward: true };
        match then {
            Then::Move { count } => self.move_by(sh, motion, count),
            Then::Operate(cmd) => {
                if let Action::Operate { op, .. } = cmd.action {
                    let action = Action::Operate { op, target: Target::Motion(motion) };
                    self.run(sh, Command { action, ..cmd });
                }
            }
            Then::Run => {}
        }
    }

    /// `*`, `#`, `g*` and `g#`: the word under the cursor, or the first
    /// after it on the line, is made the pattern, and the cursor goes to its
    /// start; the motion that then looks for the next match. None, with a
    /// message, if the line has no word.
    pub(super) fn star(&mut self, sh: &mut Shared, forward: bool, whole: bool) -> Option<Motion> {
        let t = self.buf.text();
        let found = under_cursor(t, self.cursor, true).or_else(|| under_cursor(t, self.cursor, false));
        let Some((from, to, keyword)) = found else {
            self.message = Some("No word under cursor".into());
            return None;
        };
        let word = t.slice(from, to);
        let query = if keyword && whole { format!("\\b{word}\\b") } else { pattern::escape(&word) };
        sh.search.reversed = !forward;
        let pcre = self.pcre(sh);
        if let Err(e) = sh.search.update(&mut sh.regs, &query, true, false, pcre) {
            self.message = Some(e);
            return None;
        }
        self.jump_from = Some(self.cursor);
        self.cursor = from;
        Some(Motion::SearchNext { forward: true })
    }

    /// The match `gn` or `gN` stands for, `count` on: the one the cursor is
    /// in, or the next, as a region of the text.
    pub(super) fn match_region(&self, sh: &Shared, forward: bool, count: usize) -> Option<Region> {
        let query = sh.search.query.as_ref()?;
        let prev = if sh.search.reversed { forward } else { !forward };
        let text = self.buf.doc().to_string();
        let (from, to) = scan::find_next_inclusive(query, &text, self.cursor, prev, count, false)?;
        Some(Region { from, to, linewise: false })
    }

    /// `gn` and `gN` with no operator: the match is selected, or the
    /// selection is stretched over it.
    pub(super) fn select_match(&mut self, sh: &Shared, forward: bool, count: usize) {
        let Some(query) = sh.search.query.as_ref() else { return };
        let prev = if sh.search.reversed { forward } else { !forward };
        let text = self.buf.doc().to_string();
        let visual = self.is_visual();
        let Some((from, to)) = scan::find_next_inclusive(query, &text, self.cursor, prev, count, visual) else {
            return;
        };
        let t = self.buf.text();
        let last = if to > from { t.prev(to) } else { from };
        if visual {
            if matches!(self.mode, Mode::VisualLine | Mode::VisualBlock) {
                self.mode = Mode::Visual;
            }
            self.cursor = match (sh.search.reversed, forward) {
                (true, true) | (false, false) => from,
                _ => last,
            };
        } else {
            self.mode = Mode::Visual;
            (self.anchor, self.cursor) = if prev { (last, from) } else { (from, last) };
        }
        self.want = self.buf.text().col(self.cursor);
    }
}

#[cfg(test)]
mod tests;
