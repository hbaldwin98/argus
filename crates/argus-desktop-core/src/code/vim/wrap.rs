//! `gq` and `gw`: lines wrapped to the text width. A port of the library's
//! hard wrap: a line longer than the width is broken at a blank near it,
//! and a line that, joined with the one below, still fits, is joined.

use crate::code::change::Change;

use super::Vim;

/// The blanks near column `max` of `line` at which to break it: before it,
/// else after it, else the nearest in the last `min` columns before; none
/// if the line is shorter than `max`.
fn find_space(line: &[char], max: usize, min: usize) -> Option<(usize, usize)> {
    if line.len() < max {
        return None;
    }
    let (before, after) = line.split_at(max);
    let blank = |c: &char| c.is_whitespace();
    // What `\s+$` or `\s+\S+$` leftmost-matches at the end of `before`:
    // the index of its blanks, their length, and the word after them if any.
    let space_before = {
        let trailing = before.iter().rev().take_while(|c| blank(c)).count();
        if trailing > 0 {
            Some((before.len() - trailing, trailing, None::<usize>))
        } else {
            let word = before.iter().rev().take_while(|c| !blank(c)).count();
            let blanks = before[..before.len() - word].iter().rev().take_while(|c| blank(c)).count();
            (blanks > 0).then_some((before.len() - word - blanks, blanks, Some(word)))
        }
    };
    // What `^(?:(\s+)|(\S+)(\s+))` matches at the start of `after`: blanks,
    // or a word and the blanks after it.
    let space_after = {
        let lead = after.iter().take_while(|c| blank(c)).count();
        if lead > 0 {
            Some((lead, None::<usize>, 0))
        } else {
            let word = after.iter().take_while(|c| !blank(c)).count();
            let blanks = after[word..].iter().take_while(|c| blank(c)).count();
            (word > 0 && blanks > 0).then_some((0, Some(word), blanks))
        }
    };
    let (mut start, mut end) = (0, 0);
    if let Some((index, blanks, None)) = space_before {
        start = index;
        end = max;
        debug_assert_eq!(index + blanks, max);
    }
    if let Some((lead, None, _)) = space_after {
        if start == 0 {
            start = max;
        }
        end = max + lead;
    }
    if start != 0 {
        return Some((start, end));
    }
    if let Some((index, blanks, Some(_))) = space_before {
        if index > min {
            return Some((index, index + blanks));
        }
    }
    if let Some((_, Some(word), blanks)) = space_after {
        let s = max + word;
        return Some((s, s + blanks));
    }
    None
}

impl Vim {
    /// Wraps lines `from` to `to`, to `max` characters; the row after the
    /// last that was looked at.
    pub(super) fn hard_wrap(&mut self, from: usize, to: usize, max: usize) -> usize {
        let mut row = from.min(to) as isize;
        let mut end_row = from.max(to) as isize;
        while row <= end_row {
            let t = self.buf.text();
            let line: Vec<char> = t.line_text(row as usize).chars().collect();
            if line.len() > max {
                // A break with only blanks before it leaves the rest as it was, under the
                // same indent: the library breaks it again for ever, so it is left.
                let space = find_space(&line, max, 5).filter(|&(start, _)| line[..start].iter().any(|c| !c.is_whitespace()));
                if let Some((start, end)) = space {
                    let indent: String = line.iter().take_while(|c| c.is_whitespace()).collect();
                    let (a, b) = (t.at(row as usize, start), t.at(row as usize, end));
                    self.buf.edit(&[Change { from: a, to: b, insert: format!("\n{indent}") }]);
                    end_row += 1;
                }
            } else if line.iter().any(|c| !c.is_whitespace()) && row != end_row {
                let next: Vec<char> = t.line_text(row as usize + 1).chars().collect();
                if next.iter().any(|c| !c.is_whitespace()) {
                    let trimmed_len = line.iter().rposition(|c| !c.is_whitespace()).map_or(0, |p| p + 1);
                    let lead = next.iter().take_while(|c| c.is_whitespace()).count();
                    let mut merged: Vec<char> = line[..trimmed_len].to_vec();
                    merged.push(' ');
                    merged.extend(&next[lead..]);
                    let space = find_space(&merged, max, 5);
                    if space.is_some_and(|(s, _)| s > trimmed_len) || merged.len() < max {
                        let a = t.at(row as usize, trimmed_len);
                        let b = t.at(row as usize + 1, lead);
                        self.buf.edit(&[Change { from: a, to: b, insert: " ".into() }]);
                        row -= 1;
                        end_row -= 1;
                    } else if trimmed_len < line.len() {
                        let a = t.at(row as usize, trimmed_len);
                        let b = t.line_end(row as usize);
                        self.buf.edit(&[Change { from: a, to: b, insert: String::new() }]);
                    }
                }
            }
            row += 1;
        }
        row.max(0) as usize
    }

    /// `gq` and `gw` over the lines `first` to `last`: `gq` leaves the cursor
    /// at the start of the last line wrapped, `gw` where it was.
    pub(super) fn wrap_lines(&mut self, first: usize, last: usize, linewise: bool, keep: bool) {
        if self.buf.read_only() {
            return;
        }
        let (line, col) = self.buf.text().pos(self.cursor);
        let max = super::options::Options::text_width(&self.locals);
        let mut end = self.hard_wrap(first, last, max);
        if end > first && linewise {
            end -= 1;
        }
        let t = self.buf.text();
        self.cursor = if keep { t.at(line.min(t.last_line()), col) } else { t.line_start(end.min(t.last_line())) };
        self.want = t.col(self.cursor);
    }
}

#[cfg(test)]
mod tests;
