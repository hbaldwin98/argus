//! Where a motion goes from a position, and what kind of reach it is for an
//! operator: charwise exclusive or inclusive, or whole lines. A motion that
//! cannot go (`fx` with no `x`) gives nothing, and the command it was part
//! of is dropped.

pub mod aim;
pub mod find;

use super::display;
use super::search::scan;
use super::sentence;
use super::symbol;
use super::text::Text;
use super::word;
use aim::Aim;
use find::{find, first_or_last, matching, paragraph};

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
    /// `(` and `)`
    Sentence { forward: bool },
    /// `]` and `[` with the symbol after them.
    Symbol { forward: bool, ch: char },
    /// `]'` and `['`, and the backtick pair: the next or last mark.
    JumpMark { forward: bool, linewise: bool },
    /// `'x` and `` `x ``
    Mark { name: char, linewise: bool },
    /// `H`, `M` and `L`
    Top,
    Middle,
    Bottom,
    /// `Ctrl-f` and `Ctrl-b`
    Page { forward: bool },
    /// `Ctrl-d` and `Ctrl-u`
    HalfPage { forward: bool },
    /// `gj` and `gk`
    Display { forward: bool },
    /// `g0` and `g^`
    DisplayStart,
    /// `g$`
    DisplayEnd,
    /// `n` (forward) and `N`
    SearchNext { forward: bool },
    /// `*`, `#`, `g*` and `g#`: search for the word under the cursor.
    Star { forward: bool, whole: bool },
    /// `gn` and `gN`, which a selection or an operator takes apart; not a
    /// reach of its own.
    Match { forward: bool },
}

impl Motion {
    /// Whether the cursor's place before it is put in the jump list.
    pub fn jumps(self) -> bool {
        matches!(
            self,
            Motion::Top
                | Motion::Middle
                | Motion::Bottom
                | Motion::FileStart
                | Motion::FileEnd
                | Motion::ParagraphBack
                | Motion::ParagraphForward
                | Motion::Matching
                | Motion::Mark { .. }
                | Motion::Symbol { .. }
                | Motion::SearchNext { .. }
                | Motion::Star { .. }
        )
    }
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
        Motion::Sentence { forward } => {
            let to = sentence::find(t, t.pos(from), n, forward);
            Some(Reach::excl(t.at(to.0, to.1)))
        }
        Motion::Symbol { forward, ch } => {
            let to = symbol::find(t, t.pos(from), n, forward, ch);
            Some(Reach::excl(t.at(to.0, to.1)))
        }
        Motion::Mark { name, linewise } => {
            let at = aim.marks.get(name)?.min(t.len());
            if linewise {
                Some(Reach::lines(first_or_last(t, t.line_of(at))))
            } else {
                Some(Reach::excl(at))
            }
        }
        Motion::JumpMark { forward, linewise } => {
            let marks = aim.marks.lower();
            let mut best = from;
            for _ in 0..n {
                let cursor = best;
                for &mark in &marks {
                    let wrong = if forward { mark < cursor } else { cursor < mark };
                    if wrong || (linewise && t.line_of(mark) == t.line_of(cursor)) {
                        continue;
                    }
                    let between = if forward { cursor < mark && mark < best } else { best < mark && mark < cursor };
                    if cursor == best || between {
                        best = mark;
                    }
                }
            }
            if linewise {
                best = first_or_last(t, t.line_of(best.min(t.len())));
            }
            Some(Reach::excl(best.min(t.len())))
        }
        Motion::Top | Motion::Middle | Motion::Bottom => {
            let view = aim.view.within(t.last_line());
            let target = match motion {
                Motion::Top => view.nth_from_top(n),
                Motion::Middle => view.middle(),
                _ => view.nth_from_bottom(n),
            };
            Some(Reach::lines(first_or_last(t, target)))
        }
        Motion::Page { forward } | Motion::HalfPage { forward } => {
            let view = aim.view.within(t.last_line());
            let target = if matches!(motion, Motion::Page { .. }) {
                view.paged(line, n, forward, t.last_line())
            } else {
                view.halved(line, aim.explicit.then_some(n), forward, t.last_line())
            };
            if target == line {
                return None;
            }
            Some(Reach { vertical: true, ..Reach::excl(t.at_col(target, aim.want)) })
        }
        Motion::Display { forward } => {
            let mut p = from;
            for _ in 0..n {
                match display::vertical(t, p, forward, aim.wrap, aim.want) {
                    Some(next) => p = next,
                    None => break,
                }
            }
            (p != from).then_some(Reach { vertical: true, ..Reach::excl(p) })
        }
        Motion::DisplayStart => Some(Reach::excl(display::row_start(t, from, aim.wrap))),
        Motion::DisplayEnd => Some(Reach::excl(display::row_end(t, from, aim.wrap))),
        Motion::SearchNext { forward } => {
            let query = aim.search?;
            let prev = if aim.reversed { forward } else { !forward };
            let text = t.0.to_string();
            match scan::find_next(query, &text, from, prev, n) {
                Some(to) => Some(Reach::excl(to)),
                None => {
                    let hint = if aim.pcre { " (set nopcre to use Vim regexps)" } else { "" };
                    *aim.message = Some(format!("No match found {query}{hint}"));
                    None
                }
            }
        }
        Motion::Star { .. } | Motion::Match { .. } => None,
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

#[cfg(test)]
mod tests;
