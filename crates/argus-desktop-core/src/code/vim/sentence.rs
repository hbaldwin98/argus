//! Sentences: where `(` and `)` go, and what `is` and `as` take. A port of
//! the library's two walks, which differ: the motion crosses line breaks
//! and stops at empty lines, while the object stays on its line. A sentence
//! ends at `.`, `!` or `?`.

use super::text::Text;

/// A line and a column in characters.
pub type At = (usize, usize);

fn is_end(c: Option<char>) -> bool {
    matches!(c, Some('.' | '?' | '!'))
}

fn is_space(c: Option<char>) -> bool {
    c.is_some_and(char::is_whitespace)
}

/// A walker over the lines: the line it is on as characters, and where.
struct Walk<'a> {
    t: Text<'a>,
    line: Option<Vec<char>>,
    ln: isize,
    pos: isize,
    dir: isize,
}

impl<'a> Walk<'a> {
    fn new(t: Text<'a>, (ln, pos): At, dir: isize) -> Walk<'a> {
        Walk { t, line: Some(t.line_text(ln).chars().collect()), ln: ln as isize, pos: pos as isize, dir }
    }

    fn ch(&self) -> Option<char> {
        let line = self.line.as_ref()?;
        usize::try_from(self.pos).ok().and_then(|p| line.get(p)).copied()
    }

    fn ch_at(&self, offset: isize) -> Option<char> {
        let line = self.line.as_ref()?;
        usize::try_from(self.pos + offset).ok().and_then(|p| line.get(p)).copied()
    }

    fn empty(&self) -> bool {
        self.line.as_ref().is_some_and(Vec::is_empty)
    }

    fn at(&self) -> At {
        (self.ln.max(0) as usize, self.pos.max(0) as usize)
    }

    /// The next place along, over line breaks; no line at the text's end.
    fn step_across(&mut self) {
        let Some(line) = &self.line else { return };
        if self.pos + self.dir < 0 || self.pos + self.dir >= line.len() as isize {
            self.ln += self.dir;
            if self.ln < 0 || self.ln as usize > self.t.last_line() {
                self.line = None;
                return;
            }
            let line: Vec<char> = self.t.line_text(self.ln as usize).chars().collect();
            self.pos = if self.dir > 0 { 0 } else { line.len() as isize - 1 };
            self.line = Some(line);
        } else {
            self.pos += self.dir;
        }
    }

    /// The next place along, on this line; no line at its end.
    fn step_in(&mut self) {
        let Some(line) = &self.line else { return };
        if self.pos + self.dir < 0 || self.pos + self.dir >= line.len() as isize {
            self.line = None;
        } else {
            self.pos += self.dir;
        }
    }
}

fn last_non_space(t: Text, ln: usize) -> usize {
    let line: Vec<char> = t.line_text(ln).chars().collect();
    line.iter().rposition(|c| !c.is_whitespace()).unwrap_or(0)
}

fn first_non_space(t: Text, ln: usize) -> usize {
    t.line_text(ln).chars().position(|c| !c.is_whitespace()).unwrap_or(0)
}

fn forward(t: Text, from: At) -> At {
    let mut w = Walk::new(t, from, 1);
    let mut stop = w.empty();
    let skip_empty = w.empty();
    let mut last = from;
    w.step_across();
    while w.line.is_some() {
        last = w.at();
        if (w.empty() && !skip_empty) || (stop && !w.empty() && !is_space(w.ch())) {
            return w.at();
        } else if is_end(w.ch()) && !stop {
            let len = w.line.as_ref().map_or(0, Vec::len) as isize;
            if w.pos == len - 1 || is_space(w.ch_at(1)) {
                stop = true;
            }
        }
        w.step_across();
    }
    // The end of the text: the last character that is not blank.
    (last.0, last_non_space(t, last.0))
}

fn reverse(t: Text, from: At) -> At {
    let mut w = Walk::new(t, from, -1);
    let mut last: Option<At> = None;
    let mut skip_empty = w.empty();
    let start_ln = from.0;
    w.step_across();
    while w.line.is_some() {
        if w.empty() && !skip_empty {
            return last.unwrap_or_else(|| w.at());
        } else if is_end(w.ch()) && last.is_some_and(|(l, p)| !(w.ln as usize == l && w.pos as usize + 1 == p)) {
            return last.expect("checked");
        } else if !w.empty() && !is_space(w.ch()) {
            skip_empty = false;
            last = Some(w.at());
        }
        w.step_across();
    }
    // The start of the text: the first character that is not blank.
    let ln = last.map_or(start_ln, |(l, _)| l);
    (ln, first_non_space(t, ln))
}

/// `(` and `)`: where the `count`th sentence boundary from `from` is.
pub fn find(t: Text, from: At, count: usize, forward_dir: bool) -> At {
    let mut at = from;
    for _ in 0..count.max(1) {
        at = if forward_dir { forward(t, at) } else { reverse(t, at) };
    }
    at
}

fn end_of(t: Text, from: At, inclusive: bool) -> At {
    let mut w = Walk::new(t, from, 1);
    if w.empty() {
        return from;
    }
    let mut last = from.1 as isize;
    w.step_in();
    while w.line.is_some() {
        last = w.pos;
        if is_end(w.ch()) {
            if !inclusive {
                return (w.ln as usize, w.pos as usize + 1);
            }
            w.step_in();
            while w.line.is_some() {
                if is_space(w.ch()) {
                    last = w.pos;
                    w.step_in();
                } else {
                    break;
                }
            }
            return (w.ln as usize, (last + 1) as usize);
        }
        w.step_in();
    }
    (w.ln as usize, (last + 1) as usize)
}

fn start_of(t: Text, from: At, inclusive: bool) -> At {
    let mut w = Walk::new(t, from, -1);
    if w.empty() {
        return from;
    }
    let mut last = from.1 as isize;
    w.step_in();
    while w.line.is_some() {
        if !is_space(w.ch()) && !is_end(w.ch()) {
            last = w.pos;
        } else if is_end(w.ch()) {
            if !inclusive {
                return (w.ln as usize, last as usize);
            }
            if is_space(w.ch_at(1)) {
                return (w.ln as usize, w.pos as usize + 1);
            }
            return (w.ln as usize, last as usize);
        }
        w.step_in();
    }
    // The walk ran off the line's start: the line is whole again.
    let line: Vec<char> = t.line_text(w.ln as usize).chars().collect();
    let at = usize::try_from(w.pos).ok().and_then(|p| line.get(p)).copied();
    if inclusive && is_space(at) {
        (w.ln as usize, w.pos.max(0) as usize)
    } else {
        (w.ln as usize, last.max(0) as usize)
    }
}

/// `is` and `as` at `at`, `count` sentences: from and to, in characters,
/// `to` excluded. `a` takes the blank after, or the one before when there
/// is none after.
pub fn object(t: Text, at: At, count: usize, inclusive: bool) -> (At, At) {
    let mut head = at;
    let line: Vec<char> = t.line_text(head.0).chars().collect();
    if head.1 > 0 && is_end(line.get(head.1).copied()) {
        head.1 -= 1;
    }
    let (mut end, mut start) = (head, head);
    for _ in 0..count.max(1) {
        end = end_of(t, end, inclusive);
    }
    for _ in 0..count.max(1) {
        start = start_of(t, start, inclusive);
    }
    let start_line: Vec<char> = t.line_text(start.0).chars().collect();
    let end_line: Vec<char> = t.line_text(end.0).chars().collect();
    let before = start_line.get(start.1).copied();
    let after = end.1.checked_sub(1).and_then(|p| end_line.get(p)).copied();
    if is_space(before) && is_space(after) {
        start.1 += 1;
    }
    (start, end)
}

#[cfg(test)]
mod tests;
