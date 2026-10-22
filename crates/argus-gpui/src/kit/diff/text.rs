//! A line's text as it is drawn: tabs stopped by cells, control characters shown, highlight kinds
//! cut into spans, and the stretches a selection splits them into.

use std::ops::Range;

use argus_desktop_core::connection::page::DiffSide;
use unicode_width::{UnicodeWidthChar as _, UnicodeWidthStr as _};

/// Where a tab stops. The page leaves the browser's 8; 4 keeps an indented
/// line inside a row a half of a split view holds.
pub const TAB: usize = 4;

/// A line's text as it is drawn, and the highlight kind of each stretch of it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Code {
    pub text: String,
    /// Byte ranges of `text`, in order and apart, each with its kind: `k`,
    /// `s`, `c`, `n`, `t`, `f`, `p` or `o`.
    pub spans: Vec<(Range<usize>, &'static str)>,
}

/// The side's parts as one string. A tab becomes the spaces up to the next
/// stop, counted in cells so a wide glyph (two cells) before it moves the
/// stop as it would in a terminal; a carriage return is dropped, since a
/// CRLF file would otherwise draw a box at every line's end; any other
/// control character is shown as U+FFFD rather than handed to the shaper.
pub fn code(side: &DiffSide) -> Code {
    let mut out = Code::default();
    let mut column = 0usize;
    for (text, kind) in &side.parts {
        let start = out.text.len();
        for c in text.chars() {
            match c {
                '\t' => {
                    let n = TAB - column % TAB;
                    out.text.extend(std::iter::repeat_n(' ', n));
                    column += n;
                }
                '\r' => {}
                c if c.is_control() => {
                    out.text.push('\u{fffd}');
                    column += 1;
                }
                c => {
                    out.text.push(c);
                    column += c.width().unwrap_or(0);
                }
            }
        }
        if !kind.is_empty() && out.text.len() > start {
            out.spans.push((start..out.text.len(), kind));
        }
    }
    out
}

/// How many cells a drawn text takes: a wide glyph is two, as in `code`.
pub fn cells(text: &str) -> usize {
    text.width()
}

/// A stretch of a side's text: its highlight kind, and whether it is selected.
pub type Run = (Range<usize>, Option<&'static str>, bool);

/// `code`'s text cut where its highlight kinds or `selected` change, in order
/// and apart, leaving out the stretches of neither: what `StyledText` is told.
pub fn runs(code: &Code, selected: Option<Range<usize>>) -> Vec<Run> {
    let mut cuts: Vec<usize> = vec![0, code.text.len()];
    for (range, _) in &code.spans {
        cuts.extend([range.start, range.end]);
    }
    if let Some(range) = &selected {
        cuts.extend([range.start, range.end]);
    }
    cuts.retain(|cut| *cut <= code.text.len());
    cuts.sort_unstable();
    cuts.dedup();
    cuts.windows(2)
        .filter_map(|pair| {
            let range = pair[0]..pair[1];
            let kind = code.spans.iter().find(|(span, _)| span.start <= range.start && range.end <= span.end).map(|(_, kind)| *kind);
            let chosen = selected.as_ref().is_some_and(|sel| sel.start <= range.start && range.end <= sel.end);
            (kind.is_some() || chosen).then_some((range, kind, chosen))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::diff::tests::side;

    #[test]
    fn a_tab_stops_at_the_next_column_a_multiple_of_the_tab_width() {
        let drawn = code(&side(None, None, "context", &[("\tfn", ""), ("\ta", "k")]));
        assert_eq!(drawn.text, "    fn  a");
        assert_eq!(drawn.spans, [(6..9, "k")], "the tab belongs to the part it was typed in");
    }

    #[test]
    fn a_wide_glyph_counts_two_cells_before_the_tab_that_follows_it() {
        // Two cells, then a tab: it stops at 4, two spaces, where a narrow one would take three.
        let wide = code(&side(None, None, "context", &[("\u{4e2d}\tx", "")]));
        assert_eq!(wide.text, "\u{4e2d}  x");
        let narrow = code(&side(None, None, "context", &[("a\tx", "")]));
        assert_eq!(narrow.text, "a   x");
    }

    #[test]
    fn spans_follow_the_text_after_a_tab_widens_it_and_a_carriage_return_is_dropped() {
        let drawn = code(&side(None, None, "added", &[("a\t", ""), ("b", "s"), ("\r", "c")]));
        assert_eq!(drawn.text, "a   b");
        assert_eq!(drawn.spans, [(4..5, "s")], "an emptied part makes no span");
        for (range, _) in &drawn.spans {
            assert!(drawn.text.is_char_boundary(range.start) && drawn.text.is_char_boundary(range.end));
        }
    }

    #[test]
    fn a_control_character_is_shown_not_shaped() {
        assert_eq!(code(&side(None, None, "context", &[("a\u{1b}b", "")])).text, "a\u{fffd}b");
    }

    #[test]
    fn a_selection_cuts_the_highlight_runs_where_it_starts_and_ends() {
        let drawn = code(&side(None, None, "context", &[("abcd", ""), ("efgh", "k")]));
        assert_eq!(runs(&drawn, None), [(4..8, Some("k"), false)]);
        assert_eq!(
            runs(&drawn, Some(2..6)),
            [(2..4, None, true), (4..6, Some("k"), true), (6..8, Some("k"), false)]
        );
    }
}
