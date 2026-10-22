//! The text an `iw`, `a"`, `i(` or `ap` stands for around a position: a
//! word, a quoted string, what a pair of brackets holds, a paragraph. The
//! span is `from..to`, `to` excluded. A pair with nothing enclosing the
//! position is looked for ahead, as the library does (EDITOR-VIM-11).

use super::text::{Class, Text};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Object {
    Word { big: bool },
    Quote(char),
    Pair { open: char, close: char },
    Paragraph,
}

impl Object {
    /// The object a key after `i` or `a` names.
    pub fn named(key: char) -> Option<Object> {
        Some(match key {
            'w' => Object::Word { big: false },
            'W' => Object::Word { big: true },
            '"' | '\'' | '`' => Object::Quote(key),
            '(' | ')' | 'b' => Object::Pair { open: '(', close: ')' },
            '{' | '}' | 'B' => Object::Pair { open: '{', close: '}' },
            '[' | ']' => Object::Pair { open: '[', close: ']' },
            '<' | '>' => Object::Pair { open: '<', close: '>' },
            'p' => Object::Paragraph,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub from: usize,
    pub to: usize,
    pub linewise: bool,
}

fn chars(from: usize, to: usize) -> Span {
    Span { from, to, linewise: false }
}

/// The span of `object` at `at`, `inner` for `i` and not for `a`, `count`
/// of them.
pub fn span(object: Object, inner: bool, t: Text, at: usize, count: usize) -> Option<Span> {
    let count = count.max(1);
    match object {
        Object::Word { big } => word(t, at, big, inner, count),
        Object::Quote(q) => quote(t, at, q, inner),
        Object::Pair { open, close } => pair(t, at, open, close, inner, count),
        Object::Paragraph => paragraph(t, at, inner, count),
    }
}

/// The run of one class `at` is in, within its line.
fn run(t: Text, at: usize, big: bool) -> (usize, usize) {
    let line = t.line_of(at);
    let (ls, le) = (t.line_start(line), t.line_end(line));
    let c = t.class_at(at, big);
    let (mut s, mut e) = (at, t.next(at).min(le.max(at)));
    while s > ls && t.class_at(t.prev(s), big) == c {
        s = t.prev(s);
    }
    while e < le && t.class_at(e, big) == c {
        e = t.next(e);
    }
    (s, e)
}

fn word(t: Text, at: usize, big: bool, inner: bool, count: usize) -> Option<Span> {
    let line = t.line_of(at);
    let le = t.line_end(line);
    if at >= le {
        return None;
    }
    let (start, mut end) = run(t, at, big);
    if inner {
        for _ in 1..count {
            if end >= le {
                break;
            }
            end = run(t, end, big).1;
        }
        return Some(chars(start, end));
    }
    // Each is a word and the space after it, or a space and the word after.
    let mut start = start;
    let mut spaced = false;
    for i in 0..count {
        let here = if i == 0 { at } else { end };
        if i > 0 && end >= le {
            break;
        }
        end = run(t, here, big).1;
        let on_space = t.class_at(here, big) == Class::Space;
        if end < le && (on_space || t.class_at(end, big) == Class::Space) {
            end = run(t, end, big).1;
            spaced = true;
        } else if on_space {
            spaced = true;
        }
    }
    if !spaced {
        // Nothing after it to take: the space before it goes instead.
        let ls = t.line_start(line);
        while start > ls && t.class_at(t.prev(start), big) == Class::Space {
            start = t.prev(start);
        }
    }
    Some(chars(start, end))
}

fn quote(t: Text, at: usize, q: char, inner: bool) -> Option<Span> {
    let line = t.line_of(at);
    let (ls, le) = (t.line_start(line), t.line_end(line));
    let mut marks = Vec::new();
    let mut p = ls;
    let mut escaped = false;
    while p < le {
        let c = t.char_at(p)?;
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == q {
            marks.push(p);
        }
        p = t.next(p);
    }
    let (open, close) = (0..marks.len() / 2)
        .map(|i| (marks[2 * i], marks[2 * i + 1]))
        .find(|&(_, c)| at <= c)?;
    if inner {
        return Some(chars(t.next(open), close));
    }
    let (mut from, mut to) = (open, t.next(close));
    if matches!(t.char_at(to), Some(' ' | '\t')) {
        while to < le && matches!(t.char_at(to), Some(' ' | '\t')) {
            to = t.next(to);
        }
    } else {
        while from > ls && matches!(t.char_at(t.prev(from)), Some(' ' | '\t')) {
            from = t.prev(from);
        }
    }
    Some(chars(from, to))
}

/// The close that matches the open at `open_at`.
fn close_of(t: Text, open_at: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize;
    let mut p = open_at;
    while p < t.len() {
        let c = t.char_at(p)?;
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some(p);
            }
        }
        p = t.next(p);
    }
    None
}

/// The open that encloses `at`, or is at it.
fn open_around(t: Text, at: usize, open: char, close: char) -> Option<usize> {
    if t.char_at(at) == Some(open) {
        return Some(at);
    }
    let mut depth = 0usize;
    let mut p = at;
    while p > 0 {
        p = t.prev(p);
        let c = t.char_at(p)?;
        if c == close {
            depth += 1;
        } else if c == open {
            if depth == 0 {
                return Some(p);
            }
            depth -= 1;
        }
    }
    None
}

fn pair(t: Text, at: usize, open: char, close: char, inner: bool, count: usize) -> Option<Span> {
    let mut o = match open_around(t, at, open, close) {
        Some(o) => o,
        None => {
            let mut p = at;
            while t.char_at(p)? != open {
                p = t.next(p);
            }
            p
        }
    };
    for _ in 1..count {
        o = open_around(t, t.prev(o), open, close).filter(|&n| n < o)?;
    }
    let c = close_of(t, o, open, close)?;
    if !inner {
        return Some(chars(o, t.next(c)));
    }
    let (mut from, mut to) = (t.next(o), c);
    // A block opened and closed on lines of their own is its lines.
    if t.char_at(from) == Some('\n') && from < to {
        from = t.next(from);
        let close_line = t.line_of(c);
        let before = t.line_start(close_line);
        if t.first_nonblank(close_line) == c && before >= from {
            to = before;
        }
    }
    Some(chars(from, to.max(from)))
}

fn paragraph(t: Text, at: usize, inner: bool, count: usize) -> Option<Span> {
    let line = t.line_of(at);
    let blank = |l: usize| t.is_blank_line(l);
    let last = t.last_line();
    // The last line of the run of like lines `l` starts.
    let run_end = |mut l: usize| {
        let k = blank(l);
        while l < last && blank(l + 1) == k {
            l += 1;
        }
        l
    };
    let kind = blank(line);
    let mut a = line;
    while a > 0 && blank(a - 1) == kind {
        a -= 1;
    }
    let mut b = run_end(line);
    let mut after = false;
    for i in 0..count {
        if i > 0 {
            if b >= last {
                break;
            }
            b = run_end(b + 1);
        }
        if !inner && b < last {
            b = run_end(b + 1);
            after |= i == 0;
        }
    }
    if !inner && !after && !kind {
        // Nothing after it to take: the blank lines before come instead.
        while a > 0 && blank(a - 1) {
            a -= 1;
        }
    }
    Some(Span { from: t.line_start(a), to: t.line_span(b).1, linewise: true })
}

#[cfg(test)]
mod tests;
