//! `:sort`: the lines of the range (every line, with none) put in order.
//! `!` reverses, and the flags are `i` ignore case, `u` drop repeats, `n` or
//! `d` decimal, `x` hex, `o` octal (one of them at most), `r` sort on the
//! match of a `/pattern/` rather than after it. Lines without a number, or
//! without a match, go before the rest, or after it when reversed.

use std::cmp::Ordering;
use std::sync::OnceLock;

use fancy_regex::Regex;

use crate::code::change::Change;

use super::range::Parsed;
use super::super::search::pattern::compile;
use super::super::Vim;

#[derive(Default)]
struct Args {
    reverse: bool,
    ignore_case: bool,
    unique: bool,
    include_match: bool,
    radix: Option<u32>,
    pattern: Option<Regex>,
}

fn parse_args(arg: &str) -> Result<Args, String> {
    let mut args = Args::default();
    let mut rest = arg;
    if let Some(r) = rest.strip_prefix('!') {
        args.reverse = true;
        rest = r;
    }
    if rest.is_empty() {
        return Ok(args);
    }
    if !rest.starts_with(char::is_whitespace) {
        return Err("Invalid arguments".into());
    }
    rest = rest.trim_start();
    let flags = rest.chars().take_while(|c| "dinuoxr".contains(*c)).count();
    let (flags, rest) = rest.split_at(flags);
    let rest = rest.trim();
    if flags.contains('i') {
        args.ignore_case = true;
    }
    args.include_match = flags.contains('r');
    args.unique = flags.contains('u');
    let kinds = [flags.contains('d') || flags.contains('n'), flags.contains('x'), flags.contains('o')];
    if kinds.iter().filter(|k| **k).count() > 1 {
        return Err("Invalid arguments".into());
    }
    args.radix = if kinds[0] {
        Some(10)
    } else if kinds[1] {
        Some(16)
    } else if kinds[2] {
        Some(8)
    } else {
        None
    };
    if !rest.is_empty() {
        let inner = rest.strip_prefix('/').and_then(|r| r.strip_suffix('/'));
        let Some(inner) = inner.filter(|i| !i.is_empty()) else { return Err("Invalid arguments".into()) };
        args.pattern = Some(compile(inner, args.ignore_case).map_err(|_| "Invalid arguments".to_owned())?);
    }
    Ok(args)
}

/// The first number in `text` read in `radix`, as the library's regular
/// expressions find it: decimal digits with a `-` before, hex digits with a
/// `-` and an optional `0x`, or octal digits.
fn number_of(text: &str, radix: u32) -> Option<i128> {
    static DECIMAL: OnceLock<Regex> = OnceLock::new();
    static HEX: OnceLock<Regex> = OnceLock::new();
    static OCTAL: OnceLock<Regex> = OnceLock::new();
    let re = match radix {
        10 => DECIMAL.get_or_init(|| Regex::new(r"(-?)(\d+)").expect("a valid pattern")),
        16 => HEX.get_or_init(|| Regex::new(r"(?i)(-?)(?:0x)?([0-9a-f]+)").expect("a valid pattern")),
        _ => OCTAL.get_or_init(|| Regex::new(r"()([0-7]+)").expect("a valid pattern")),
    };
    let caps = re.captures(text).ok()??;
    let digits = caps.get(2)?.as_str();
    let value = i128::from_str_radix(digits, radix).ok()?;
    Some(if caps.get(1).is_some_and(|m| m.as_str() == "-") { -value } else { value })
}

fn utf16_cmp(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

impl Vim {
    pub(super) fn ex_sort(&mut self, p: &Parsed) {
        let arg = p.arg.as_deref().unwrap_or_default();
        let args = match parse_args(arg) {
            Ok(a) => a,
            Err(e) => {
                self.message = Some(format!("{e}: {arg}"));
                return;
            }
        };
        let t = self.buf.text();
        let first = p.line.filter(|l| *l != 0).unwrap_or(0);
        let last = p.line_end.filter(|l| *l != 0).or(p.line.filter(|l| *l != 0)).unwrap_or(t.last_line());
        if first == last || self.buf.read_only() {
            return;
        }
        let (first, last) = (first.min(last), last.min(t.last_line()));
        let from = t.line_start(first);
        let to = t.line_end(last);
        let text = t.slice(from, to);
        let lines: Vec<&str> = text.split('\n').collect();

        let radix = args.radix;
        let key_of = |line: &str| -> Option<i128> { radix.and_then(|r| number_of(line, r)) };
        let (mut numbered, mut plain): (Vec<(String, Option<String>)>, Vec<String>) = (Vec::new(), Vec::new());
        if radix.is_some() || args.pattern.is_some() {
            for line in &lines {
                let found = args.pattern.as_ref().and_then(|re| re.find(line).ok().flatten());
                let matched = found.map(|m| {
                    if args.include_match {
                        m.as_str().to_owned()
                    } else {
                        line[m.end()..].to_owned()
                    }
                });
                match matched {
                    Some(m) if !m.is_empty() => numbered.push(((*line).to_owned(), Some(m))),
                    _ if radix.is_some_and(|r| number_of(line, r).is_some()) => numbered.push(((*line).to_owned(), None)),
                    _ => plain.push((*line).to_owned()),
                }
            }
        } else {
            plain = lines.iter().map(|l| (*l).to_owned()).collect();
        }
        let fold = |s: &str| if args.ignore_case { s.to_lowercase() } else { s.to_owned() };
        let compare = |a: &str, b: &str| -> Ordering {
            let (a, b) = if args.reverse { (b, a) } else { (a, b) };
            let (a, b) = (fold(a), fold(b));
            match (key_of(&a), key_of(&b)) {
                (Some(x), Some(y)) if radix.is_some() => x.cmp(&y),
                _ => utf16_cmp(&a, &b),
            }
        };
        if args.pattern.is_some() {
            numbered.sort_by(|(_, a), (_, b)| {
                let (a, b) = (a.clone().unwrap_or_default(), b.clone().unwrap_or_default());
                let (a, b) = if args.reverse { (b, a) } else { (a, b) };
                utf16_cmp(&fold(&a), &fold(&b))
            });
        } else {
            numbered.sort_by(|(a, _), (b, _)| compare(a, b));
        }
        if radix.is_none() && args.pattern.is_none() {
            plain.sort_by(|a, b| compare(a, b));
        }
        let numbered: Vec<String> = numbered.into_iter().map(|(l, _)| l).collect();
        let mut out: Vec<String> = if args.reverse { [numbered, plain].concat() } else { [plain, numbered].concat() };
        if args.unique {
            out.dedup();
        }
        let sorted = out.join("\n");
        let new_len = sorted.len();
        self.buf.edit(&[Change { from, to, insert: sorted }]);
        // The cursor follows the change: one in the lines sorted goes to their start.
        let cursor = self.cursor;
        self.cursor = if cursor < from {
            cursor
        } else if cursor <= to {
            from
        } else {
            cursor + new_len - (to - from)
        };
    }
}

#[cfg(test)]
mod tests;
