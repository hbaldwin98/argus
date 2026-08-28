//! The commit list, and the files each commit touched.

use super::*;

pub(super) fn render_history(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    let Some(view) = app.history.as_mut() else {
        return;
    };
    view.scroll_into_view(area.height as usize);

    let lines: Vec<Line> = view
        .rows
        .iter()
        .enumerate()
        .skip(view.top)
        .take(area.height as usize)
        .map(|(i, row)| history_line(view, *row, i == view.sel, th))
        .collect();

    f.render_widget(Paragraph::new(lines), area);
}

pub(super) fn history_line<'a>(view: &'a HistoryView, row: HistoryRow, selected: bool, th: Theme) -> Line<'a> {
    let commit = &view.commits[row.commit()];
    let spans = match row {
        HistoryRow::Commit { .. } => vec![
            // The fold marker is the only sign that a header has anything
            // under it: its files are not fetched until it is opened.
            Span::styled(
                format!("{} ", fold_marker(commit)),
                Style::default().fg(th.muted),
            ),
            Span::styled(
                format!(" {} ", commit.info.short),
                Style::default().fg(th.on_accent).bg(th.accent),
            ),
            Span::styled(
                format!(" {}", commit.info.summary),
                Style::default().fg(th.text).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}", commit.info.author),
                Style::default().fg(th.muted),
            ),
        ],
        HistoryRow::File { .. } => {
            let Some(file) = view.file_at(row) else {
                return Line::default();
            };
            let mut v = vec![
                Span::styled(
                    format!("  {} ", file.kind.marker()),
                    Style::default().fg(th.on_accent).bg(th.accent),
                ),
                Span::styled(format!(" {}", file.path), Style::default().fg(th.text)),
            ];
            if let Some(old) = &file.old_path {
                v.push(Span::styled(
                    format!(" ← {old}"),
                    Style::default().fg(th.muted),
                ));
            }
            v
        }
    };
    let mut line = Line::from(spans);
    if selected {
        line = line.style(Style::default().bg(th.sel_bg));
    }
    line
}

pub(super) fn fold_marker(commit: &crate::history::HistoryEntry) -> &'static str {
    match (commit.expanded, commit.pending) {
        (_, true) => "…",
        (true, _) => "▾",
        (false, _) => "▸",
    }
}
