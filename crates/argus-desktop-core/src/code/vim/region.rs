//! The text an operator acts on: a motion's reach, an object, or the
//! selection, made into one span of the rope, whole lines or characters.
//! `from..to`, `to` excluded; whole lines run to the start of the next.

use super::motion::Reach;
use super::object::Span;
use super::text::Text;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub from: usize,
    pub to: usize,
    pub linewise: bool,
}

impl Region {
    /// The first and last line the region touches.
    pub fn lines(self, t: Text) -> (usize, usize) {
        let first = t.line_of(self.from);
        let last = if self.to > self.from { t.line_of(t.prev(self.to)) } else { first };
        (first, last.max(first))
    }
}

pub fn of_lines(t: Text, first: usize, last: usize) -> Region {
    Region { from: t.line_start(first), to: t.line_span(last).1, linewise: true }
}

pub fn of_span(span: Span) -> Region {
    Region { from: span.from, to: span.to, linewise: span.linewise }
}

/// The selection from `anchor` to `cursor`, both characters of it.
pub fn of_selection(t: Text, anchor: usize, cursor: usize, linewise: bool) -> Region {
    let (a, b) = (anchor.min(cursor), anchor.max(cursor));
    if linewise {
        return of_lines(t, t.line_of(a), t.line_of(b));
    }
    Region { from: a, to: t.next(b), linewise: false }
}

/// What a motion from `cursor` covers. An exclusive motion that ends at the
/// start of a later line stops short of the break, and if it began at or
/// before the first non-blank takes whole lines instead (Vim's `o_v` rule):
/// `dw` on a line's last word leaves the line break, `d}` leaves the blank.
pub fn of_reach(t: Text, cursor: usize, reach: Reach) -> Region {
    let (a, b) = (cursor.min(reach.to), cursor.max(reach.to));
    let (la, lb) = (t.line_of(a), t.line_of(b));
    if reach.linewise {
        return of_lines(t, la, lb);
    }
    if reach.inclusive {
        let to = if matches!(t.char_at(b), None | Some('\n')) { b } else { t.next(b) };
        return Region { from: a, to, linewise: false };
    }
    if b > a && lb > la && b == t.line_start(lb) {
        if a <= t.first_nonblank(la) {
            return of_lines(t, la, lb - 1);
        }
        return Region { from: a, to: t.line_end(lb - 1), linewise: false };
    }
    Region { from: a, to: b, linewise: false }
}

#[cfg(test)]
mod tests;
