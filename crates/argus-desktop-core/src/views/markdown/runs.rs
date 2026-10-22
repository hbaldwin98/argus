//! A run of inline marks flattened into one string and the stretches of it that are marked, which
//! is what a window that draws text as one wrapped run needs, and what a selection and a click are
//! read against.

use std::ops::Range;

use super::model::Inline;

/// What marks a stretch of text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Marks {
    pub strong: bool,
    pub em: bool,
    pub del: bool,
    pub code: bool,
    /// An index of [`Styled::links`].
    pub link: Option<usize>,
}

/// Inline marks as one string, a break as `\n`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Styled {
    pub text: String,
    /// Byte ranges of `text`, in order and apart, each with marks; what has none is not listed.
    pub spans: Vec<(Range<usize>, Marks)>,
    /// The URL of each link, in the order met.
    pub links: Vec<String>,
}

/// A stretch of the text with one set of marks and one selection state: what a run is cut into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cut {
    pub range: Range<usize>,
    pub marks: Marks,
    pub selected: bool,
}

impl Styled {
    pub fn of(inlines: &[Inline]) -> Self {
        let mut out = Self::default();
        out.add(inlines, Marks::default());
        out
    }

    fn add(&mut self, inlines: &[Inline], marks: Marks) {
        for inline in inlines {
            match inline {
                Inline::Text(text) => self.push(text, marks),
                Inline::Code(text) => self.push(text, Marks { code: true, ..marks }),
                Inline::Break => self.push("\n", marks),
                Inline::Strong(inner) => self.add(inner, Marks { strong: true, ..marks }),
                Inline::Em(inner) => self.add(inner, Marks { em: true, ..marks }),
                Inline::Del(inner) => self.add(inner, Marks { del: true, ..marks }),
                Inline::Link { url, children } => {
                    self.links.push(url.clone());
                    let link = Some(self.links.len() - 1);
                    self.add(children, Marks { link, ..marks });
                }
            }
        }
    }

    fn push(&mut self, text: &str, marks: Marks) {
        if text.is_empty() {
            return;
        }
        let start = self.text.len();
        self.text.push_str(text);
        if marks == Marks::default() {
            return;
        }
        match self.spans.last_mut() {
            Some((range, last)) if range.end == start && *last == marks => range.end = self.text.len(),
            _ => self.spans.push((start..self.text.len(), marks)),
        }
    }

    /// The URL of the link the byte `at` of the text is in.
    pub fn link_at(&self, at: usize) -> Option<&str> {
        self.spans.iter().find(|(range, _)| range.contains(&at)).and_then(|(_, marks)| marks.link).map(|link| self.links[link].as_str())
    }

    /// The whole text cut where its marks or `selected` change, in order and apart.
    pub fn cuts(&self, selected: Option<Range<usize>>) -> Vec<Cut> {
        let len = self.text.len();
        let mut points = vec![0, len];
        for (range, _) in &self.spans {
            points.extend([range.start, range.end]);
        }
        if let Some(range) = &selected {
            points.extend([range.start.min(len), range.end.min(len)]);
        }
        points.sort_unstable();
        points.dedup();
        // Spans are in order and apart, and so are the windows: one cursor serves both, so a long
        // paragraph is cut in a single pass.
        let mut span = 0;
        points
            .windows(2)
            .map(|pair| {
                let range = pair[0]..pair[1];
                while self.spans.get(span).is_some_and(|(s, _)| s.end <= range.start) {
                    span += 1;
                }
                let marks = self.spans.get(span).filter(|(s, _)| s.start <= range.start && range.end <= s.end).map(|(_, marks)| *marks).unwrap_or_default();
                let chosen = selected.as_ref().is_some_and(|sel| sel.start <= range.start && range.end <= sel.end);
                Cut { range, marks, selected: chosen }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::markdown::inline::parse;

    #[test]
    fn marks_become_one_string_and_the_stretches_that_carry_them() {
        let styled = Styled::of(&parse("a **b _c_** `d`\ne"));
        assert_eq!(styled.text, "a b c d\ne");
        let strong = Marks { strong: true, ..Marks::default() };
        assert_eq!(
            styled.spans,
            [(2..4, strong), (4..5, Marks { strong: true, em: true, ..Marks::default() }), (6..7, Marks { code: true, ..Marks::default() })]
        );
    }

    #[test]
    fn a_link_names_its_url_for_a_byte_inside_it_and_none_outside() {
        let styled = Styled::of(&parse("go [there](https://a.b/c) now"));
        assert_eq!(styled.text, "go there now");
        assert_eq!(styled.link_at(3), Some("https://a.b/c"));
        assert_eq!(styled.link_at(7), Some("https://a.b/c"));
        assert_eq!(styled.link_at(2), None);
        assert_eq!(styled.link_at(8), None);
    }

    #[test]
    fn each_cut_carries_the_marks_of_the_span_it_lies_in_across_many_spans() {
        let styled = Styled::of(&parse("a **b** c _d_ `e` f"));
        let marked: Vec<_> = styled.cuts(Some(3..5)).iter().map(|c| (c.range.clone(), c.marks.strong, c.marks.em, c.marks.code)).collect();
        assert_eq!(marked, [(0..2, false, false, false), (2..3, true, false, false), (3..5, false, false, false), (5..6, false, false, false), (6..7, false, true, false), (7..8, false, false, false), (8..9, false, false, true), (9..11, false, false, false)]);
    }

    #[test]
    fn cuts_cover_the_text_and_split_where_a_selection_starts_and_ends() {
        let styled = Styled::of(&parse("ab **cd** ef"));
        let cuts = styled.cuts(Some(1..5));
        assert_eq!(cuts.iter().map(|c| (c.range.clone(), c.selected)).collect::<Vec<_>>(), [(0..1, false), (1..3, true), (3..5, true), (5..8, false)]);
        assert!(cuts[2].marks.strong && !cuts[1].marks.strong);
        assert!(styled.cuts(None).iter().all(|c| !c.selected));
        assert!(Styled::default().cuts(None).is_empty());
    }
}
