//! Where a motion goes from a position, and what kind of reach it is for an
//! operator: charwise exclusive or inclusive, or whole lines. A motion that
//! cannot go (`fx` with no `x`) gives nothing, and the command it was part
//! of is dropped.

use super::text::Text;
use super::word;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Find {
    /// `f`
    To,
    /// `F`
    ToBack,
    /// `t`
    Till,
    /// `T`
    TillBack,
}

impl Find {
    fn reversed(self) -> Find {
        match self {
            Find::To => Find::ToBack,
            Find::ToBack => Find::To,
            Find::Till => Find::TillBack,
            Find::TillBack => Find::Till,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
    /// `0`
    LineStart,
    /// `^`
    FirstNonBlank,
    /// `$`
    LineEnd,
    /// `|`
    Column,
    WordForward { big: bool },
    WordEnd { big: bool },
    WordBack { big: bool },
    WordEndBack { big: bool },
    /// `gg`
    FileStart,
    /// `G`
    FileEnd,
    Find(Find, char),
    /// `;` and `,`
    RepeatFind { reverse: bool },
    ParagraphForward,
    ParagraphBack,
    /// `%`
    Matching,
    /// The current line and the count - 1 below it: what a doubled operator
    /// acts on.
    Line,
    /// `+`, `-` and `_`: down, up, down count - 1, to the first non-blank.
    Plus,
    Minus,
    Underscore,
}

/// Where a motion lands and how an operator takes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reach {
    pub to: usize,
    pub linewise: bool,
    /// Charwise only: the character at `to` is part of it.
    pub inclusive: bool,
    /// Vim keeps the deleted text of this one out of the small-delete
    /// register whatever its length (`%`, `{`, `}`).
    pub far: bool,
    /// Whether `j` and `k` keep the column they were aiming at.
    pub vertical: bool,
}

impl Reach {
    fn excl(to: usize) -> Reach {
        Reach { to, linewise: false, inclusive: false, far: false, vertical: false }
    }

    fn incl(to: usize) -> Reach {
        Reach { inclusive: true, ..Reach::excl(to) }
    }

    fn lines(to: usize) -> Reach {
        Reach { linewise: true, inclusive: true, ..Reach::excl(to) }
    }
}

/// What a motion needs of the editor beyond the text.
pub struct Aim<'a> {
    /// The column `j` and `k` are aiming for.
    pub want: usize,
    /// The last `f`, `F`, `t` or `T`, which `;` and `,` repeat.
    pub last_find: &'a mut Option<(Find, char)>,
    /// An operator is waiting: `l` may then reach the line's end, which a
    /// bare cursor cannot rest on.
    pub operator: bool,
}

fn matching(t: Text, from: usize) -> Option<usize> {
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

fn find(t: Text, from: usize, kind: Find, ch: char, count: usize, repeat: bool) -> Option<usize> {
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

fn paragraph(t: Text, from: usize, count: usize, forward: bool) -> usize {
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

/// Where `motion` takes a cursor at `from`, `count` times (`None` is no
/// count).
pub fn reach(motion: Motion, t: Text, from: usize, count: Option<usize>, aim: &mut Aim) -> Option<Reach> {
    let n = count.unwrap_or(1).max(1);
    let line = t.line_of(from);
    match motion {
        Motion::Left => {
            let start = t.line_start(line);
            if from == start {
                return None;
            }
            let mut p = from;
            for _ in 0..n {
                if p == start {
                    break;
                }
                p = t.prev(p);
            }
            Some(Reach::excl(p))
        }
        Motion::Right => {
            let limit = if aim.operator { t.line_end(line) } else { t.last_char(line) };
            if from >= limit {
                return None;
            }
            let mut p = from;
            for _ in 0..n {
                if p >= limit {
                    break;
                }
                p = t.next(p);
            }
            Some(Reach::excl(p))
        }
        Motion::Up | Motion::Down => {
            let up = motion == Motion::Up;
            let target = if up { line.checked_sub(n).or((line > 0).then_some(0))? } else { (line + n).min(t.last_line()) };
            if target == line {
                return None;
            }
            Some(Reach { vertical: true, ..Reach::lines(t.at_col(target, aim.want)) })
        }
        Motion::LineStart => Some(Reach::excl(t.line_start(line))),
        Motion::FirstNonBlank => Some(Reach::excl(first_or_last(t, line))),
        Motion::LineEnd => {
            let target = (line + n - 1).min(t.last_line());
            Some(Reach::incl(t.last_char(target).max(t.line_start(target))))
        }
        Motion::Column => Some(Reach::excl(t.at_col(line, n - 1))),
        Motion::WordForward { big } => {
            let mut p = from;
            for _ in 0..n {
                p = word::forward(t, p, big);
            }
            Some(Reach::excl(p))
        }
        Motion::WordEnd { big } => {
            let mut p = from;
            for _ in 0..n {
                p = word::end(t, p, big);
            }
            Some(Reach::incl(p))
        }
        Motion::WordBack { big } => {
            let mut p = from;
            for _ in 0..n {
                p = word::back(t, p, big);
            }
            Some(Reach::excl(p))
        }
        Motion::WordEndBack { big } => {
            let mut p = from;
            for _ in 0..n {
                p = word::end_back(t, p, big);
            }
            Some(Reach::incl(p))
        }
        Motion::FileStart | Motion::FileEnd => {
            let target = match (count, motion) {
                (Some(c), _) => (c.max(1) - 1).min(t.last_line()),
                (None, Motion::FileStart) => 0,
                (None, _) => t.last_line(),
            };
            Some(Reach::lines(first_or_last(t, target)))
        }
        Motion::Find(kind, ch) => {
            *aim.last_find = Some((kind, ch));
            let to = find(t, from, kind, ch, n, false)?;
            Some(Reach { inclusive: matches!(kind, Find::To | Find::Till), ..Reach::excl(to) })
        }
        Motion::RepeatFind { reverse } => {
            let (kind, ch) = (*aim.last_find)?;
            let kind = if reverse { kind.reversed() } else { kind };
            let to = find(t, from, kind, ch, n, true)?;
            Some(Reach { inclusive: matches!(kind, Find::To | Find::Till), ..Reach::excl(to) })
        }
        Motion::ParagraphForward | Motion::ParagraphBack => {
            let forward = motion == Motion::ParagraphForward;
            let to = paragraph(t, from, n, forward);
            // At the end of the text the motion takes the last character too.
            let end = forward && to == t.len() && !t.is_empty();
            let to = if end { t.prev(t.len()) } else { to };
            Some(Reach { inclusive: end, far: true, ..Reach::excl(to) })
        }
        Motion::Matching => Some(Reach { far: true, ..Reach::incl(matching(t, from)?) }),
        Motion::Line => {
            let target = (line + n - 1).min(t.last_line());
            Some(Reach::lines(t.line_start(target)))
        }
        Motion::Plus | Motion::Minus | Motion::Underscore => {
            let target = match motion {
                Motion::Plus => line + n,
                Motion::Minus => line.checked_sub(n)?,
                _ => line + n - 1,
            };
            if target > t.last_line() {
                return None;
            }
            Some(Reach::lines(first_or_last(t, target)))
        }
    }
}

/// Where `^` and `gg` land: the first character that is not blank, or the
/// last one on a line of blanks.
fn first_or_last(t: Text, line: usize) -> usize {
    let p = t.first_nonblank(line);
    if p >= t.line_end(line) {
        t.last_char(line)
    } else {
        p
    }
}

#[cfg(test)]
mod tests;
