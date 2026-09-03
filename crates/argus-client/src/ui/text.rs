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
/// The page margin: the horizontal gutter, and a single row top and
/// bottom. A screen is short and wide, so rows are the scarcer of the two
/// and the margin is not square.
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
