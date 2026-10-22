//! What one line of Markdown is, said without a regex: blank, a fence, a heading, a rule, a quote,
//! a list item or a table's separator.
//!
//! The page reads each with a regular expression (`views/blocks/markdown.ts`); every test here is
//! the same expression written out, so a line is the same thing in both windows. Tabs were made
//! four spaces before any line is read.

/// Only whitespace.
pub fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

/// Up to three spaces of indent, then the rest; none if there are more.
fn indented(line: &str) -> Option<&str> {
    let rest = line.trim_start_matches(' ');
    (line.len() - rest.len() <= 3).then_some(rest)
}

fn is_blank_char(c: char) -> bool {
    c == ' ' || c == '\t'
}

/// A fence's opening: its character and how many of it, three or more.
pub fn fence(line: &str) -> Option<(char, usize)> {
    let rest = indented(line)?;
    let mark = rest.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let run = rest.chars().take_while(|c| *c == mark).count();
    (run >= 3).then_some((mark, run))
}

/// Whether `line` closes a fence of `mark` that opened with `len` of it.
pub fn closes_fence(line: &str, mark: char, len: usize) -> bool {
    let Some(rest) = indented(line) else { return false };
    let run = rest.chars().take_while(|c| *c == mark).count();
    run >= len && rest[run * mark.len_utf8()..].chars().all(is_blank_char)
}

/// An ATX heading: how many `#` it opened with and its text, a closing run of `#` taken off.
pub fn heading(line: &str) -> Option<(usize, &str)> {
    let rest = indented(line)?;
    let hashes = rest.chars().take_while(|c| *c == '#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let after = rest[hashes..].trim_end_matches(is_blank_char);
    if after.is_empty() {
        return Some((hashes, ""));
    }
    if !after.starts_with(is_blank_char) {
        return None;
    }
    let body = after.trim_start_matches(is_blank_char);
    if body.chars().all(|c| c == '#') {
        return Some((hashes, ""));
    }
    let stripped = body.trim_end_matches('#');
    if stripped.len() < body.len() && stripped.ends_with(is_blank_char) {
        return Some((hashes, stripped.trim_end_matches(is_blank_char)));
    }
    Some((hashes, body))
}

/// Three or more of one of `-`, `*` and `_`, spaces allowed between.
pub fn is_rule(line: &str) -> bool {
    let Some(rest) = indented(line) else { return false };
    let Some(mark) = rest.chars().next().filter(|c| matches!(c, '-' | '*' | '_')) else { return false };
    let mut count = 0;
    for c in rest.chars() {
        if c == mark {
            count += 1;
        } else if !is_blank_char(c) {
            return false;
        }
    }
    count >= 3
}

/// What follows a quote's `>`, if `line` is one.
pub fn quote(line: &str) -> Option<&str> {
    let rest = indented(line)?.strip_prefix('>')?;
    Some(rest.strip_prefix(' ').unwrap_or(rest))
}

/// A list item: its indent, its marker and the text after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Item<'a> {
    pub indent: usize,
    pub marker: &'a str,
    pub text: &'a str,
}

impl Item<'_> {
    pub fn ordered(&self) -> bool {
        self.marker.starts_with(|c: char| c.is_ascii_digit())
    }
}

pub fn item(line: &str) -> Option<Item<'_>> {
    let body = line.trim_start_matches(' ');
    let indent = line.len() - body.len();
    let first = body.chars().next()?;
    let marker_len = if matches!(first, '-' | '*' | '+') {
        1
    } else {
        let digits = body.chars().take_while(char::is_ascii_digit).count();
        let ends = body[digits..].chars().next().is_some_and(|c| c == '.' || c == ')');
        if !(1..=9).contains(&digits) || !ends {
            return None;
        }
        digits + 1
    };
    let (marker, after) = body.split_at(marker_len);
    let text = after.trim_start_matches(is_blank_char);
    // At least one blank, and then something that is not one.
    (text.len() < after.len() && !text.is_empty() && !text.starts_with(char::is_whitespace)).then_some(Item { indent, marker, text })
}

/// Whether `line` is a table's separator: `---`, optional colons and pipes between.
pub fn is_table_separator(line: &str) -> bool {
    let Some(rest) = indented(line) else { return false };
    let rest = rest.trim_end_matches(is_blank_char);
    let rest = rest.strip_prefix('|').unwrap_or(rest);
    let rest = rest.strip_suffix('|').unwrap_or(rest);
    !rest.is_empty()
        && rest.split('|').all(|cell| {
            let cell = cell.trim_matches(is_blank_char);
            let cell = cell.strip_prefix(':').unwrap_or(cell);
            let cell = cell.strip_suffix(':').unwrap_or(cell);
            !cell.is_empty() && cell.chars().all(|c| c == '-')
        })
}

/// Whether `line` starts a block of its own and so ends a paragraph.
pub fn starts_block(line: &str) -> bool {
    fence(line).is_some() || heading(line).is_some() || is_rule(line) || quote(line).is_some() || item(line).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fence_is_three_or_more_of_one_mark_after_at_most_three_spaces() {
        assert_eq!(fence("```rust"), Some(('`', 3)));
        assert_eq!(fence("   ~~~~"), Some(('~', 4)));
        assert_eq!(fence("    ```"), None, "four spaces is code, not a fence");
        assert_eq!(fence("``"), None);
        assert!(closes_fence("```  ", '`', 3));
        assert!(closes_fence("`````", '`', 3), "longer closes");
        assert!(!closes_fence("``", '`', 3), "shorter does not");
        assert!(!closes_fence("~~~", '`', 3));
        assert!(!closes_fence("``` x", '`', 3), "a closer has nothing after it");
    }

    #[test]
    fn a_heading_takes_its_closing_hashes_off_and_needs_a_blank_after_the_marker() {
        assert_eq!(heading("# Title"), Some((1, "Title")));
        assert_eq!(heading("### Title ###"), Some((3, "Title")));
        assert_eq!(heading("## a # b ##"), Some((2, "a # b")));
        assert_eq!(heading("## foo#"), Some((2, "foo#")), "a hash with no blank before it is text");
        assert_eq!(heading("#"), Some((1, "")));
        assert_eq!(heading("# ##"), Some((1, "")));
        assert_eq!(heading("#hashtag"), None);
        assert_eq!(heading("####### seven"), None);
        assert_eq!(heading("    # four spaces"), None);
    }

    #[test]
    fn a_rule_is_three_of_one_mark_and_nothing_else() {
        for rule in ["---", "***", "___", " - - -", "*  *  *   "] {
            assert!(is_rule(rule), "{rule:?}");
        }
        for not in ["--", "-*-", "--- x", "    ---", ""] {
            assert!(!is_rule(not), "{not:?}");
        }
    }

    #[test]
    fn a_quote_drops_one_space_after_its_marker() {
        assert_eq!(quote("> a"), Some("a"));
        assert_eq!(quote(">  a"), Some(" a"));
        assert_eq!(quote(">a"), Some("a"));
        assert_eq!(quote("a > b"), None);
    }

    #[test]
    fn an_item_has_a_marker_a_blank_and_text() {
        assert_eq!(item("- a"), Some(Item { indent: 0, marker: "-", text: "a" }));
        assert_eq!(item("   12) twelve "), Some(Item { indent: 3, marker: "12)", text: "twelve " }));
        assert!(item("1. x").unwrap().ordered());
        assert!(!item("* x").unwrap().ordered());
        for not in ["-a", "-", "- ", "1234567890. ten digits", "1 x", "a. x", "+"] {
            assert_eq!(item(not), None, "{not:?}");
        }
    }

    #[test]
    fn a_separator_is_dashes_with_optional_colons_and_pipes() {
        for sep in ["---", "|---|---|", "| :-- | --: | :-: |", "--|--", "  |-|"] {
            assert!(is_table_separator(sep), "{sep:?}");
        }
        for not in ["", "|", "| |", "|--- x|", "||---", "- -", "a|b"] {
            assert!(!is_table_separator(not), "{not:?}");
        }
    }
}
