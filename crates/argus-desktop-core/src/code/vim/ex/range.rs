//! Reading the front of an ex line: the lines it names and the command's
//! name, as the library reads them. A range is `%` (every line), `*` (the
//! last selection), or one or two addresses joined by `,`. An address is a
//! number, `.`, `$`, a mark (`'a`), or a pattern (`/foo/` the next line with
//! it, `?foo?` the last, `\/` and `\?` with the last pattern), with any
//! number of `+N` and `-N` after. The rest is the command, whose name is a
//! run of word characters or one of `! @ # & * < = > ~`.

use crate::code::vim::marks::{Marks, ANCHOR, HEAD};
use crate::code::vim::search::{pattern, scan};
use crate::code::vim::text::Text;

/// What an ex line says before the command's own arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    /// The first line named, 0-based, and the second if a second was.
    pub line: Option<usize>,
    pub line_end: Option<usize>,
    /// The lines a command acts on: those named, or the selection's, or the
    /// cursor's.
    pub first: usize,
    pub last: Option<usize>,
    pub name: String,
    /// What follows the name, as typed.
    pub arg: Option<String>,
}

impl Parsed {
    /// The lines the command acts on, ordered, clamped to the text.
    pub fn lines(&self, t: Text) -> (usize, usize) {
        let last = t.last_line();
        let a = self.first.min(last);
        let b = self.last.unwrap_or(self.first).min(last);
        (a.min(b), a.max(b))
    }

    /// The arguments, split at blanks.
    pub fn args(&self) -> Vec<&str> {
        self.arg.as_deref().map_or(Vec::new(), |a| a.split_whitespace().collect())
    }
}

/// What the reading needs of the editor.
pub struct Reader<'a> {
    pub t: Text<'a>,
    pub cursor_line: usize,
    pub visual: bool,
    pub marks: &'a Marks,
    /// What the `/` register holds, for a pattern address with none typed.
    pub last_pattern: Option<&'a str>,
}

struct Stream<'a> {
    s: &'a str,
    at: usize,
}

impl<'a> Stream<'a> {
    fn rest(&self) -> &'a str {
        &self.s[self.at..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.at += c.len_utf8();
            return true;
        }
        false
    }

    fn digits(&mut self) -> Option<usize> {
        let n = self.rest().bytes().take_while(u8::is_ascii_digit).count();
        if n == 0 {
            return None;
        }
        let v = self.rest()[..n].parse().unwrap_or(usize::MAX / 4);
        self.at += n;
        Some(v)
    }
}

/// The pattern at the start of `rest` (after a `/` or `?`) and how many
/// bytes it and the closing mark take. Going forward it ends at an
/// unescaped `/`; backward, at a `/` or a `?`.
fn read_pattern(rest: &str, forward: bool) -> (String, usize) {
    let mut query = String::new();
    let mut chars = rest.char_indices().peekable();
    let mut used = 0;
    while let Some((i, c)) = chars.next() {
        let next = chars.peek().map(|(_, n)| *n);
        if forward && c == '\\' {
            if next == Some('/') {
                query.push_str("\\/");
                chars.next();
                used = i + 2;
                continue;
            }
            break;
        }
        if c == '/' || (!forward && c == '?') {
            used = i + 1;
            return (query, used);
        }
        query.push(c);
        used = i + c.len_utf8();
    }
    (query, used)
}

impl Reader<'_> {
    pub fn parse(&self, input: &str) -> Result<Parsed, String> {
        let mut s = Stream { s: input, at: 0 };
        while s.eat(':') {}
        let last = self.t.last_line();
        let (line, line_end);
        if s.eat('%') {
            line = Some(0);
            line_end = Some(last);
        } else if s.eat('*') {
            let end = |name| self.marks.get(name).map_or(0, |p| self.t.line_of(p.min(self.t.len())));
            let (a, h) = (end(ANCHOR), end(HEAD));
            line = Some(a.max(h));
            line_end = Some(a.min(h));
        } else {
            line = self.spec(&mut s)?;
            line_end = if line.is_some() && s.eat(',') { self.spec(&mut s)? } else { None };
        }
        let (first, last_sel) = match line {
            None if self.visual => {
                let at = |name| self.marks.get(name).map(|p| self.t.line_of(p.min(self.t.len())));
                (at('<').unwrap_or(self.cursor_line), at('>'))
            }
            None => (self.cursor_line, None),
            Some(l) => (l, line_end),
        };
        while s.peek().is_some_and(|c| c == ' ' || c == '\t') {
            s.at += 1;
        }
        let rest = s.rest();
        let word = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').count();
        let name_len = if word > 0 {
            rest.char_indices().nth(word).map_or(rest.len(), |(i, _)| i)
        } else if rest.starts_with("!!") || rest.starts_with("@@") {
            2
        } else if rest.chars().next().is_some_and(|c| "!#&*<=>@~".contains(c)) {
            1
        } else {
            rest.len()
        };
        let name = rest[..name_len].to_owned();
        let tail = &rest[name_len..];
        Ok(Parsed {
            line: line.map(|l| l.min(last)),
            line_end: line_end.map(|l| l.min(last)),
            first: first.min(last),
            last: last_sel.map(|l| l.min(last)),
            name,
            arg: (!tail.is_empty()).then(|| tail.to_owned()),
        })
    }

    /// One address, with its offsets; none if the line does not begin with one.
    fn spec(&self, s: &mut Stream) -> Result<Option<usize>, String> {
        if let Some(n) = s.digits() {
            return self.offsets(s, n as i64 - 1).map(Some);
        }
        match s.peek() {
            Some('.') => {
                s.at += 1;
                self.offsets(s, self.cursor_line as i64).map(Some)
            }
            Some('$') => {
                s.at += 1;
                self.offsets(s, self.t.last_line() as i64).map(Some)
            }
            Some('\'') => {
                s.at += 1;
                let name = s.peek().ok_or("Mark not set")?;
                s.at += name.len_utf8();
                let at = self.marks.get(name).ok_or("Mark not set")?;
                self.offsets(s, self.t.line_of(at.min(self.t.len())) as i64).map(Some)
            }
            Some('-' | '+' | '/' | '?') => self.offsets(s, self.cursor_line as i64).map(Some),
            _ => Ok(None),
        }
    }

    /// The `+N`, `-N` and pattern addresses after an address.
    fn offsets(&self, s: &mut Stream, mut line: i64) -> Result<usize, String> {
        loop {
            let rest = s.rest();
            if rest.starts_with("\\/") || rest.starts_with("\\?") {
                let forward = rest.starts_with("\\/");
                s.at += 2;
                line = self.find_line(line, forward, "")?;
            } else if rest.starts_with('/') || rest.starts_with('?') {
                let forward = rest.starts_with('/');
                let (query, used) = read_pattern(&rest[1..], forward);
                s.at += 1 + used;
                line = self.find_line(line, forward, &query)?;
            } else {
                let sign = if rest.starts_with(['+', '-']) { rest.chars().next() } else { None };
                let skip = usize::from(sign.is_some());
                let n = rest[skip..].bytes().take_while(u8::is_ascii_digit).count();
                if skip + n == 0 {
                    break;
                }
                let value: i64 = if n == 0 { 1 } else { rest[skip..skip + n].parse().unwrap_or(i64::MAX / 4) };
                line = line.saturating_add(if sign == Some('-') { -value } else { value });
                s.at += skip + n;
            }
        }
        Ok(line.clamp(0, i64::from(i32::MAX)) as usize)
    }

    /// The line of the next match of `query` after `line` (or before it).
    fn find_line(&self, line: i64, forward: bool, query: &str) -> Result<i64, String> {
        let query = if query.is_empty() { self.last_pattern.unwrap_or_default() } else { query };
        let missing = || format!("Pattern not found/{query}/");
        let q = pattern::parse_query(query, false, false, true).ok().flatten().ok_or_else(missing)?;
        let text = self.t.0.to_string();
        let from_line = (line + i64::from(forward)).clamp(0, self.t.last_line() as i64) as usize;
        let mut cursor = scan::Cursor::new(&q, &text, self.t.line_start(from_line));
        if !cursor.find(!forward) {
            return Err(missing());
        }
        let (start, _) = cursor.found().expect("found");
        Ok(self.t.line_of(start) as i64)
    }
}

#[cfg(test)]
mod tests;
