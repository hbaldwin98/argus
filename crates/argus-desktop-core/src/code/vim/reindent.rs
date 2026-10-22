//! `=`: lines re-indented by how deeply the brackets above them are open. Without an indent unit
//! (no grammar loaded, or a language that is not written in braces) it changes nothing and only
//! leaves the cursor where the library does (EDITOR-VIM-24).
//!
//! The rule is the plain one of a brace language: a line is indented one unit for each `{`, `(` or
//! `[` still open above it, less the closers it begins with. Strings and the language's line
//! comments are skipped when counting; a block comment or a string that spans lines is not known
//! and may throw the count off, as it does in any editor with no parse of its own.

use crate::code::change::Change;

use super::region::Region;
use super::Vim;

/// The width, in spaces, each line of `lines` is to be indented to, or `None` for a blank line,
/// which is left as it is.
pub fn indents(lines: &[&str], comment: Option<&str>, unit: usize) -> Vec<Option<usize>> {
    let mut depth = 0i64;
    let mut out = Vec::with_capacity(lines.len());
    for line in lines {
        let body = line.trim_start();
        let leading = body.chars().take_while(|c| matches!(c, '}' | ')' | ']')).count() as i64;
        out.push((!body.is_empty()).then(|| (depth - leading).max(0) as usize * unit));
        depth = (depth + net(body, comment)).max(0);
    }
    out
}

/// The brackets a line opens less the ones it closes, outside strings and after no line comment.
fn net(line: &str, comment: Option<&str>) -> i64 {
    let mut net = 0;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut rest = line;
    while let Some(c) = rest.chars().next() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
                quote = None;
            }
        } else if comment.is_some_and(|token| rest.starts_with(token)) {
            break;
        } else {
            match c {
                '"' | '`' => quote = Some(c),
                // A `'` opens a string only as a char literal ('x', '\n'): in Rust it is as often a
                // lifetime (`<'a>(x: &'a str)`), and reading that as a quote hides the brackets
                // between the two.
                '\'' => {
                    if let Some(len) = char_literal(rest) {
                        rest = &rest[len..];
                        continue;
                    }
                }
                '{' | '(' | '[' => net += 1,
                '}' | ')' | ']' => net -= 1,
                _ => {}
            }
        }
        rest = &rest[c.len_utf8()..];
    }
    net
}

/// The byte length of the char literal `text` begins with, if it begins with one.
fn char_literal(text: &str) -> Option<usize> {
    let mut chars = text.char_indices().skip(1);
    let (_, first) = chars.next()?;
    if first == '\\' {
        // An escape is a few characters at most ('\n', '\'', '\u{1F600}').
        return chars.skip(1).take(10).find(|(_, c)| *c == '\'').map(|(at, _)| at + 1);
    }
    let (at, close) = chars.next()?;
    (close == '\'').then_some(at + 1)
}

impl Vim {
    /// Re-indents the lines of `region` when there is an indent unit, and puts the cursor on the
    /// first non-blank of the first of them.
    pub(super) fn reindent(&mut self, region: Region) {
        let (first, last) = region.lines(self.buf.text());
        if let (Some(unit), false) = (self.indent_unit, self.buf.read_only()) {
            let t = self.buf.text();
            let whole = t.slice(0, t.len());
            let lines: Vec<&str> = whole.split('\n').collect();
            let wanted = indents(&lines, self.comment.as_deref(), unit);
            let mut edits = Vec::new();
            for line in first..=last.min(t.last_line()) {
                let Some(Some(width)) = wanted.get(line) else { continue };
                let (from, to) = (t.line_start(line), t.first_nonblank(line));
                let indent = " ".repeat(*width);
                if t.slice(from, to) != indent {
                    edits.push(Change { from, to, insert: indent });
                }
            }
            self.buf.edit(&edits);
        }
        let t = self.buf.text();
        let line = first.min(t.last_line());
        let p = t.first_nonblank(line);
        self.cursor = if p >= t.line_end(line) { t.last_char(line) } else { p };
        self.want = t.col(self.cursor);
    }
}

#[cfg(test)]
mod tests;
