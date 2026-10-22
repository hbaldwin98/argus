//! What a search pattern is: the text typed after `/` made into a regular
//! expression the way the library makes it, with `/i` after it forcing case
//! off, a smart case that goes by capitals, and (with `pcre` off) Vim's own
//! pattern syntax translated first. The page runs these as JavaScript
//! regular expressions, so what a script of Vim's `\v` translates to is
//! JavaScript's; here the engine is `fancy-regex`, which has the look-around
//! and back-references a JavaScript pattern may use.

use std::fmt;

use fancy_regex::Regex;

/// A compiled search: the pattern as typed (without `/flags`) and the
/// engine's regex, matched over the whole text with `^` and `$` at lines.
#[derive(Clone)]
pub struct Query {
    pub source: String,
    pub ignore_case: bool,
    pub regex: Regex,
}

impl fmt::Display for Query {
    /// As JavaScript writes a regular expression, which is how the library
    /// shows it in its messages.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "/{}/{}", self.source, if self.ignore_case { "im" } else { "m" })
    }
}

impl fmt::Debug for Query {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Query({self})")
    }
}

impl PartialEq for Query {
    fn eq(&self, other: &Query) -> bool {
        self.source == other.source && self.ignore_case == other.ignore_case
    }
}

/// The places in `s` of an unescaped `separator`, as the library counts
/// them: a backslash takes the character after it.
pub fn unescaped(s: &str, separator: char) -> Vec<usize> {
    let mut out = Vec::new();
    let mut escaped = false;
    for (i, c) in s.char_indices() {
        if !escaped && c == separator {
            out.push(i);
        }
        escaped = !escaped && c == '\\';
    }
    out
}

/// `s` split at its unescaped `separator`, which it must begin with; the
/// parts after the first separator, or none if it does not begin with one.
pub fn split_by(s: &str, separator: char) -> Option<Vec<String>> {
    let at = unescaped(s, separator);
    if at.is_empty() {
        return Some(Vec::new());
    }
    if at[0] != 0 {
        return None;
    }
    let mut parts = Vec::new();
    for (i, &from) in at.iter().enumerate() {
        let start = from + separator.len_utf8();
        let end = at.get(i + 1).copied().unwrap_or(s.len());
        parts.push(s[start..end].to_owned());
    }
    Some(parts)
}

/// `s` with each `/` that is not escaped escaped, so it can be followed by
/// `/flags` and read back as a pattern and its flags.
pub fn escape_slashes(s: &str) -> String {
    let mut out = String::new();
    let mut escaped = false;
    for c in s.chars() {
        if c == '/' && !escaped {
            out.push('\\');
        }
        out.push(c);
        escaped = !escaped && c == '\\';
    }
    out
}

/// Vim's pattern syntax as a JavaScript one: `\<` and `\>` word edges,
/// `\zs` and `\ze` as look-around, and the magic that `\v`, `\m`, `\M` and
/// `\V` choose.
pub fn translate(pattern: &str) -> String {
    let modes = |c: char| -> Option<&'static str> {
        match c {
            'V' => Some("|(){+?*.[$^"),
            'M' => Some("|(){+?*.["),
            'm' => Some("|(){+?"),
            'v' => Some("<>"),
            _ => None,
        }
    };
    let edge = |c: char| -> Option<&'static str> {
        match c {
            '>' => Some("(?<=[\\w])(?=[^\\w]|$)"),
            '<' => Some("(?<=[^\\w]|^)(?=[\\w])"),
            _ => None,
        }
    };
    let mut specials = "|(){+?";
    let mut out = String::new();
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            let Some(next) = chars.next() else {
                out.push('\\');
                break;
            };
            if next == '}' || specials.contains(next) {
                out.push(next);
            } else if let Some(m) = modes(next) {
                specials = m;
            } else if let Some(e) = edge(next) {
                out.push_str(e);
            } else {
                out.push('\\');
                out.push(next);
            }
        } else if "[|(){+*?.$^<>".contains(c) {
            if specials.contains(c) {
                match edge(c) {
                    Some(e) => out.push_str(e),
                    None => {
                        out.push('\\');
                        out.push(c);
                    }
                }
            } else {
                out.push(c);
            }
        } else {
            out.push(c);
        }
    }
    if let Some(i) = out.find("\\zs") {
        out = format!("(?<={}){}", &out[..i], &out[i + 3..]);
    }
    if let Some(i) = out.find("\\ze") {
        out = format!("{}(?={})", &out[..i], &out[i + 3..]);
    }
    out
}

/// A JavaScript pattern in the engine's syntax: `\/` is `/`, `\uXXXX` and
/// `\0` as the engine writes them, `[^]` any character, `[]` none, and a
/// named group's `(?<name>` as `(?P<name>`.
fn to_engine(pattern: &str) -> String {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let mut in_class = false;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' if i + 1 < chars.len() => {
                let n = chars[i + 1];
                match n {
                    '/' => out.push('/'),
                    '0' if !chars.get(i + 2).is_some_and(char::is_ascii_digit) => out.push_str("\\x00"),
                    'u' if i + 5 < chars.len() && chars[i + 2..i + 6].iter().all(char::is_ascii_hexdigit) => {
                        let hex: String = chars[i + 2..i + 6].iter().collect();
                        out.push_str(&format!("\\x{{{hex}}}"));
                        i += 4;
                    }
                    _ => {
                        out.push('\\');
                        out.push(n);
                    }
                }
                i += 2;
                continue;
            }
            '[' if !in_class => {
                if chars.get(i + 1) == Some(&'^') && chars.get(i + 2) == Some(&']') {
                    out.push_str("[\\s\\S]");
                    i += 3;
                    continue;
                }
                if chars.get(i + 1) == Some(&']') {
                    out.push_str("[^\\s\\S]");
                    i += 2;
                    continue;
                }
                in_class = true;
                out.push('[');
                // A `]` first in a class is a member in the engine, as a JavaScript one never is.
            }
            ']' if in_class => {
                in_class = false;
                out.push(']');
            }
            '[' if in_class => out.push_str("\\["),
            '(' if !in_class
                && chars.get(i + 1) == Some(&'?')
                && chars.get(i + 2) == Some(&'<')
                && !matches!(chars.get(i + 3), Some('=' | '!')) =>
            {
                out.push_str("(?P<");
                i += 3;
                continue;
            }
            _ => out.push(c),
        }
        i += 1;
    }
    out
}

/// The search a typed pattern asks for, or none if its pattern part is
/// empty (the previous search then stands). `ignore_case` and `smart_case`
/// are the caller's; a `/i` after the pattern forces case off whatever
/// they say, and a smart case goes by the pattern's capitals.
pub fn parse_query(raw: &str, ignore_case: bool, smart_case: bool, pcre: bool) -> Result<Option<Query>, String> {
    let slashes = unescaped(raw, '/');
    let (part, flags) = match slashes.first() {
        Some(&at) => (&raw[..at], &raw[at..]),
        None => (raw, ""),
    };
    if part.is_empty() {
        return Ok(None);
    }
    let force = flags.contains('i');
    let source = if pcre { part.to_owned() } else { translate(part) };
    let ignore = (if smart_case { !source.chars().any(|c| c.is_ascii_uppercase()) } else { ignore_case }) || force;
    match compile(&source, ignore) {
        Ok(regex) => Ok(Some(Query { source, ignore_case: ignore, regex })),
        Err(_) => Err(format!("Invalid regex: {raw}")),
    }
}

/// A JavaScript pattern, as the engine runs it: `^` and `$` at lines.
pub fn compile(pattern: &str, ignore_case: bool) -> Result<Regex, fancy_regex::Error> {
    Regex::new(&format!("(?m{}){}", if ignore_case { "i" } else { "" }, to_engine(pattern)))
}

/// A word made safe to put in a pattern, as the library does for `*`, and
/// a `^` too, which its list leaves out, so a run of symbols with one in it
/// is searched for as it is.
pub fn escape(word: &str) -> String {
    let mut out = String::new();
    for c in word.chars() {
        if ".?*+$[]/\\(){}|-^".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests;
