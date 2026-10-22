//! A markdown file's images, said as the words that stand for them.
//!
//! The window has no way to read a file of the checkout, and drawing an address the file names would
//! fetch it from a network nobody chose, so an image is its alt text. A picture inside a link
//! (a badge) leaves its words as the link's label.

/// `source` with each `![alt](address)` replaced by `alt`. Fenced code and inline code are left as
/// written, since a README that shows the syntax means to show it.
pub fn as_alt_text(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut fence: Option<(char, usize)> = None;
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start_matches(' ');
        let indent = line.len() - trimmed.len();
        let marker = trimmed.chars().next().filter(|c| matches!(c, '`' | '~'));
        let run = marker.map_or(0, |m| trimmed.chars().take_while(|c| *c == m).count());
        match fence {
            Some((open, len)) => {
                out.push_str(line);
                if marker == Some(open) && run >= len && trimmed[run..].trim().is_empty() {
                    fence = None;
                }
            }
            None if indent < 4 && run >= 3 => {
                out.push_str(line);
                fence = marker.map(|m| (m, run));
            }
            None => out.push_str(&in_line(line)),
        }
    }
    out
}

fn in_line(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut at = 0;
    while at < line.len() {
        let rest = &line[at..];
        let width = rest.chars().next().map_or(1, char::len_utf8);
        if rest.starts_with('`') {
            // A span of code runs to the next run of as many backticks, or to the line's end.
            let run = rest.chars().take_while(|c| *c == '`').count();
            let end = rest[run..].find(&"`".repeat(run)).map_or(rest.len(), |close| run + close + run);
            out.push_str(&rest[..end]);
            at += end;
        } else if rest.starts_with('\\') {
            // An escaped character is not syntax: `\![a](b)` is the text it spells.
            let end = (width + rest[width..].chars().next().map_or(0, char::len_utf8)).min(rest.len());
            out.push_str(&rest[..end]);
            at += end;
        } else if let Some((alt, taken)) = image(rest) {
            out.push_str(alt);
            at += taken;
        } else {
            out.push_str(&rest[..width]);
            at += width;
        }
    }
    out
}

/// The alt text and the length of the `![alt](address)` that `rest` begins with, when it does.
fn image(rest: &str) -> Option<(&str, usize)> {
    let inner = rest.strip_prefix("![")?;
    let alt_end = closing(inner, '[', ']')?;
    let after = inner[alt_end + 1..].strip_prefix('(')?;
    let address_end = closing(after, '(', ')')?;
    Some((&inner[..alt_end], 2 + alt_end + 1 + 1 + address_end + 1))
}

/// The byte of the bracket that closes the one just opened, counting those opened since.
fn closing(text: &str, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize;
    let mut chars = text.char_indices();
    while let Some((at, c)) = chars.next() {
        match c {
            '\\' => {
                chars.next();
            }
            c if c == open => depth += 1,
            c if c == close && depth == 0 => return Some(at),
            c if c == close => depth -= 1,
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_image_is_its_alt_text_and_a_badge_keeps_its_link() {
        assert_eq!(as_alt_text("see ![the logo](img/logo.png \"t\") here"), "see the logo here");
        assert_eq!(as_alt_text("[![CI](https://x.test/b.svg)](https://x.test/ci)\n"), "[CI](https://x.test/ci)\n");
        assert_eq!(as_alt_text("![a [b] c](u(1))"), "a [b] c");
    }

    #[test]
    fn code_is_left_as_written() {
        let text = "`![a](b)` and ``![c](d)``\n```md\n![e](f)\n```\n~~~\n![g](h)\n~~~\n![i](j)\n";
        assert_eq!(as_alt_text(text), "`![a](b)` and ``![c](d)``\n```md\n![e](f)\n```\n~~~\n![g](h)\n~~~\ni\n");
    }

    #[test]
    fn what_is_not_a_whole_image_is_left_alone() {
        for text in ["![no address]", "![open](addr", "! [a](b)", "\u{fc}n\u{ef} ![\u{e1}", "\\![a](b)x"] {
            assert_eq!(as_alt_text(text), text);
        }
    }

    mod properties {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            // Source: this module's own promise that only image syntax is touched, so text with no
            // `![` is returned as written, and that no input panics.
            #[test]
            fn text_without_image_syntax_is_unchanged_and_nothing_panics(text in "\\PC{0,200}") {
                let said = as_alt_text(&text);
                if !text.contains("![") {
                    prop_assert_eq!(said, text);
                }
            }
        }
    }
}
