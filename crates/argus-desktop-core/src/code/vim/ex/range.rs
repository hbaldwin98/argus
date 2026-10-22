//! The lines an ex command names: `%`, `'<,'>`, and addresses `N`, `.`, `$`
//! with an optional `+N` or `-N`, one or two joined by `,`. A pure reading
//! of the front of the line; what follows is the command.

use super::text::Text;

/// A range read off the front of `line`: first and last line (0-based,
/// first never past last) and the rest. `None` when the line names none.
/// `selection` is the lines the last visual selection covered, for `'<,'>`.
pub fn parse<'a>(t: Text, current: usize, selection: Option<(usize, usize)>, line: &'a str) -> (Option<(usize, usize)>, &'a str) {
    let last = t.last_line();
    if let Some(rest) = line.strip_prefix('%') {
        return (Some((0, last)), rest.trim_start());
    }
    if let Some(rest) = line.strip_prefix("'<,'>") {
        let (a, b) = selection.unwrap_or((current, current));
        return (Some((a.min(last), b.min(last))), rest.trim_start());
    }
    let Some((a, rest)) = address(line, current, last) else { return (None, line) };
    let (b, rest) = match rest.strip_prefix(',') {
        Some(after) => match address(after, current, last) {
            Some((b, rest)) => (b, rest),
            None => (current, after),
        },
        None => (a, rest),
    };
    (Some((a.min(b), a.max(b))), rest.trim_start())
}

fn address(s: &str, current: usize, last: usize) -> Option<(usize, &str)> {
    let (base, mut rest) = if let Some(r) = s.strip_prefix('.') {
        (current as i64, r)
    } else if let Some(r) = s.strip_prefix('$') {
        (last as i64, r)
    } else {
        let digits = s.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            // A bare `+N` or `-N` is relative to the current line.
            if !s.starts_with(['+', '-']) {
                return None;
            }
            (current as i64, s)
        } else {
            let n: i64 = s[..digits].parse().unwrap_or(i64::MAX);
            ((n - 1).min(last as i64), &s[digits..])
        }
    };
    let mut at = base;
    while let Some(sign) = rest.chars().next().filter(|c| matches!(c, '+' | '-')) {
        let after = &rest[1..];
        let digits = after.bytes().take_while(u8::is_ascii_digit).count();
        let n: i64 = if digits == 0 { 1 } else { after[..digits].parse().unwrap_or(i64::MAX / 4) };
        at = if sign == '+' { at.saturating_add(n) } else { at.saturating_sub(n) };
        rest = &after[digits..];
    }
    Some((at.clamp(0, last as i64) as usize, rest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ropey::Rope;

    fn read(doc: &str, current: usize, line: &str) -> (Option<(usize, usize)>, String) {
        let rope = Rope::from_str(doc);
        let (r, rest) = parse(Text(&rope), current, Some((1, 2)), line);
        (r, rest.to_owned())
    }

    #[test]
    fn a_range_is_read_off_the_front_and_the_command_is_left() {
        let doc = "a\nb\nc\nd\ne";
        assert_eq!(read(doc, 0, "d"), (None, "d".into()));
        assert_eq!(read(doc, 2, "d"), (None, "d".into()));
        assert_eq!(read(doc, 0, "%d"), (Some((0, 4)), "d".into()));
        assert_eq!(read(doc, 0, "2,4y"), (Some((1, 3)), "y".into()));
        assert_eq!(read(doc, 3, "4,2j"), (Some((1, 3)), "j".into()));
        assert_eq!(read(doc, 1, ".,$d"), (Some((1, 4)), "d".into()));
        assert_eq!(read(doc, 1, ".+1,$-1d"), (Some((2, 3)), "d".into()));
        assert_eq!(read(doc, 1, "99d"), (Some((4, 4)), "d".into()));
        assert_eq!(read(doc, 1, "'<,'>d"), (Some((1, 2)), "d".into()));
        assert_eq!(read(doc, 1, "3"), (Some((2, 2)), String::new()));
    }
}
