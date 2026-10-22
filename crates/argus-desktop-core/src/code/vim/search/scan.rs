//! Finding matches in the text: the library's search cursor, which finds
//! the next match after the last one or, going back, the last one that
//! ends before the last one began, and `n`'s walk over it, which skips the
//! match the cursor is on and wraps round the text's ends.

use super::pattern::Query;

/// A cursor over the matches of one query in one text.
pub struct Cursor<'a> {
    query: &'a Query,
    text: &'a str,
    /// Where it was put, until the first match is found.
    at: usize,
    m: Option<(usize, usize)>,
}

impl<'a> Cursor<'a> {
    pub fn new(query: &'a Query, text: &'a str, at: usize) -> Cursor<'a> {
        Cursor { query, text, at: at.min(text.len()), m: None }
    }

    /// The match found last.
    pub fn found(&self) -> Option<(usize, usize)> {
        self.m
    }

    fn step_past(&self, at: usize) -> Option<usize> {
        let mut p = at + 1;
        while p < self.text.len() && !self.text.is_char_boundary(p) {
            p += 1;
        }
        (at < self.text.len()).then_some(p.min(self.text.len()))
    }

    /// The next match, or the previous one if `backward`; false if there is
    /// none, and the cursor stays where it was.
    pub fn find(&mut self, backward: bool) -> bool {
        let found = if backward { self.before() } else { self.after() };
        match found {
            Some(m) => {
                self.m = Some(m);
                true
            }
            None => false,
        }
    }

    fn after(&self) -> Option<(usize, usize)> {
        let start = match self.m {
            Some((s, e)) if s == e => self.step_past(e)?,
            Some((_, e)) => e,
            None => self.at,
        };
        let m = self.query.regex.find_from_pos(self.text, start).ok()??;
        Some((m.start(), m.end()))
    }

    fn before(&self) -> Option<(usize, usize)> {
        let limit = self.m.map_or(self.at, |(s, _)| s);
        let mut at = 0;
        let mut best = None;
        while let Ok(Some(m)) = self.query.regex.find_from_pos(self.text, at) {
            if m.end() > limit {
                break;
            }
            if self.m != Some((m.start(), m.end())) {
                best = Some((m.start(), m.end()));
            }
            at = if m.end() == m.start() {
                match self.step_past(m.end()) {
                    Some(p) => p,
                    None => break,
                }
            } else {
                m.end()
            };
        }
        best
    }
}

/// Whether `at` is at the end of its line.
fn at_line_end(text: &str, at: usize) -> bool {
    at >= text.len() || text.as_bytes()[at] == b'\n'
}

/// `n` and `N`: where the `repeat`th match from `pos` is, going back if
/// `prev`. The match the cursor stands at the start of is passed over, and
/// the walk wraps past the text's end; none if the text has no match.
pub fn find_next(query: &Query, text: &str, pos: usize, prev: bool, repeat: usize) -> Option<usize> {
    let mut cursor = Cursor::new(query, text, pos);
    for i in 0..repeat.max(1) {
        let mut found = cursor.find(prev);
        if i == 0 && found && cursor.m.is_some_and(|(s, _)| s == pos) {
            let (s, e) = cursor.m.expect("found");
            let last_end = if prev { s } else { e };
            found = cursor.find(prev);
            if found {
                let (s2, e2) = cursor.m.expect("found");
                if s2 == e2 && s2 == last_end && at_line_end(text, last_end) {
                    found = cursor.find(prev);
                }
            }
        }
        if !found {
            cursor = Cursor::new(query, text, if prev { text.len() } else { 0 });
            if !cursor.find(prev) {
                return None;
            }
        }
    }
    cursor.m.map(|(s, _)| s)
}

/// `gn` and `gN`: the match the cursor is in, or else the next one, whole.
/// In visual mode the match the cursor is at the start of counts as passed.
pub fn find_next_inclusive(
    query: &Query,
    text: &str,
    pos: usize,
    prev: bool,
    repeat: usize,
    visual: bool,
) -> Option<(usize, usize)> {
    let mut cursor = Cursor::new(query, text, pos);
    // One result back, so a match the cursor is in is the one found.
    let found = cursor.find(!prev);
    if !visual && found && cursor.m.is_some_and(|(s, _)| s == pos) {
        cursor.find(!prev);
    }
    for _ in 0..repeat.max(1) {
        if !cursor.find(prev) {
            cursor = Cursor::new(query, text, if prev { text.len() } else { 0 });
            if !cursor.find(prev) {
                return None;
            }
        }
    }
    cursor.m
}

/// Every match, in order, for highlighting and for `:s`.
pub fn all(query: &Query, text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut cursor = Cursor::new(query, text, 0);
    while cursor.find(false) {
        out.push(cursor.m.expect("found"));
    }
    out
}

#[cfg(test)]
mod tests;
