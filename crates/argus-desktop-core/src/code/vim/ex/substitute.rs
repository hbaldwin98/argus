//! `:s/pattern/replacement/flags count`: the matches in the lines named
//! are replaced, the first on each line or, with `g`, all of them; with
//! `c` each is asked about (`y n a l q`). The replacement has `&` and `$&`
//! for the match, `$1` for a group, `\n`, `\r` and `\t`; a pattern left
//! empty is the last one, and a replacement left out is the last one too.
//! Where the library's own rules differ from Vim's, the library's are kept:
//! the pattern goes by capitals (smart case), and a `/i` flag forces case
//! off.

use fancy_regex::Captures;

use crate::code::change::Change;

use super::range::Parsed;
use super::super::key::{Code, Key};
use super::super::search::pattern::{escape_slashes, split_by};
use super::super::search::scan;
use super::super::{Mode, Shared, Vim};

/// One match to replace, as it stands in the text before any replacement.
#[derive(Debug, Clone)]
struct Item {
    from: usize,
    to: usize,
    text: String,
}

/// A substitute asking whether to replace each match.
#[derive(Debug)]
pub struct Confirm {
    items: Vec<Item>,
    at: usize,
    /// How many bytes the replacements made so far added.
    delta: isize,
    /// Where the last match visited began, in the text as it is now.
    last: Option<usize>,
    found: usize,
    shown: String,
}

/// `\n`, `\r` and `\t` in a replacement are the characters, `\\` and `\/` the
/// characters themselves, and `\&` an `&`.
fn unescape_replacement(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some('/') => out.push('/'),
            Some('\\') => out.push('\\'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('&') => out.push('&'),
            _ => {
                out.push('\\');
                continue;
            }
        }
        chars.next();
    }
    out
}

/// A replacement in Vim's syntax (`pcre` off): `\1` is `$1`, `\n` a line
/// break, and the rest as the library translates it.
fn translate_replacement(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut escape_next = false;
    let mut i: isize = -1;
    while i < chars.len() as isize {
        let c = usize::try_from(i).ok().and_then(|k| chars.get(k)).copied();
        let n = chars.get((i + 1) as usize).copied();
        let pair = match (c, n) {
            (Some('\\'), Some('n')) => Some('\n'),
            (Some('\\'), Some('r')) => Some('\r'),
            (Some('\\'), Some('t')) => Some('\t'),
            _ => None,
        };
        if let Some(p) = pair {
            out.push(p);
            i += 1;
        } else if escape_next {
            out.extend(c);
            escape_next = false;
        } else if c == Some('\\') {
            escape_next = true;
            if n.is_some_and(|n| n.is_ascii_digit() || n == '$') {
                out.push('$');
            } else if n != Some('/') && n != Some('\\') {
                out.push('\\');
            }
        } else {
            if c == Some('$') {
                out.push('$');
            }
            out.extend(c);
            if n == Some('/') {
                out.push('\\');
            }
        }
        i += 1;
    }
    out
}

/// The replacement with `$&`, `$$` and `$1` to `$999` filled in from the
/// match: a group number past the last group loses digits until one is a
/// group, and a group that did not match is empty.
fn expand(replacement: &str, caps: &Captures) -> String {
    let chars: Vec<char> = replacement.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '$' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        match chars.get(i + 1) {
            Some('$') => {
                out.push('$');
                i += 2;
            }
            Some('&') => {
                out.push_str(caps.get(0).map_or("", |m| m.as_str()));
                i += 2;
            }
            Some(d) if d.is_ascii_digit() => {
                let mut end = i + 1;
                while end < chars.len() && end < i + 4 && chars[end].is_ascii_digit() {
                    end += 1;
                }
                let digits: String = chars[i + 1..end].iter().collect();
                let mut keep = digits.len();
                while keep > 0 && digits[..keep].parse::<usize>().unwrap_or(usize::MAX) >= caps.len() {
                    keep -= 1;
                }
                if keep == 0 {
                    out.push('$');
                    out.push_str(&digits);
                } else {
                    let group: usize = digits[..keep].parse().expect("digits");
                    out.push_str(caps.get(group).map_or("", |m| m.as_str()));
                    out.push_str(&digits[keep..]);
                }
                i = end;
            }
            _ => {
                out.push('$');
                i += 1;
            }
        }
    }
    out
}

impl Vim {
    pub(super) fn ex_substitute(&mut self, sh: &mut Shared, p: &Parsed, in_global: bool) {
        let arg = p.arg.clone().unwrap_or_default();
        let pcre = self.pcre(sh);
        let tokens = arg.chars().next().and_then(|sep| split_by(&arg, sep));
        let (mut regex_part, mut replacement, mut trailing) = (String::new(), None, Vec::new());
        match tokens {
            Some(t) if !t.is_empty() => {
                regex_part = t[0].clone();
                if let Some(r) = t.get(1) {
                    let r = if pcre {
                        // An `&` that is not escaped is `$&`. (The library's own rule asks for
                        // a character before it, so one first in the replacement, or right
                        // after another, stays an `&` there; it is the match here.)
                        let mut spaced = String::new();
                        let mut escaped = false;
                        for c in r.chars() {
                            if c == '&' && !escaped {
                                spaced.push_str("$&");
                            } else {
                                spaced.push(c);
                            }
                            escaped = !escaped && c == '\\';
                        }
                        unescape_replacement(&spaced)
                    } else {
                        translate_replacement(r)
                    };
                    sh.search.replacement = Some(r.clone());
                    replacement = Some(r);
                }
                if let Some(rest) = t.get(2).filter(|r| !r.is_empty()) {
                    trailing = rest.split(' ').map(str::to_owned).collect();
                }
            }
            _ if !arg.is_empty() => {
                self.message = Some("Substitutions should be of the form :s/pattern/replace/".into());
                return;
            }
            _ => {}
        }
        let flags = trailing.first().cloned().unwrap_or_default();
        let count = trailing.get(1).and_then(|c| c.trim().parse::<usize>().ok()).filter(|c| *c > 0);
        let (confirm, global) = (flags.contains('c'), flags.contains('g'));
        if !flags.is_empty() {
            regex_part = format!("{}/{flags}", escape_slashes(&regex_part));
        }
        if !regex_part.is_empty() {
            if let Err(e) = sh.search.update(&mut sh.regs, &regex_part, true, true, pcre) {
                self.message = Some(e);
                return;
            }
        }
        let Some(replacement) = replacement.or_else(|| sh.search.replacement.clone()) else {
            self.message = Some("No previous substitute regular expression".into());
            return;
        };
        let Some(query) = sh.search.query.clone() else {
            self.message = Some("No previous regular expression".into());
            return;
        };
        let t = self.buf.text();
        let cursor_line = t.line_of(self.cursor);
        let mut line_start = p.line.unwrap_or(cursor_line).min(t.last_line());
        let mut line_end = p.line_end.filter(|e| *e != 0).unwrap_or(line_start);
        if let Some(count) = count {
            line_start = line_end;
            line_end = line_start + count - 1;
        }
        let hint = if pcre { " (set nopcre to use Vim regexps)" } else { "" };
        let text = t.0.to_string();
        let from_byte = t.line_start(line_start.min(t.last_line()));
        let mut items = Vec::new();
        let (mut found, mut visited_last) = (0usize, false);
        let (mut prev_to, mut modified_line, mut joined): (Option<usize>, Option<usize>, bool) = (None, None, false);
        for (from, to) in scan::all(&query, &text) {
            if from < from_byte {
                continue;
            }
            // An empty match against the end of the last is not a new one.
            if from == to && prev_to == Some(from) && !items.is_empty() {
                continue;
            }
            found += 1;
            prev_to = Some(to);
            if t.line_of(from) > line_end {
                break;
            }
            if !global && modified_line == Some(t.line_of(from)) && !joined {
                continue;
            }
            let caps = query.regex.captures_from_pos(&text, from).ok().flatten();
            let new = match &caps {
                Some(c) => expand(&replacement, c),
                None => replacement.clone(),
            };
            joined = new.matches('\n').count() < text[from..to].matches('\n').count();
            modified_line = Some(t.line_of(to));
            items.push(Item { from, to, text: crate::code::sync::lf(&new) });
            visited_last = true;
        }
        let _ = visited_last;
        if items.is_empty() {
            let hint = if pcre { " (set nopcre to use vim regexps)" } else { "" };
            self.message = Some(format!("No matches for {query}{hint}"));
            return;
        }
        let shown = format!("{query}{hint}");
        if confirm {
            self.confirm = Some(Confirm { items, at: 0, delta: 0, last: None, found, shown });
            self.mode = Mode::Confirm;
            self.show_confirm();
            return;
        }
        let last_from = {
            let mut delta = 0isize;
            let mut last = 0;
            for it in &items {
                last = (it.from as isize + delta) as usize;
                delta += it.text.len() as isize - (it.to - it.from) as isize;
            }
            last
        };
        if !self.buf.read_only() {
            let edits: Vec<Change> =
                items.iter().map(|i| Change { from: i.from, to: i.to, insert: i.text.clone() }).collect();
            self.buf.edit(&edits);
        }
        self.cursor = last_from.min(self.buf.doc().len());
        if !in_global {
            self.message = Some(format!("Found {found} matches for pattern: {shown}"));
        }
    }

    /// The match being asked about is the selection, and the cursor is at it.
    fn show_confirm(&mut self) {
        let Some(c) = &mut self.confirm else { return };
        let item = &c.items[c.at];
        let from = (item.from as isize + c.delta) as usize;
        c.last = Some(from);
        self.cursor = from.min(self.buf.doc().len());
    }

    /// The match `:s///c` is asking about, in the text as it is now.
    pub fn confirming(&self) -> Option<(usize, usize)> {
        let c = self.confirm.as_ref()?;
        let item = c.items.get(c.at)?;
        Some(((item.from as isize + c.delta) as usize, (item.to as isize + c.delta) as usize))
    }

    fn replace_current(&mut self) {
        // A read-only file takes no replacement, so what follows stays where it is.
        if self.buf.read_only() {
            return;
        }
        let Some(c) = &mut self.confirm else { return };
        let item = c.items[c.at].clone();
        let from = (item.from as isize + c.delta) as usize;
        let to = (item.to as isize + c.delta) as usize;
        c.delta += item.text.len() as isize - (item.to - item.from) as isize;
        self.buf.replace(from, to, &item.text);
    }

    pub(in crate::code::vim) fn confirm_key(&mut self, sh: &mut Shared, key: Key) -> bool {
        let _ = sh;
        if self.confirm.is_none() {
            self.mode = Mode::Normal;
            return false;
        }
        let mut stop = false;
        match (key.code, key.typed()) {
            (_, Some('y')) => {
                self.replace_current();
                stop = self.next_confirm();
            }
            (_, Some('n')) => stop = self.next_confirm(),
            (_, Some('a')) => {
                while self.confirm.as_ref().is_some_and(|c| c.at < c.items.len()) {
                    self.replace_current();
                    if let Some(c) = &mut self.confirm {
                        c.last = Some((c.items[c.at].from as isize + c.delta) as usize);
                        c.at += 1;
                    }
                }
                stop = true;
            }
            (_, Some('l')) => {
                self.replace_current();
                stop = true;
            }
            (Code::Esc, _) | (_, Some('q')) => stop = true,
            _ => {}
        }
        if stop {
            self.finish_confirm();
        }
        true
    }

    /// Goes on to the next match; whether there is none.
    fn next_confirm(&mut self) -> bool {
        let Some(c) = &mut self.confirm else { return true };
        c.at += 1;
        if c.at >= c.items.len() {
            return true;
        }
        self.show_confirm();
        false
    }

    fn finish_confirm(&mut self) {
        let Some(c) = self.confirm.take() else { return };
        self.mode = Mode::Normal;
        if let Some(at) = c.last {
            self.cursor = at.min(self.buf.doc().len());
        }
        if c.at >= c.items.len() {
            self.message = Some(format!("Found {} matches for pattern: {}", c.found, c.shown));
        }
    }
}

#[cfg(test)]
mod tests;
