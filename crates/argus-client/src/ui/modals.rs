//! The modal input layers — the fuzzy picker, the directory browser, and
//! the typed prompt. Each covers what is under it and owns the caret.

use super::*;

/// Rows a fuzzy picker will show at once. Past this it scrolls, so a
/// 5000-file list is still a modal and not a wall.
pub(super) const PICKER_ROWS: usize = 12;

pub(super) fn render_picker(f: &mut Frame, app: &App, area: Rect, th: Theme) {
    let Some(picker) = &app.picker else { return };
    let fuzzy = picker.is_fuzzy();

    let rows = picker
        .len()
        .min(if fuzzy { PICKER_ROWS } else { usize::MAX });
    // Borders, the top pad, and the query line with its own blank beneath.
    let chrome = 3 + if fuzzy { 2 } else { 0 };
    let height = (rows as u16 + chrome as u16).min(area.height);
    let widest = picker
        .items
        .iter()
        .map(|i| i.chars().count())
        .max()
        .unwrap_or(0);
    let floor = if fuzzy { 44 } else { 28 };
    let width = (widest as u16 + 8).clamp(floor, 72).min(area.width);
    let popup = centered_rect(width, height, area);

    f.render_widget(Clear, popup);
    let block = panel_block(picker.title, true, th, popup.width);
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    if inner.height == 0 {
        return;
    }

    let list = if fuzzy {
        let mut spans = vec![Span::styled("› ", Style::default().fg(th.accent))];
        spans.extend(field(&picker.query, th).spans);
        if picker.query.is_empty() {
            spans.push(Span::styled(" type to filter", Style::default().fg(th.dim)));
        }
        f.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect { height: 1, ..inner },
        );
        // Skip the query line and the blank under it.
        Rect {
            y: inner.y + 2,
            height: inner.height.saturating_sub(2),
            ..inner
        }
    } else {
        inner
    };

    // Scroll so the cursor stays on screen once the list outgrows the box.
    let visible = list.height as usize;
    let first = if picker.sel >= visible {
        picker.sel + 1 - visible
    } else {
        0
    };

    for slot in 0..visible {
        let i = first + slot;
        if i >= picker.len() {
            break;
        }
        let Some(row) = row_rect_of(list, slot, 1) else {
            break;
        };
        render_row(
            f,
            row,
            picker_item(picker, i, th),
            i == picker.sel,
            true,
            th,
        );
    }
}

/// One picker row. The last row of a branch picker can be the offer to
/// create the branch whose name you just typed.
pub(super) fn picker_item<'a>(picker: &'a crate::app::Picker, i: usize, th: Theme) -> Item<'a> {
    if let (Some(name), true) = (&picker.create, i == picker.shown.len()) {
        return Item::new(
            vec![
                Span::styled("+ ", Style::default().fg(th.ok)),
                Span::styled("create ", Style::default().fg(th.dim)),
                Span::styled(name.clone(), Style::default().fg(th.ok)),
            ],
            Vec::new(),
        );
    }
    let name = picker
        .shown
        .get(i)
        .and_then(|idx| picker.items.get(*idx))
        .cloned()
        .unwrap_or_default();
    Item::new(
        vec![Span::styled(name, Style::default().fg(th.text))],
        Vec::new(),
    )
}

/// The directory browser. Wider than a picker and taller than a prompt,
/// because it has to show three things at once: where you are, what is
/// under it, and which of those are repositories.
pub(super) fn render_dir_picker(f: &mut Frame, app: &App, area: Rect, th: Theme) {
    let Some(picker) = &app.dir_picker else {
        return;
    };

    let rows = picker.len().min(PICKER_ROWS);
    // Borders, the block's top pad, breadcrumb, query, the blank under it,
    // and the key hint.
    let height = (rows as u16 + 7).min(area.height);
    let width = 64.min(area.width);
    let popup = centered_rect(width, height, area);

    f.render_widget(Clear, popup);
    let block = panel_block(picker.title(), true, th, popup.width);
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    if inner.height < 3 {
        return;
    }

    // The tail of the path, not its head: the segments nearest the cursor
    // are the ones that tell you where you are.
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            elide_head(&picker.path, inner.width as usize),
            Style::default().fg(th.muted),
        ))),
        Rect { height: 1, ..inner },
    );

    let mut query = vec![Span::styled("› ", Style::default().fg(th.accent))];
    query.extend(field(&picker.query, th).spans);
    if picker.query.is_empty() {
        query.push(Span::styled(" type to filter", Style::default().fg(th.dim)));
    }
    f.render_widget(
        Paragraph::new(Line::from(query)),
        Rect {
            y: inner.y + 1,
            height: 1,
            ..inner
        },
    );

    let hint_y = inner.y + inner.height - 1;
    let list = Rect {
        y: inner.y + 3,
        height: hint_y.saturating_sub(inner.y + 3),
        ..inner
    };

    if let Some(error) = &picker.error {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                ellipsize_text(error, list.width as usize),
                Style::default().fg(th.err),
            ))),
            Rect { height: 1, ..list },
        );
    } else {
        let visible = list.height as usize;
        let first = picker.sel.saturating_sub(visible.saturating_sub(1));
        for slot in 0..visible {
            let i = first + slot;
            let Some(row) = picker.row(i) else { break };
            let Some(rect) = row_rect_of(list, slot, 1) else {
                break;
            };
            render_row(
                f,
                rect,
                dir_item(row, picker.here_label(), th),
                i == picker.sel,
                true,
                th,
            );
        }
    }

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(
                "enter open   ← up   enter {} on ·   esc cancel",
                picker.here_action()
            ),
            Style::default().fg(th.dim),
        ))),
        Rect {
            y: hint_y,
            height: 1,
            ..inner
        },
    );
}

pub(super) fn dir_item(row: &DirRow, here: &str, th: Theme) -> Item<'static> {
    match row {
        DirRow::Here => Item::new(
            vec![
                Span::styled("· ", Style::default().fg(th.accent)),
                Span::styled(here.to_string(), Style::default().fg(th.text)),
            ],
            Vec::new(),
        ),
        DirRow::Child { name, is_repo } => {
            let item = Item::new(
                vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(name.clone(), Style::default().fg(th.text)),
                ],
                Vec::new(),
            );
            // Which children are repositories is invisible from the name,
            // and is usually the whole question being asked here.
            if *is_repo {
                item.badged(vec![Span::styled("git", Style::default().fg(th.ok))])
            } else {
                item
            }
        }
    }
}

/// How wide a prompt box gets. A comment is a sentence and gets the wider
/// box; everything else here is an identifier — a branch name, a path — and
/// a wide box would only be a wide box.
pub(super) const PROMPT_WIDTH: u16 = 54;
pub(super) const COMMENT_WIDTH: u16 = 76;
/// How tall the text being typed may grow before the box stops growing and
/// starts scrolling instead. A prompt is still a modal: it must not become
/// the screen.
pub(super) const PROMPT_MAX_ROWS: usize = 8;

/// The modal for all five prompts, drawn over everything else. Destructive
/// confirmations are tinted `err` so a removal never looks like a text
/// field you can dismiss by typing.
///
/// Text wraps rather than running off the edge, and the box grows with it:
/// a field you cannot read back is a field you cannot check before sending.
pub(super) fn render_prompt(f: &mut Frame, app: &App, area: Rect, th: Theme) {
    let Some(prompt) = &app.prompt else { return };

    let wanted = match prompt {
        Prompt::Comment { .. } => COMMENT_WIDTH,
        _ => PROMPT_WIDTH,
    };
    let width = wanted.min(area.width.saturating_sub(2));
    // Borders and the gutter inside them, which the block below pads.
    let inner_width = width.saturating_sub(4);

    let (title, body, hint, danger) = match prompt {
        Prompt::NewWorktree { input, .. } => (
            "new worktree",
            wrapped_field(input, inner_width, th),
            "enter create   esc cancel",
            false,
        ),
        Prompt::NewRepository { parent, input, .. } => {
            // The path it will land at, resolved as you type: the name
            // alone would leave the one thing worth checking — where this
            // is going — off the screen that asks for it.
            let name = input.trim();
            let dest = if name.is_empty() {
                parent.clone()
            } else {
                crate::dirpicker::join(parent, name)
            };
            let mut lines = vec![Line::from(Span::styled(
                elide_head(&dest, inner_width as usize),
                Style::default().fg(th.muted),
            ))];
            lines.extend(wrapped_field(input, inner_width, th));
            (
                "new repository",
                lines,
                "empty to use the directory itself   enter create   esc cancel",
                false,
            )
        }
        Prompt::EditorCommand { input } => (
            "editor command",
            wrapped_field(input, inner_width, th),
            "empty to use $EDITOR   enter save   esc cancel",
            false,
        ),
        Prompt::Comment { anchor, input } => {
            // The anchor gets lines of its own. Sharing one with the text
            // left a long path only a few columns to type a sentence in.
            let where_ = anchor.notification("");
            let mut lines = vec![Line::from(Span::styled(
                ellipsize_text(where_.trim_end_matches([' ', ':']), inner_width as usize),
                Style::default().fg(th.muted),
            ))];
            lines.extend(wrapped_field(input, inner_width, th));
            (
                "comment to the agent",
                lines,
                "enter send   esc cancel",
                false,
            )
        }
        Prompt::ConfirmRemove { target, label } => {
            let (title, detail) = target.wording();
            (
                title,
                vec![Line::from(vec![
                    Span::styled(
                        label.clone(),
                        Style::default().fg(th.text).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(detail, Style::default().fg(th.muted)),
                ])],
                "y/enter remove   n/esc cancel",
                true,
            )
        }
    };

    // Borders, the body, and the hint under it.
    let height = (body.len() as u16 + 3).min(area.height);
    let popup = centered_rect(width, height, area);

    f.render_widget(Clear, popup);
    let accent = if danger { th.err } else { th.accent };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent))
        .padding(Padding::horizontal(1))
        .title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(th.on_accent)
                .bg(accent)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    let mut lines = body;
    lines.push(Line::from(Span::styled(hint, Style::default().fg(th.dim))));
    f.render_widget(Paragraph::new(lines), inner);
}

/// A text field with a visible caret. Empty fields show nothing but the
/// caret, so there is always something to look at.
pub(super) fn field(input: &str, th: Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(input.to_string(), Style::default().fg(th.text)),
        Span::styled(CARET, Style::default().fg(th.accent)),
    ])
}

/// The same field wrapped to `width`, showing its last [`PROMPT_MAX_ROWS`]
/// rows. The tail is what survives because the caret is there: a prompt
/// that scrolls away from what you are typing is the bug this exists to
/// avoid.
pub(super) fn wrapped_field(input: &str, width: u16, th: Theme) -> Vec<Line<'static>> {
    let mut rows = wrap(input, width);
    // The caret needs a cell of its own, and a row filled to the edge has
    // none left.
    if rows
        .last()
        .is_none_or(|r| Span::raw(r.clone()).width() >= width.max(1) as usize)
    {
        rows.push(String::new());
    }
    let last = rows.len() - 1;
    rows.into_iter()
        .enumerate()
        .skip(last.saturating_sub(PROMPT_MAX_ROWS - 1))
        .map(|(i, row)| {
            let mut spans = vec![Span::styled(row, Style::default().fg(th.text))];
            if i == last {
                spans.push(Span::styled(CARET, Style::default().fg(th.accent)));
            }
            Line::from(spans)
        })
        .collect()
}
