//! Fitting text into a width: ellipsis, wrapping, insetting, centering.

use super::*;

pub(super) fn ellipsize_text(text: &str, width: usize) -> String {
    ellipsize_spans(vec![Span::raw(text.to_string())], width)
        .into_iter()
        .map(|span| span.content.into_owned())
        .collect()
}

pub(super) fn ellipsize_spans<'a>(spans: Vec<Span<'a>>, width: usize) -> Vec<Span<'a>> {
    if spans.iter().map(Span::width).sum::<usize>() <= width {
        return spans;
    }
    if width == 0 {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut remaining = width - 1;
    let mut ellipsis_style = Style::default();
    'spans: for span in spans {
        let style = span.style;
        ellipsis_style = style;
        for ch in span.content.chars() {
            let cell_width = Span::raw(ch.to_string()).width();
            if cell_width > remaining {
                break 'spans;
            }
            out.push(Span::styled(ch.to_string(), style));
            remaining -= cell_width;
        }
    }
    out.push(Span::styled("…", ellipsis_style));
    out
}

/// Shrinks a rect by `n` on every side, clamping rather than underflowing.
/// The page margin, given per axis, because a cell is about twice as tall
/// as it is wide: the same number on both reads as more space above and
/// below than beside. The columns need theirs wider anyway — the fold tabs
/// live in the left one.
pub(super) fn inset(area: Rect, x: u16, y: u16) -> Rect {
    Rect {
        x: area.x.saturating_add(x),
        y: area.y.saturating_add(y),
        width: area.width.saturating_sub(x * 2),
        height: area.height.saturating_sub(y * 2),
    }
}

/// Truncates from the left, keeping the end. The opposite of
/// [`ellipsize_text`], and the right choice for a path.
pub(super) fn elide_head(text: &str, width: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= width || width == 0 {
        return text.to_string();
    }
    let tail: String = chars[chars.len() + 1 - width..].iter().collect();
    format!("…{tail}")
}

/// Greedy word wrap. A word moves down whole when it will not fit; one
/// wider than the line itself is cut, because the alternative is a row
/// that overruns the box. Always returns at least one row, so an empty
/// field still has somewhere to put its caret.
pub(super) fn wrap(text: &str, width: u16) -> Vec<String> {
    let width = width.max(1) as usize;
    let mut rows: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut cur_w = 0usize;
    for ch in text.chars() {
        let w = Span::raw(ch.to_string()).width().max(1);
        if cur_w + w > width {
            let carry = match cur.rfind(' ') {
                Some(i) if i + 1 < cur.len() => cur.split_off(i + 1),
                _ => String::new(),
            };
            rows.push(std::mem::take(&mut cur));
            cur_w = Span::raw(carry.clone()).width();
            cur = carry;
        }
        cur.push(ch);
        cur_w += w;
    }
    rows.push(cur);
    rows
}

/// Text on one line: a value that holds a newline would otherwise break the
/// row it sits in.
pub(super) fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Prose onto rows `width` cells wide: [`wrap`] for each paragraph, with
/// the runs of whitespace inside it collapsed and the paragraph breaks
/// kept, which is what text an agent wrote needs and a typed field does not.
pub(super) fn wrap_words(text: &str, width: usize) -> Vec<String> {
    let width = width.clamp(1, u16::MAX as usize) as u16;
    let mut rows: Vec<String> = Vec::new();
    for paragraph in text.lines() {
        for row in wrap(&one_line(paragraph), width) {
            rows.push(row.trim_end().to_string());
        }
    }
    if rows.is_empty() {
        rows.push(String::new());
    }
    rows
}

pub(super) fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}

/// The exit code appended to a pane row, and only for a failure — a clean
/// exit is already said by the outlined box, and repeating it as text would
/// make every finished pane shout.
/// The detail line's word for a pane's state — the glyph's meaning spelled
/// out as well.
pub(super) fn plural(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("{n} {noun}")
    } else if let Some(stem) = noun.strip_suffix('y') {
        format!("{n} {stem}ies")
    } else {
        format!("{n} {noun}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prose_wraps_on_words_and_keeps_its_paragraphs() {
        assert_eq!(wrap_words("one  two three\n\nfour", 9), vec!["one two", "three", "", "four"]);
    }

    #[test]
    fn a_word_longer_than_the_row_is_broken_where_it_must_be() {
        assert_eq!(wrap_words("abcdefg", 3), vec!["abc", "def", "g"]);
    }

    #[test]
    fn empty_prose_is_still_one_row() {
        assert_eq!(wrap_words("", 5), vec![String::new()]);
    }
}
