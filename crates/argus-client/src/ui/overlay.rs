//! The floating window over the columns, and the two views that only
//! ever appear in it: a note, and the settings panel.

use super::*;

/// How much of the screen a floating window takes. Big enough that vim is
/// usable, small enough that the tree still frames it — losing your place
/// is the thing the whole layout exists to prevent.
pub(super) const OVERLAY_FRACTION: (u16, u16) = (82, 78);

/// Returns where the hardware cursor belongs while an overlay is up. An
/// overlay covers the content column, so `None` here means the cursor is
/// not drawn at all this frame — the column underneath does not get to
/// keep it (see [`render`]).
pub(super) fn render_overlay(f: &mut Frame, app: &mut App, area: Rect, th: Theme) -> Option<CursorPlacement> {
    let Some(overlay) = &app.overlay else {
        app.layout.overlay = Panel::default();
        return None;
    };

    let width = (area.width * OVERLAY_FRACTION.0 / 100).max(20.min(area.width));
    let minimum_height = if matches!(overlay, Overlay::Settings { .. }) {
        // Two border rows and the panel's top padding sit outside the
        // setting lines and save-location footer.
        (Setting::ALL.len() as u16 * 3 + 3).min(area.height)
    } else {
        6.min(area.height)
    };
    let height = (area.height * OVERLAY_FRACTION.1 / 100).max(minimum_height);
    let popup = centered_rect(width, height, area);

    // The way out is in the title because a floating pane eats every other
    // key on purpose, and a window you cannot leave is worse than no window.
    let title = match overlay {
        Overlay::Pane { title, .. } => format!("{title}  ·  ctrl-space esc / F12 to close"),
        Overlay::Settings { .. } => "settings".to_string(),
        Overlay::Review => match app.review.as_ref() {
            Some(v) => match &v.review.commit {
                Some(c) => format!("review · {}  {}", c.short, c.summary),
                None => format!("review · {}", v.review.base.label()),
            },
            None => "review".to_string(),
        },
        Overlay::History => match app.history.as_ref() {
            Some(h) => format!("history · {} commits", h.commits.len()),
            None => "history".to_string(),
        },
        Overlay::Notes => match app.notes.as_ref() {
            // The mode is in the title because it changes what every key
            // does, and a modal surface that does not say which mode it is
            // in is a trap.
            Some(v) => format!(
                "note · {}  ·  {}",
                v.title,
                match v.mode {
                    NoteMode::View => "i edit · space tick · q close",
                    NoteMode::Insert => "INSERT · esc to save",
                }
            ),
            None => "note".to_string(),
        },
    };

    f.render_widget(Clear, popup);
    let block = panel_block(&title, true, th, popup.width);
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    app.layout.overlay = Panel {
        outer: popup,
        inner,
        first: 0,
    };

    match overlay {
        Overlay::Pane { pane, .. } => render_term(f, app.grids.get(pane), inner, true),
        Overlay::Settings { sel } => {
            render_settings(f, app, inner, *sel, th);
            None
        }
        Overlay::Review => {
            render_review(f, app, inner, th);
            None
        }
        Overlay::History => {
            render_history(f, app, inner, th);
            None
        }
        Overlay::Notes => render_notes(f, app, inner, th),
    }
}

/// The note, one line per line, with the checkbox lines picked out.
///
/// Returns where the hardware cursor goes: shown while typing, hidden in
/// view mode, where a block on a random character would read as a
/// selection rather than as an insertion point.
pub(super) fn render_notes(f: &mut Frame, app: &mut App, area: Rect, th: Theme) -> Option<CursorPlacement> {
    // One row of the window pays for the count footer.
    let body = Rect {
        height: area.height.saturating_sub(1),
        ..area
    };
    let view = app.notes.as_mut()?;
    view.follow_cursor(body.height as usize);
    let view = app.notes.as_ref()?;

    let mut lines: Vec<Line> = Vec::new();
    let todos = view.todos();
    for (i, text) in view
        .lines
        .iter()
        .enumerate()
        .skip(view.scroll)
        .take(body.height as usize)
    {
        let on_cursor = i == view.line;
        let bar = if on_cursor {
            Style::default().bg(th.sel_bg)
        } else {
            Style::default()
        };
        let state = todos.iter().find(|t| t.line == i).map(|t| t.state);
        lines.push(Line::from(note_line(text, state, bar, th)));
    }
    if view.body().is_empty() {
        lines = vec![Line::from(Span::styled(
            "empty — press i to write something",
            Style::default().fg(th.dim),
        ))];
    }
    f.render_widget(Paragraph::new(lines), body);

    let footer = Rect {
        y: area.y + area.height.saturating_sub(1),
        height: 1,
        ..area
    };
    let mut summary = match &view.error {
        Some(message) => vec![Span::styled(
            format!("not saved: {message}"),
            Style::default().fg(th.err),
        )],
        None => note_counts_spans(view.counts(), "  ", th),
    };
    if summary.is_empty() {
        summary.push(Span::styled(
            "no checkboxes yet — a line like \"- [ ] thing\" is counted",
            Style::default().fg(th.dim),
        ));
    }
    // Who else has been in this note. Dim and last, because it is
    // provenance rather than content — but present, since a note an agent
    // may write to is one a person has to be able to audit.
    if view.error.is_none() {
        if let Some(change) = view.last_agent_change(now_seconds()) {
            summary.push(Span::styled("  ", Style::default()));
            summary.push(Span::styled(change, Style::default().fg(th.dim)));
        }
    }
    f.render_widget(Paragraph::new(Line::from(summary)), footer);

    if view.mode != NoteMode::Insert {
        return None;
    }
    // The column is a character offset; the screen wants cells.
    let x: usize = view.lines[view.line]
        .chars()
        .take(view.column)
        .map(|c| Span::raw(c.to_string()).width())
        .sum();
    let row = view.line.checked_sub(view.scroll)?;
    Some(CursorPlacement {
        position: Position::new(
            body.x + (x as u16).min(body.width.saturating_sub(1)),
            body.y + (row as u16).min(body.height.saturating_sub(1)),
        ),
        // A bar, because this is an insertion point in text rather than a
        // terminal's own cursor.
        shape: argus_protocol::CursorShape::SteadyBar,
    })
}

/// The wall clock, for ages in the footer. The only clock reading in the
/// draw path, and it is here rather than passed in because nothing else on
/// screen depends on the time of day.
fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

/// One note line. A checkbox gets its marker coloured by state and its
/// text struck through once done; everything else is prose.
pub(super) fn note_line(text: &str, state: Option<TodoState>, bar: Style, th: Theme) -> Vec<Span<'static>> {
    let Some(state) = state else {
        return vec![Span::styled(
            text.to_string(),
            Style::default().fg(th.text).patch(bar),
        )];
    };
    // Split at the marker so only it is recoloured: the rest of the line
    // is the user's text and keeps its indent and bullet verbatim.
    let Some(open) = text.find('[') else {
        return vec![Span::styled(
            text.to_string(),
            Style::default().fg(th.text).patch(bar),
        )];
    };
    let (colour, glyph) = match state {
        TodoState::Open => (th.warn, "☐"),
        TodoState::Done => (th.ok, "☑"),
        TodoState::Pinned => (th.accent, "★"),
    };
    let rest = &text[open + 3..];
    let mut text_style = Style::default().fg(th.text).patch(bar);
    if state == TodoState::Done {
        text_style = Style::default()
            .fg(th.muted)
            .add_modifier(Modifier::CROSSED_OUT)
            .patch(bar);
    }
    vec![
        Span::styled(text[..open].to_string(), Style::default().fg(th.dim).patch(bar)),
        Span::styled(glyph.to_string(), Style::default().fg(colour).patch(bar)),
        Span::styled(rest.to_string(), text_style),
    ]
}

/// Counts as they are shown: what is outstanding first, because it is the
/// only one of the three that is a claim on anybody.
///
/// `gap` separates them — two spaces in the note's own footer, none in a
/// tree row, where the detail line is already carrying git status and a
/// column is thirteen characters wide.
pub(super) fn note_counts_spans(counts: NoteCounts, gap: &str, th: Theme) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut push = |text: String, colour| {
        if !spans.is_empty() {
            spans.push(Span::raw(gap.to_string()));
        }
        spans.push(Span::styled(text, Style::default().fg(colour)));
    };
    if counts.open > 0 {
        push(format!("☐{}", counts.open), th.warn);
    }
    if counts.pinned > 0 {
        push(format!("★{}", counts.pinned), th.accent);
    }
    if counts.done > 0 {
        push(format!("☑{}", counts.done), th.dim);
    }
    spans
}

/// What a tree row says about its note, appended to the row's detail line.
///
/// A note with nothing to count still says it exists: the point of the
/// mark is knowing there is something written down here, and "no open
/// items" is not the same as "nothing written".
pub(super) fn note_detail(counts: NoteCounts, has_note: bool, th: Theme) -> Vec<Span<'static>> {
    if !has_note && counts.is_empty() {
        return Vec::new();
    }
    let mut spans = vec![Span::raw("  ")];
    let counted = note_counts_spans(counts, " ", th);
    if counted.is_empty() {
        spans.push(Span::styled("✎", Style::default().fg(th.dim)));
    } else {
        spans.extend(counted);
    }
    spans
}

/// Each setting gets a name, its current value, and a line saying what
/// choosing it does — the reason a panel exists rather than another picker.
pub(super) fn render_settings(f: &mut Frame, app: &App, area: Rect, sel: usize, th: Theme) {
    let mut lines: Vec<Line> = Vec::new();
    for (i, setting) in Setting::ALL.iter().enumerate() {
        let selected = i == sel;
        let bar = if selected {
            Style::default().bg(th.sel_bg)
        } else {
            Style::default()
        };
        let (value, detail) = match setting {
            Setting::Editor => (
                app.settings.editor.label().to_string(),
                app.settings.editor.detail().to_string(),
            ),
            Setting::EditorCmd => {
                let value = if app.settings.editor_cmd.is_empty() {
                    "(from $VISUAL / $EDITOR)".to_string()
                } else {
                    app.settings.editor_cmd.clone()
                };
                (
                    value,
                    "the command to run, flags and all — enter to change".to_string(),
                )
            }
            Setting::PaneView => (
                app.settings.pane_view.label().to_string(),
                app.settings.pane_view.detail().to_string(),
            ),
            Setting::Theme => (
                app.settings.theme.clone(),
                "colours for the whole client".to_string(),
            ),
            Setting::Notifications => (
                app.settings.notifications.label().to_string(),
                app.settings.notifications.detail().to_string(),
            ),
        };

        let marker = if selected {
            Span::styled(MARKER, Style::default().fg(th.accent).patch(bar))
        } else {
            Span::styled(GUTTER, bar)
        };
        // Only a value you can cycle gets the arrows; free text would be
        // promising a carousel that isn't there.
        let cyclable = *setting != Setting::EditorCmd;
        let (open, close) = if cyclable {
            ("‹ ", " ›")
        } else {
            ("  ", "")
        };
        lines.push(Line::from(vec![
            marker,
            Span::styled(
                format!(" {:<16}", setting.label()),
                Style::default().fg(th.text).patch(bar),
            ),
            Span::styled(open, Style::default().fg(th.dim).patch(bar)),
            Span::styled(
                value,
                Style::default()
                    .fg(th.accent)
                    .add_modifier(Modifier::BOLD)
                    .patch(bar),
            ),
            Span::styled(close, Style::default().fg(th.dim).patch(bar)),
        ]));
        lines.push(Line::from(vec![
            Span::styled(GUTTER, bar),
            Span::styled(
                format!("  {detail}"),
                Style::default().fg(th.dim).patch(bar),
            ),
        ]));
        if i + 1 < Setting::ALL.len() {
            lines.push(Line::raw(""));
        }
    }

    lines.push(Line::from(vec![
        Span::styled(
            " save: ",
            Style::default().fg(th.dim).add_modifier(Modifier::ITALIC),
        ),
        Span::styled(
            crate::settings::path().display().to_string(),
            Style::default().fg(th.dim),
        ),
    ]));

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}
