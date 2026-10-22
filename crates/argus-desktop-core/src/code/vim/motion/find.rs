//! The searches of a motion over the text alone: the bracket `%` matches,
//! the character `f` and `t` look for, where a paragraph ends, and the first
//! character `^` and `gg` land on.

use super::super::text::Text;
use super::Find;

pub(super) fn matching(t: Text, from: usize) -> Option<usize> {
    let line = t.line_of(from);
    let end = t.line_end(line);
    let mut p = from;
    let (open, dir, close) = loop {
        let c = t.char_at(p).filter(|_| p < end)?;
        match c {
            '(' => break ('(', true, ')'),
            '[' => break ('[', true, ']'),
            '{' => break ('{', true, '}'),
            ')' => break (')', false, '('),
            ']' => break (']', false, '['),
            '}' => break ('}', false, '{'),
            _ => p = t.next(p),
        }
    };
    let mut depth = 0usize;
    let mut q = p;
    loop {
        let c = t.char_at(q)?;
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some(q);
            }
        }
        if dir {
            q = t.next(q);
            if q >= t.len() {
                return None;
            }
        } else {
            if q == 0 {
                return None;
            }
            q = t.prev(q);
        }
    }
}

pub(super) fn find(t: Text, from: usize, kind: Find, ch: char, count: usize, repeat: bool) -> Option<usize> {
    let line = t.line_of(from);
    let (start, end) = (t.line_start(line), t.line_end(line));
    let forward = matches!(kind, Find::To | Find::Till);
    let till = matches!(kind, Find::Till | Find::TillBack);
    let mut p = from;
    // `;` after `t` must not stop at the character it is already against.
    let mut first = true;
    for _ in 0..count {
        loop {
            if forward {
                p = t.next(p);
                if p >= end {
                    return None;
                }
            } else {
                if p <= start {
                    return None;
                }
                p = t.prev(p);
            }
            let skip = till && repeat && first && {
                let near = if forward { t.next(from) } else { t.prev(from) };
                near == p
            };
            if t.char_at(p) == Some(ch) && !skip {
                break;
            }
            first = false;
        }
        first = false;
    }
    Some(match (till, forward) {
        (false, _) => p,
        (true, true) => t.prev(p),
        (true, false) => t.next(p),
    })
}

pub(super) fn paragraph(t: Text, from: usize, count: usize, forward: bool) -> usize {
    let mut line = t.line_of(from);
    for _ in 0..count {
        // Past the blank lines the paragraph starts on, then to the next one.
        while t.is_blank_line(line) && (if forward { line < t.last_line() } else { line > 0 }) {
            line = if forward { line + 1 } else { line - 1 };
        }
        loop {
            if forward {
                if line >= t.last_line() {
                    return t.len();
                }
                line += 1;
            } else {
                if line == 0 {
                    return 0;
                }
                line -= 1;
            }
            if t.is_blank_line(line) {
                break;
            }
        }
    }
    t.line_start(line)
}

/// Where `^` and `gg` land: the first character that is not blank, or the
/// last one on a line of blanks.
pub(super) fn first_or_last(t: Text, line: usize) -> usize {
    let p = t.first_nonblank(line);
    if p >= t.line_end(line) {
        t.last_char(line)
    } else {
        p
    }
}
