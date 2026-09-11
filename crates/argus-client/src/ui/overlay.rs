//! The floating window over the columns, and the two views that only
//! ever appear in it: a brief, and the settings panel.

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
        Overlay::Brief => match app.brief.as_ref() {
            // The mode is in the title because it changes what every key
            // does, and a modal surface that does not say which mode it is
            // in is a trap.
            Some(v) => format!(
                "brief · {}  ·  {}",
                v.title,
                match v.mode {
                    BriefMode::View => "i edit · q close",
                    BriefMode::Insert => "INSERT · esc to save",
                }
            ),
            None => "brief".to_string(),
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
        Overlay::Pane { pane, .. } => render_term(
            f,
            app.grids.get(pane),
            inner,
            true,
            Some(*pane),
            app.selection.as_ref(),
        ),
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
        Overlay::Brief => render_brief(f, app, inner, th),
    }
}

/// The brief, one line per line, as marked-up prose.
///
/// Returns where the hardware cursor goes: shown while typing, hidden in
/// view mode, where a block on a random character would read as a
/// selection rather than as an insertion point.
pub(super) fn render_brief(f: &mut Frame, app: &mut App, area: Rect, th: Theme) -> Option<CursorPlacement> {
    let view = app.brief.as_mut()?;
    view.follow_cursor(area.height as usize);
    let view = app.brief.as_ref()?;

    let mut lines: Vec<Line> = Vec::new();
    for (i, text) in view
        .lines
        .iter()
        .enumerate()
        .skip(view.scroll)
        .take(area.height as usize)
    {
        let bar = if i == view.line {
            Style::default().bg(th.sel_bg)
        } else {
            Style::default()
        };
        lines.push(Line::from(crate::ui::prose::prose_spans(text, bar, th)));
    }
    if view.body().is_empty() {
        lines = vec![Line::from(Span::styled(
            "empty — press i to write something",
            Style::default().fg(th.dim),
        ))];
    }
    f.render_widget(Paragraph::new(lines), area);

    if view.mode != BriefMode::Insert {
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
            area.x + (x as u16).min(area.width.saturating_sub(1)),
            area.y + (row as u16).min(area.height.saturating_sub(1)),
        ),
        // A bar, because this is an insertion point in text rather than a
        // terminal's own cursor.
        shape: argus_protocol::CursorShape::SteadyBar,
    })
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
