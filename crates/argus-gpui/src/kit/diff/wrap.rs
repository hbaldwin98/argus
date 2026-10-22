//! Where a long line breaks in a split half, and how a place in the text maps between the line and
//! its broken form.
//!
//! It is done here and not left to the text system so the number of lines a row takes is a fact of
//! the text and the width, which the list needs up front.

use super::text::Code;
use unicode_width::UnicodeWidthChar as _;

/// The byte offsets a text breaks before when each line holds `per_line`
/// cells. The page wraps a split half "anywhere"; gpui's own wrapper prefers
/// a word boundary, so the row's height could not be known up front. Cutting
/// at the cell makes the number of lines a fact of the text and the width.
/// A glyph wider than the line takes a line of its own.
fn breaks(text: &str, per_line: usize) -> impl Iterator<Item = usize> + '_ {
    let mut column = 0usize;
    text.char_indices().filter_map(move |(at, c)| {
        let width = c.width().unwrap_or(0);
        let cut = column > 0 && column + width > per_line;
        column = if cut { width } else { column + width };
        cut.then_some(at)
    })
}

/// How many lines `text` takes at `per_line` cells a line; 0 means unbounded.
pub fn wrapped_lines(text: &str, per_line: usize) -> usize {
    if per_line == 0 {
        return 1;
    }
    1 + breaks(text, per_line).count()
}

/// `drawn` with a newline at each break, its spans moved to stay on the same
/// characters. Unbounded when `per_line` is 0.
pub fn wrap(drawn: &Code, per_line: usize) -> Code {
    if per_line == 0 {
        return drawn.clone();
    }
    let cuts: Vec<usize> = breaks(&drawn.text, per_line).collect();
    if cuts.is_empty() {
        return drawn.clone();
    }
    let mut text = String::with_capacity(drawn.text.len() + cuts.len());
    let mut from = 0;
    for &cut in &cuts {
        text.push_str(&drawn.text[from..cut]);
        text.push('\n');
        from = cut;
    }
    text.push_str(&drawn.text[from..]);
    let spans = drawn
        .spans
        .iter()
        .map(|(range, kind)| {
            let start = range.start + cuts.partition_point(|cut| *cut <= range.start);
            let end = range.end + cuts.partition_point(|cut| *cut < range.end);
            (start..end, *kind)
        })
        .collect();
    Code { text, spans }
}

/// The index in `drawn` of the character at `index` in `wrapped` (`drawn`
/// with a newline at each break): the newlines before it are not in `drawn`.
pub fn unwrapped(wrapped: &str, index: usize) -> usize {
    index - wrapped.as_bytes()[..index.min(wrapped.len())].iter().filter(|b| **b == b'\n').count()
}

/// The index in `wrapped` of the byte at `index` in the text it was cut
/// from. At a break the byte is the first of the next line, which a start of
/// a range takes (`after`) and an end does not.
pub fn wrapped_index(wrapped: &str, index: usize, after: bool) -> usize {
    let (mut seen, mut at) = (0, 0);
    for (i, b) in wrapped.bytes().enumerate() {
        if b == b'\n' {
            continue;
        }
        if seen == index {
            return if after { i } else { at };
        }
        seen += 1;
        at = i + 1;
    }
    wrapped.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::diff::tests::side;
    use crate::kit::diff::text::code;

    #[test]
    fn a_text_wraps_at_the_cell_with_no_regard_for_words() {
        assert_eq!(wrapped_lines("abcdefghij", 4), 3);
        assert_eq!(wrapped_lines("abcd", 4), 1);
        assert_eq!(wrapped_lines("abcde", 4), 2);
        assert_eq!(wrapped_lines("a b c d e f g", 4), 4, "the spaces are cells like any other");
        assert_eq!(wrapped_lines("abcdefghij", 0), 1, "unbounded");
        assert_eq!(wrapped_lines("", 4), 1);
        assert_eq!(wrapped_lines("ab\u{4e2d}", 3), 2, "a wide glyph that does not fit moves down whole");
        assert_eq!(wrapped_lines("\u{4e2d}\u{4e2d}", 1), 2, "a glyph wider than the line takes one alone");
    }

    #[test]
    fn wrapping_keeps_each_span_on_its_characters() {
        let drawn = code(&side(None, None, "context", &[("abcd", ""), ("efgh", "k"), ("ij", "")]));
        let wrapped = wrap(&drawn, 3);
        assert_eq!(wrapped.text, "abc\ndef\nghi\nj");
        assert_eq!(wrapped.text.lines().count(), wrapped_lines(&drawn.text, 3));
        let cut: Vec<&str> = wrapped.spans.iter().map(|(range, _)| &wrapped.text[range.clone()]).collect();
        assert_eq!(cut, ["ef\ngh"]);
        assert_eq!(wrap(&drawn, 0), drawn);
        assert_eq!(wrap(&drawn, 100), drawn);
    }

    #[test]
    fn a_selection_is_carried_between_the_text_and_its_wrapped_form() {
        let drawn = code(&side(None, None, "context", &[("abcdefghij", "")]));
        let wrapped = wrap(&drawn, 3);
        assert_eq!(wrapped.text, "abc
def
ghi
j");
        assert_eq!(unwrapped(&wrapped.text, 5), 4);
        assert_eq!(unwrapped(&wrapped.text, wrapped.text.len()), 10);
        assert_eq!(wrapped_index(&wrapped.text, 3, true), 4);
        assert_eq!(wrapped_index(&wrapped.text, 3, false), 3);
        assert_eq!(wrapped_index(&wrapped.text, 10, false), wrapped.text.len());
        assert_eq!(wrapped_index(&wrapped.text, 0, true), 0);
    }
}
