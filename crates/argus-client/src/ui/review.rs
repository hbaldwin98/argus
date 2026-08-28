//! The diff viewer: one file at a time, stacked or side by side, with
//! syntax highlighting the daemon computed.

use super::*;

/// Drawn in the column the live pane uses, so the nav columns stay put
/// (DESIGN.md §9 M4).
/// The diff itself. The window around it is drawn by `render_overlay`,
/// which owns the border and the title.
pub(super) fn render_review(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    let Some(view) = app.review.as_mut() else {
        return;
    };
    view.scroll_into_view(area.height as usize);
    let (from, to) = view.selection();

    let lines: Vec<Line> = view
        .rows
        .iter()
        .enumerate()
        .skip(view.top)
        .take(area.height as usize)
        .map(|(i, row)| review_line(view, *row, i >= from && i <= to, area.width as usize, th))
        .collect();

    f.render_widget(Paragraph::new(lines), area);
}

/// Four digits covers nearly every file; the code matters more than the rest.
pub(super) const LINENO_WIDTH: usize = 4;

/// Splits one diff line into styled runs. Text no span covers is drawn plain,
/// which is most identifiers and all of any file with no grammar.
///
/// Offsets arrive from another process, so every one of them is checked
/// against this string before it is used to slice it. A bad span is dropped
/// rather than trusted: colour is not worth a panic in the renderer.
pub(super) fn highlighted<'a>(text: &'a str, spans: &[HighlightSpan], th: Theme) -> Vec<Span<'a>> {
    if spans.is_empty() {
        return vec![Span::styled(text, Style::default().fg(th.text))];
    }
    let mut out = Vec::with_capacity(spans.len() * 2 + 1);
    let mut at = 0usize;
    for span in spans {
        let (start, end) = (span.start as usize, span.end as usize);
        if start < at
            || end > text.len()
            || start >= end
            || !text.is_char_boundary(start)
            || !text.is_char_boundary(end)
        {
            continue;
        }
        if start > at {
            out.push(Span::styled(&text[at..start], Style::default().fg(th.text)));
        }
        out.push(Span::styled(&text[start..end], syntax_style(span.kind, th)));
        at = end;
    }
    if at < text.len() {
        out.push(Span::styled(&text[at..], Style::default().fg(th.text)));
    }
    out
}

pub(super) fn syntax_style(kind: HighlightKind, th: Theme) -> Style {
    let s = th.syntax;
    match kind {
        HighlightKind::Keyword => Style::default().fg(s.keyword),
        HighlightKind::Str => Style::default().fg(s.string),
        HighlightKind::Comment => Style::default()
            .fg(s.comment)
            .add_modifier(Modifier::ITALIC),
        HighlightKind::Number | HighlightKind::Constant => Style::default().fg(s.number),
        HighlightKind::Type => Style::default().fg(s.type_name),
        HighlightKind::Function => Style::default().fg(s.function),
        HighlightKind::Property => Style::default().fg(s.property),
        HighlightKind::Operator => Style::default().fg(s.operator),
        HighlightKind::Punctuation => Style::default().fg(s.punctuation),
    }
}

pub(super) fn review_line<'a>(
    view: &'a ReviewView,
    row: Row,
    selected: bool,
    width: usize,
    th: Theme,
) -> Line<'a> {
    let file = &view.review.files[row.file()];
    let pad = " ".repeat(LINENO_WIDTH + 1);

    let spans = match row {
        Row::File { .. } => {
            let mut v = vec![
                Span::styled(
                    format!(" {} ", file.kind.marker()),
                    Style::default().fg(th.on_accent).bg(th.accent),
                ),
                Span::styled(
                    format!(" {}", file.path),
                    Style::default().fg(th.text).add_modifier(Modifier::BOLD),
                ),
            ];
            if file.added_lines() + file.removed_lines() > 0 {
                v.push(Span::styled(
                    format!("  +{}", file.added_lines()),
                    Style::default().fg(th.ok),
                ));
                v.push(Span::styled(
                    format!(" -{}", file.removed_lines()),
                    Style::default().fg(th.err),
                ));
            }
            v
        }
        Row::Hunk { hunk, .. } => vec![Span::styled(
            format!("{pad}{}", file.hunks[hunk].header),
            Style::default().fg(th.muted),
        )],
        Row::Note { .. } => vec![Span::styled(
            format!("{pad}{}", file.note.as_deref().unwrap_or("")),
            Style::default().fg(th.dim).add_modifier(Modifier::ITALIC),
        )],
        // The only row that splits. Headers and notes span the width in
        // either view, and the wash below is skipped because each side
        // carries its own.
        Row::Pair {
            hunk, left, right, ..
        } => {
            // Cells, not bytes: the rule is a three-byte glyph.
            let half = width.saturating_sub(DIVIDER.chars().count()) / 2;
            let mut spans = diff_side(file, hunk, left, true, half, selected, th);
            spans.push(Span::styled(DIVIDER, Style::default().fg(th.edge)));
            spans.extend(diff_side(file, hunk, right, false, half, selected, th));
            spans
        }
        Row::Line { hunk, line, .. } => {
            let l = &file.hunks[hunk].lines[line];
            // The old side's number only where there is no new one.
            let no = match l.new_lineno.or(l.old_lineno) {
                Some(n) => format!("{n:>LINENO_WIDTH$}"),
                None => " ".repeat(LINENO_WIDTH),
            };
            // The marker keeps its colour even though the wash already says
            // which side this is: the wash is gone wherever a terminal drops
            // backgrounds, and the marker column is what is left.
            let marker_fg = match l.kind {
                LineKind::Added => th.ok,
                LineKind::Removed => th.err,
                LineKind::Context => th.dim,
            };
            let mut spans = vec![
                Span::styled(format!("{no} "), Style::default().fg(th.dim)),
                Span::styled(
                    crate::review::marker(l.kind).to_string(),
                    Style::default().fg(marker_fg),
                ),
            ];
            spans.extend(highlighted(&l.text, &l.spans, th));
            spans
        }
    };

    // A wash rather than a marker column: the left edge is already spent,
    // and a range should read as one block. Added and removed lines carry
    // their own wash whether or not they are selected, which is what frees
    // the foreground for syntax.
    let mut line = Line::from(spans);
    if let Row::Line {
        hunk, line: idx, ..
    } = row
    {
        if let Some(bg) = wash(file.hunks[hunk].lines[idx].kind, selected, th) {
            line = line.style(Style::default().bg(bg));
        }
    }
    line
}

/// Between the two sides. Air either side of the rule keeps one side's
/// wash from running into the other's.
pub(super) const DIVIDER: &str = " │ ";

/// One half of a split row, padded to exactly `width` so the divider and
/// the far side land in the same columns on every row. Each side is
/// ellipsized on its own: half a screen is narrower than a diff line often
/// is, and letting one side overrun would push the other off the row.
///
/// `old` picks which of the line's two numbers this side is showing, which
/// is the whole reason a split view can label both. Where a run of removals
/// is longer than the run that replaced it the far side has no line at all;
/// it is drawn recessed rather than blank, so the gap reads as absence
/// rather than as an empty line of code.
pub(super) fn diff_side<'a>(
    file: &'a FileDiff,
    hunk: usize,
    line: Option<usize>,
    old: bool,
    width: usize,
    selected: bool,
    th: Theme,
) -> Vec<Span<'a>> {
    let Some(i) = line else {
        return vec![Span::styled(
            " ".repeat(width),
            Style::default().bg(th.surface),
        )];
    };
    let l = &file.hunks[hunk].lines[i];
    let no = match if old { l.old_lineno } else { l.new_lineno } {
        Some(n) => format!("{n:>LINENO_WIDTH$}"),
        None => " ".repeat(LINENO_WIDTH),
    };
    let marker_fg = match l.kind {
        LineKind::Added => th.ok,
        LineKind::Removed => th.err,
        LineKind::Context => th.dim,
    };
    let mut spans = vec![
        Span::styled(format!("{no} "), Style::default().fg(th.dim)),
        Span::styled(
            crate::review::marker(l.kind).to_string(),
            Style::default().fg(marker_fg),
        ),
    ];
    spans.extend(highlighted(&l.text, &l.spans, th));

    let mut spans = ellipsize_spans(spans, width);
    let used: usize = spans.iter().map(Span::width).sum();
    spans.push(Span::raw(" ".repeat(width.saturating_sub(used))));
    if let Some(bg) = wash(l.kind, selected, th) {
        for span in &mut spans {
            span.style = span.style.bg(bg);
        }
    }
    spans
}

/// Which side of the diff a line is on, as a background. Selecting a line
/// brightens its wash rather than replacing it, so a selected range still
/// shows both sides.
pub(super) fn wash(kind: LineKind, selected: bool, th: Theme) -> Option<Color> {
    match (kind, selected) {
        (LineKind::Added, false) => Some(th.add_bg),
        (LineKind::Added, true) => Some(th.add_bg_sel),
        (LineKind::Removed, false) => Some(th.del_bg),
        (LineKind::Removed, true) => Some(th.del_bg_sel),
        (LineKind::Context, true) => Some(th.sel_bg),
        (LineKind::Context, false) => None,
    }
}
