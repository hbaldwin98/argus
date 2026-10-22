//! Where `w`, `e`, `b` and `ge` go, one step of each. A word is a run of
//! one class, a line break is space, and an empty line is a word of its own
//! that a motion stops on.

use super::text::{Class, Text};

fn empty_line_at(t: Text, at: usize) -> bool {
    t.char_at(at) == Some('\n') && (at == 0 || t.char_at(t.prev(at)) == Some('\n'))
        || at == t.len() && at > 0 && t.char_at(t.prev(at)) == Some('\n')
}

/// The start of the next word.
pub fn forward(t: Text, from: usize, big: bool) -> usize {
    let mut p = from;
    let c = t.class_at(p, big);
    if c != Class::Space {
        while p < t.len() && t.class_at(p, big) == c {
            p = t.next(p);
        }
    }
    while p < t.len() && t.class_at(p, big) == Class::Space {
        let was_break = t.char_at(p) == Some('\n');
        p = t.next(p);
        if was_break && (t.char_at(p) == Some('\n') || p == t.len()) {
            break;
        }
    }
    p
}

/// The end of this word, or of the next if already on one's end.
pub fn end(t: Text, from: usize, big: bool) -> usize {
    let mut p = t.next(from);
    while p < t.len() && t.class_at(p, big) == Class::Space {
        p = t.next(p);
    }
    if p >= t.len() {
        return t.prev(t.len());
    }
    let c = t.class_at(p, big);
    loop {
        let n = t.next(p);
        if n >= t.len() || t.class_at(n, big) != c {
            return p;
        }
        p = n;
    }
}

/// The start of this word, or of the one before if already on a start.
pub fn back(t: Text, from: usize, big: bool) -> usize {
    if from == 0 {
        return 0;
    }
    let mut p = t.prev(from);
    while p > 0 && t.class_at(p, big) == Class::Space && !empty_line_at(t, p) {
        p = t.prev(p);
    }
    if t.class_at(p, big) == Class::Space {
        return p;
    }
    let c = t.class_at(p, big);
    while p > 0 {
        let q = t.prev(p);
        if t.class_at(q, big) != c {
            break;
        }
        p = q;
    }
    p
}

/// The end of the word before this one.
pub fn end_back(t: Text, from: usize, big: bool) -> usize {
    let mut p = from;
    let c = t.class_at(p, big);
    if c != Class::Space {
        while p > 0 && t.class_at(p, big) == c {
            p = t.prev(p);
        }
        if t.class_at(p, big) == c {
            return 0;
        }
    } else if p > 0 {
        p = t.prev(p);
    }
    while p > 0 && t.class_at(p, big) == Class::Space && !empty_line_at(t, p) {
        p = t.prev(p);
    }
    p
}

#[cfg(test)]
mod tests;
