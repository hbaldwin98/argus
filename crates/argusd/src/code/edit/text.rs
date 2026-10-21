//! A text's changes, positioned as the page's editor counts — UTF-16 code
//! units, with each CRLF read as one LF: applying a client's change set,
//! and the change set that turns one text into another, which is what a
//! write by anything else is sent as.

use std::time::Duration;

use argus_protocol::TextChange;
use similar::DiffOp;

/// How long a diff may look for the smallest change set before it settles
/// for a larger one: a rewrite of a megabyte file must not hold up the
/// next change.
const DIFF_TIMEOUT: Duration = Duration::from_millis(50);

/// `raw` with each CRLF a bare LF, and whether it had any, so a write puts
/// them back.
pub fn lf(raw: String) -> (String, bool) {
    if raw.contains("\r\n") {
        (raw.replace("\r\n", "\n"), true)
    } else {
        (raw, false)
    }
}

/// `text` as it goes to disk.
pub fn on_disk(text: &str, crlf: bool) -> std::borrow::Cow<'_, str> {
    if crlf {
        text.replace('\n', "\r\n").into()
    } else {
        text.into()
    }
}

/// `text` with `changes` made, each positioned in `text` as it was. A set
/// out of order, overlapping, past the end or splitting a character's
/// surrogate pair is refused whole rather than half made.
pub fn apply(text: &str, changes: &[TextChange]) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut seek = Seek::new(text);
    let mut copied = 0;
    let mut last = 0;
    for change in changes {
        if change.from < last || change.to < change.from {
            return Err("changes out of order or overlapping".into());
        }
        let from = seek.to(change.from)?;
        out.push_str(&text[copied..from]);
        out.push_str(&change.insert);
        copied = seek.to(change.to)?;
        last = change.to;
    }
    out.push_str(&text[copied..]);
    Ok(out)
}

/// The changes that turn `old` into `new`: a line diff, each run of
/// changed lines narrowed to the characters that differ, so a cursor on an
/// untouched part of a changed line stays where it was.
pub fn diff(old: &str, new: &str) -> Vec<TextChange> {
    let lines = similar::TextDiff::configure().algorithm(similar::Algorithm::Myers).timeout(DIFF_TIMEOUT).diff_lines(old, new);
    let old_at = starts(lines.old_slices());
    let new_at = starts(lines.new_slices());
    let mut seek = Seek::new(old);
    let mut changes = Vec::new();
    // Walked by each op's lengths, not its indices: with a lone CR among
    // the lines, similar has been seen to place an insertion one line early.
    let (mut o, mut n) = (0, 0);
    for op in lines.ops() {
        let (old_len, new_len) = (op.old_range().len(), op.new_range().len());
        let (old_lines, new_lines) = (o..o + old_len, n..n + new_len);
        o += old_len;
        n += new_len;
        if matches!(op, DiffOp::Equal { .. }) {
            continue;
        }
        let (mut old_from, mut old_to) = (old_at[old_lines.start], old_at[old_lines.end]);
        let (mut new_from, mut new_to) = (new_at[new_lines.start], new_at[new_lines.end]);
        let same = common_prefix(&old[old_from..old_to], &new[new_from..new_to]);
        old_from += same;
        new_from += same;
        let same = common_suffix(&old[old_from..old_to], &new[new_from..new_to]);
        old_to -= same;
        new_to -= same;
        if old_from == old_to && new_from == new_to {
            continue;
        }
        // Positions only grow from one run to the next, so the walk to
        // them goes over the text once.
        let from = seek.units_at(old_from);
        let to = seek.units_at(old_to);
        changes.push(TextChange { from, to, insert: new[new_from..new_to].to_owned() });
    }
    changes
}

/// Where each slice starts in the text they make, and where the last ends.
fn starts(slices: &[&str]) -> Vec<usize> {
    let mut at = Vec::with_capacity(slices.len() + 1);
    let mut sum = 0;
    at.push(0);
    for slice in slices {
        sum += slice.len();
        at.push(sum);
    }
    at
}

/// How many bytes, whole characters, two texts start with alike.
fn common_prefix(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).map(|(x, _)| x.len_utf8()).sum()
}

fn common_suffix(a: &str, b: &str) -> usize {
    a.chars().rev().zip(b.chars().rev()).take_while(|(x, y)| x == y).map(|(x, _)| x.len_utf8()).sum()
}

/// A walk forward through a text, keeping its byte and UTF-16 offsets in
/// step.
struct Seek<'a> {
    chars: std::str::CharIndices<'a>,
    len: usize,
    byte: usize,
    unit: u32,
}

impl<'a> Seek<'a> {
    fn new(text: &'a str) -> Self {
        Seek { chars: text.char_indices(), len: text.len(), byte: 0, unit: 0 }
    }

    /// The byte offset of UTF-16 position `unit`, at or after the last.
    fn to(&mut self, unit: u32) -> Result<usize, String> {
        while self.unit < unit {
            let Some((_, c)) = self.chars.next() else {
                return Err(format!("position {unit} is past the end of the text"));
            };
            self.unit += c.len_utf16() as u32;
            self.byte += c.len_utf8();
        }
        if self.unit > unit {
            return Err(format!("position {unit} splits a character"));
        }
        Ok(self.byte.min(self.len))
    }

    /// The UTF-16 position of byte offset `byte`, a character boundary at
    /// or after the last.
    fn units_at(&mut self, byte: usize) -> u32 {
        while self.byte < byte {
            let Some((_, c)) = self.chars.next() else { break };
            self.unit += c.len_utf16() as u32;
            self.byte += c.len_utf8();
        }
        self.unit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(from: u32, to: u32, insert: &str) -> TextChange {
        TextChange { from, to, insert: insert.into() }
    }

    #[test]
    fn positions_count_utf16_units_so_an_emoji_is_two() {
        // What the page's editor counts: 😀 is a surrogate pair there.
        assert_eq!(apply("a😀b", &[change(3, 4, "c")]).unwrap(), "a😀c");
        assert_eq!(apply("é😀", &[change(1, 3, "")]).unwrap(), "é");
    }

    #[test]
    fn a_change_that_splits_a_surrogate_pair_or_runs_past_the_end_is_refused() {
        assert!(apply("a😀b", &[change(2, 2, "x")]).is_err());
        assert!(apply("ab", &[change(1, 3, "")]).is_err());
        assert!(apply("abc", &[change(2, 3, ""), change(0, 1, "")]).is_err(), "out of order");
    }

    #[test]
    fn a_one_line_edit_is_sent_as_the_characters_it_changed() {
        let old = "fn a() {\n    one();\n}\n";
        let new = "fn a() {\n    two();\n}\n";
        assert_eq!(diff(old, new), [change(13, 16, "two")]);
    }

    #[test]
    fn crlf_is_read_as_lf_and_written_back() {
        let (text, crlf) = lf("a\r\nb\r\n".into());
        assert_eq!((text.as_str(), crlf), ("a\nb\n", true));
        assert_eq!(on_disk(&text, crlf), "a\r\nb\r\n");
        assert_eq!(lf("a\nb".into()), ("a\nb".into(), false));
    }

    mod properties {
        use super::*;
        use proptest::prelude::*;

        /// Text with lines, surrogate pairs and lone CRs among it: what
        /// moves positions apart from bytes and lines apart from breaks.
        fn text() -> impl Strategy<Value = String> {
            proptest::collection::vec(prop_oneof![4 => "[a-c ]{0,4}", 2 => Just("\n".to_owned()), 1 => Just("😀".to_owned()), 1 => Just("é".to_owned()), 1 => Just("\r".to_owned())], 0..24)
                .prop_map(|parts| parts.concat())
        }

        /// The set as UTF-16 units spliced: what `TextChange` says a set
        /// means (argus-protocol `code/edit`), written the plainest way.
        fn spliced(text: &str, changes: &[TextChange]) -> Option<String> {
            let units: Vec<u16> = text.encode_utf16().collect();
            let splits = |p: usize| p > 0 && p < units.len() && (0xD800..0xDC00).contains(&units[p - 1]);
            let mut out = Vec::new();
            let mut at = 0usize;
            for c in changes {
                let (from, to) = (c.from as usize, c.to as usize);
                if from < at || to < from || to > units.len() || splits(from) || splits(to) {
                    return None;
                }
                out.extend_from_slice(&units[at..from]);
                out.extend(c.insert.encode_utf16());
                at = to;
            }
            out.extend_from_slice(&units[at..]);
            String::from_utf16(&out).ok()
        }

        proptest! {
            /// Whatever writes the file, the client's copy must end up the
            /// disk's: `EditChanged` carries the changes from one version
            /// to the next (argus-protocol `code/edit`).
            #[test]
            fn the_diff_of_two_texts_turns_the_first_into_the_second(old in text(), new in text()) {
                let changes = diff(&old, &new);
                prop_assert_eq!(apply(&old, &changes).unwrap(), new);
            }

            /// A change set is sorted and none overlaps another, the order
            /// the page's editor reads one in (argus-protocol `code/edit`).
            #[test]
            fn a_diff_is_sorted_and_never_overlaps(old in text(), new in text()) {
                let changes = diff(&old, &new);
                let len = old.encode_utf16().count() as u32;
                let mut last = 0;
                for c in &changes {
                    prop_assert!(c.from >= last && c.to >= c.from && c.to <= len);
                    prop_assert!(c.from < c.to || !c.insert.is_empty(), "no empty change");
                    last = c.to;
                }
            }

            /// A client's set is made as its positions say, or refused whole:
            /// never a panic, never half made.
            #[test]
            fn a_set_is_applied_as_utf16_units_spliced_or_refused(
                old in text(),
                changes in proptest::collection::vec((0u32..40, 0u32..6, "[x😀\n]{0,3}"), 0..4),
            ) {
                let changes: Vec<TextChange> = changes.into_iter().map(|(from, len, insert)| change(from, from + len, &insert)).collect();
                match (apply(&old, &changes), spliced(&old, &changes)) {
                    (Ok(made), Some(expected)) => prop_assert_eq!(made, expected),
                    (Err(_), None) => {}
                    (made, expected) => prop_assert!(false, "apply {made:?}, splice {expected:?}"),
                }
            }
        }
    }
}
