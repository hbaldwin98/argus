//! The five-column spine and the cards it draws: how the widths are
//! shared out, how a column is scrolled to keep its selection on screen,
//! and what one row looks like.

use super::*;

/// The shared shape of the project and repository rows: the most urgent
/// state anywhere beneath, the name, what the row holds, what it owes, and
/// a badge counting the panes running under it.
pub(super) fn rollup_item<'a>(
    name: &str,
    contents: String,
    checkouts: impl Iterator<Item = &'a argus_protocol::CheckoutInfo>,
    notes: NoteCounts,
    has_note: bool,
    th: Theme,
) -> Item<'static> {
    let mut panes = 0usize;
    let mut status: Option<PaneStatus> = None;
    for c in checkouts {
        panes += c.listed_panes().count();
        if let Some(here) = worst_pane_status(c) {
            if status.is_none_or(|s| here.urgency() > s.urgency()) {
                status = Some(here);
            }
        }
    }
    let mut detail = vec![Span::styled(contents, Style::default().fg(th.dim))];
    detail.extend(note_detail(notes, has_note, th));
    let item = Item::new(
        vec![
            status_dot(status, th),
            Span::styled(
                name.to_string(),
                Style::default().fg(th.text).add_modifier(Modifier::BOLD),
            ),
        ],
        detail,
    );
    if panes == 0 {
        item
    } else {
        item.badged(vec![Span::styled(
            format!("{panes} ▣"),
            Style::default().fg(th.dim),
        )])
    }
}

/// Draws the normal five-column spine. The projects column may be folded
/// away to a tab in the left gutter, in which case its width is ceded to
/// the other four.
pub(super) fn render_columns(f: &mut Frame, app: &mut App, area: Rect) -> Option<CursorPlacement> {
    let th = app.theme;
    let collapsed = app.projects_collapsed;
    let constraints = if collapsed {
        collapsed_projects_constraints(area.width, app.column_widths.as_deref())
    } else {
        column_constraints(area.width, app.column_widths.as_deref())
    };
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .spacing(GUTTER_COLS)
        .constraints(constraints)
        .split(area);

    // When projects is folded away there is no leading card, so the
    // remaining four occupy `cols[0..4]`. `col(i)` is the expanded index.
    let col = |i: usize| cols[if collapsed { i - 1 } else { i }];

    app.layout.projects = if collapsed {
        render_collapsed_projects(f, area, th)
    } else {
        let project_rows: Vec<Item> = app
            .tree
            .iter()
            .map(|p| {
                // The rollup, not just this project's own note: from the
                // leftmost column the question is whether anything in
                // there is owed, not where it was written down.
                rollup_item(
                    &p.name,
                    plural(p.repositories.len(), "repository"),
                    p.repositories.iter().flat_map(|r| r.checkouts.iter()),
                    p.note_rollup(),
                    p.has_note,
                    th,
                )
            })
            .collect();
        // The projects column is scoped to the open workspace, so it says so
        // in its own title rather than leaving the scope to be inferred.
        let projects_title = if app.open_workspace.is_empty() {
            "projects".to_string()
        } else {
            format!("projects · {}", app.open_workspace)
        };
        render_column(
            f,
            col(0),
            &projects_title,
            project_rows,
            app.focus == Focus::Projects,
            (!app.tree.is_empty()).then_some(app.sel_project),
            app.layout.projects.first,
            "no projects yet

n  add one",
            th,
        )
    };

    let repository_rows: Vec<Item> = app
        .current_project()
        .map(|p| {
            p.repositories
                .iter()
                .map(|r| {
                    rollup_item(
                        &r.name,
                        plural(r.checkouts.len(), "checkout"),
                        r.checkouts.iter(),
                        r.note_rollup(),
                        false,
                        th,
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let nrepo = app
        .current_project()
        .map(|p| p.repositories.len())
        .unwrap_or(0);
    app.layout.repositories = render_column(
        f,
        col(1),
        "repositories",
        repository_rows,
        app.focus == Focus::Repositories,
        (nrepo > 0).then_some(app.sel_repository),
        app.layout.repositories.first,
        "no repositories

n  add one",
        th,
    );

    // The column's order is the app's, not either list's: the main branch
    // leads it whether it has a directory or not.
    let checkout_rows: Vec<Item> = app
        .current_repository()
        .map(|r| {
            app.checkout_rows()
                .into_iter()
                .filter_map(|row| match row {
                    CheckoutRow::Checkout(i) => r.checkouts.get(i).map(|c| checkout_item(c, th)),
                    CheckoutRow::Branch(i) => r.branches.get(i).map(|b| branch_item(b, th)),
                    CheckoutRow::Remote(i) => r.remote_branches.get(i).map(|b| remote_item(b, th)),
                })
                .collect()
        })
        .unwrap_or_default();
    let ncheck = app.checkout_row_count();
    app.layout.checkouts = render_column(
        f,
        col(2),
        "checkouts",
        checkout_rows,
        app.focus == Focus::Checkouts,
        (ncheck > 0).then_some(app.sel_checkout),
        app.layout.checkouts.first,
        "no checkouts",
        th,
    );

    let flat = app.settings.pane_view == crate::settings::PaneView::Flat;
    let pane_rows: Vec<Item> = app
        .pane_column_locations()
        .into_iter()
        .flat_map(|location| {
            let p = app
                .pane_at(location)
                .expect("pane column locations must point at panes");
            let flash = if app.pane_is_flashing(p.id) {
                Style::default().bg(th.sel_bg_dim)
            } else {
                Style::default()
            };
            let mut state = status_dot(Some(p.status), th);
            state.style = state.style.patch(flash);
            let detail = if flat {
                let (project, repository, checkout) = app
                    .pane_path(location)
                    .expect("pane locations must have a tree path");
                let mut detail = vec![Span::styled(
                    format!("{project} › {repository} › {checkout}  "),
                    Style::default().fg(th.accent),
                )];
                detail.extend(pane_detail(p, th));
                detail
            } else {
                pane_detail(p, th)
            };
            let parent = Item::new(
                vec![
                    state,
                    Span::styled(
                        p.title.clone(),
                        Style::default()
                            .fg(th.text)
                            .add_modifier(Modifier::BOLD)
                            .patch(flash),
                    ),
                    Span::styled(
                        exit_note(p.status),
                        Style::default().fg(th.err).patch(flash),
                    ),
                ],
                detail,
            )
            .badged(vec![Span::styled(
                format!("#{}", p.id.0),
                Style::default().fg(th.dim),
            )]);
            std::iter::once(parent).chain(p.children.iter().map(|c| child_item(c, th)))
        })
        .collect();
    // The selection is a pane, but the rows it sits among include the
    // children listed under each one, so the highlight has to be moved
    // onto the row that pane actually occupies.
    let selected = app.pane_location();
    let selected_row = pane_row_owners(app)
        .iter()
        .position(|owner| Some(*owner) == selected);
    app.layout.panes = render_column(
        f,
        col(3),
        if flat { "panes · all" } else { "panes" },
        pane_rows,
        app.focus == Focus::Panes,
        selected_row,
        app.layout.panes.first,
        "nothing running

s  shell
a  agent",
        th,
    );

    render_content(f, app, col(4), th)
}

pub(super) fn column_constraints(total_width: u16, preferred: Option<&[u16]>) -> Vec<Constraint> {
    let Some(mut widths) = preferred
        .filter(|widths| widths.len() == 5)
        .map(<[u16]>::to_vec)
    else {
        return vec![
            Constraint::Percentage(16),
            Constraint::Percentage(17),
            Constraint::Percentage(17),
            Constraint::Percentage(18),
            Constraint::Percentage(32),
        ];
    };

    let available = total_width.saturating_sub(GUTTER_COLS * 4);
    if available == 0 {
        return vec![Constraint::Length(0); 5];
    }
    let floor = MIN_COLUMN_WIDTH.min(available / 5).max(1);
    fit_widths(&mut widths, available, floor);
    widths.into_iter().map(Constraint::Length).collect()
}

/// The collapsed layout: no projects card, its width passing to the other
/// four columns. Gutters dragged before collapsing are absolute preferences,
/// not fractions, so the four survivors keep them as-is and the slack lands
/// in the live view; with nothing captured yet the default split is re-dealt
/// over four columns instead of five.
pub(super) fn collapsed_projects_constraints(total_width: u16, preferred: Option<&[u16]>) -> Vec<Constraint> {
    let available = total_width.saturating_sub(GUTTER_COLS * 3);
    if available == 0 {
        return vec![Constraint::Length(0); 4];
    }
    let floor = MIN_COLUMN_WIDTH.min(available / 4).max(1);
    let mut widths: Vec<u16> = match preferred.filter(|widths| widths.len() == 5) {
        Some(widths) => widths[1..].to_vec(),
        None => [20u32, 21, 21, 38]
            .iter()
            .map(|share| (u32::from(available) * share / 100).max(u32::from(floor)) as u16)
            .collect(),
    };
    fit_widths(&mut widths, available, floor);
    widths.into_iter().map(Constraint::Length).collect()
}

/// Reconciles preferred widths with what is actually available: nothing
/// below the floor, any shortfall reclaimed from the right, any spare
/// handed to the last column (the live view, where spare width does the
/// most good).
pub(super) fn fit_widths(widths: &mut [u16], available: u16, floor: u16) {
    for width in widths.iter_mut() {
        *width = (*width).max(floor);
    }
    let mut sum: u32 = widths.iter().map(|w| u32::from(*w)).sum();
    let available = u32::from(available);
    if sum < available {
        if let Some(last) = widths.last_mut() {
            *last = last.saturating_add((available - sum) as u16);
        }
    } else {
        for width in widths.iter_mut().rev() {
            if sum <= available {
                break;
            }
            let take = (sum - available).min(u32::from(width.saturating_sub(floor)));
            *width -= take as u16;
            sum -= take;
        }
    }
}

/// Renders one bordered column of rows and returns its inner (post-border)
/// area, so the caller can hit-test mouse clicks against the same rows.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_column(
    f: &mut Frame,
    area: Rect,
    title: &str,
    rows: Vec<Item>,
    focused: bool,
    selected: Option<usize>,
    scrolled_to: usize,
    empty_hint: &str,
    th: Theme,
) -> Panel {
    let block = panel_block(title, focused, th, area.width);
    let inner = block.inner(area);
    let mut panel = Panel {
        outer: area,
        inner,
        first: 0,
    };
    f.render_widget(block, area);

    if rows.is_empty() {
        // Wrapped, not truncated: these columns are narrow, and a hint cut
        // off mid-word ("no projects — ") tells the user nothing.
        f.render_widget(
            Paragraph::new(empty_hint)
                .style(Style::default().fg(th.dim))
                .wrap(Wrap { trim: true }),
            inner,
        );
        return panel;
    }

    // Scroll the window so the selection stays on screen in a long list.
    let visible = (inner.height / ROW_HEIGHT) as usize;
    let first = scrolled_to_show(scrolled_to, selected, visible, rows.len());
    panel.first = first;

    for (i, item) in rows.into_iter().enumerate().skip(first).take(visible) {
        let Some(row) = row_rect(inner, i - first) else {
            break;
        };
        render_row(f, row, item, selected == Some(i), focused, th);
    }
    panel
}

/// Where a column's window sits after this frame: `scrolled_to` is where
/// the last frame left it, and it moves as little as it can to keep the
/// selection on screen.
///
/// Deliberately not derived from the selection alone. Doing that pins the
/// selected row to the bottom of the card the moment the list is longer
/// than the card — nothing below the cursor is ever visible, every step
/// drags the whole list under it, and a row appearing above the selection
/// (a branch losing its checkout pins one) makes the column lurch.
pub(super) fn scrolled_to_show(
    scrolled_to: usize,
    selected: Option<usize>,
    visible: usize,
    len: usize,
) -> usize {
    if visible == 0 || len <= visible {
        return 0;
    }
    // Never leave blank rows below a list that could fill them.
    let mut first = scrolled_to.min(len - visible);
    if let Some(selected) = selected {
        if selected < first {
            first = selected;
        } else if selected >= first + visible {
            first = selected + 1 - visible;
        }
    }
    first
}

/// The projects column folded away: a disclosure mark in the page's left
/// gutter, sitting beside the first remaining card's top border. The rest
/// of that gutter is empty page, so nothing reads as a second column —
/// clicking it (or `p`) brings the column back. Losing the project name is
/// fine because the breadcrumb still carries it.
pub(super) fn render_collapsed_projects(f: &mut Frame, columns: Rect, th: Theme) -> Panel {
    let Some(x) = columns.x.checked_sub(GUTTER_COLS) else {
        return Panel::default();
    };
    let gutter = Rect {
        x,
        y: columns.y,
        width: GUTTER_COLS,
        height: columns.height,
    };
    let chip = Rect {
        x,
        y: columns.y,
        width: 1,
        height: 1,
    };
    f.render_widget(
        Paragraph::new(Span::styled(
            COLLAPSED_TAB,
            Style::default().fg(th.accent).bg(th.surface),
        )),
        chip,
    );
    Panel {
        outer: gutter,
        inner: chip,
        first: 0,
    }
}

/// A two-line item: name, then detail. The selection is a raised bar over
/// both lines with an accent marker pinning the first; unselected rows get
/// a blank gutter so text lines up either way. `dim` spans would sink into
/// the selection fill, so they are lifted to `muted` there.
pub(super) fn render_row(f: &mut Frame, area: Rect, item: Item, selected: bool, focused: bool, th: Theme) {
    let bar = match (selected, focused) {
        (true, true) => Style::default().bg(th.sel_bg),
        (true, false) => Style::default().bg(th.sel_bg_dim),
        _ => Style::default(),
    };

    let marker = if selected && focused {
        Span::styled(MARKER, Style::default().fg(th.accent).patch(bar))
    } else {
        Span::styled(GUTTER, bar)
    };
    fn lift<'a>(spans: Vec<Span<'a>>, bar: Style, selected: bool, th: Theme) -> Vec<Span<'a>> {
        spans
            .into_iter()
            .map(|s| {
                let mut style = s.style.patch(bar);
                if selected && style.fg == Some(th.dim) {
                    style = style.fg(th.muted);
                }
                Span::styled(s.content, style)
            })
            .collect()
    }

    let mut name = vec![marker];
    name.extend(lift(item.name, bar, selected, th));

    let badge = lift(item.badge, bar, selected, th);
    if !badge.is_empty() {
        let used: usize = name.iter().chain(badge.iter()).map(Span::width).sum();
        // One trailing column of air, and nothing at all if it won't fit.
        if let Some(pad) = (area.width as usize).checked_sub(used + 1) {
            name.push(Span::styled(" ".repeat(pad), bar));
            name.extend(badge);
        }
    }
    let mut detail = vec![Span::styled(GUTTER, bar), Span::styled("  ", bar)];
    detail.extend(lift(item.detail, bar, selected, th));

    let name = ellipsize_spans(name, area.width as usize);
    let detail = ellipsize_spans(detail, area.width as usize);

    f.render_widget(
        Paragraph::new(vec![Line::from(name), Line::from(detail)]).style(bar),
        area,
    );
}

/// A padded card. Focus has to be unmissable at a glance, so the focused
/// panel is lifted a step in elevation and given an accent border and
/// title, against the unfocused panels' receding edge and muted label.
pub(super) fn panel_block(title: &str, focused: bool, th: Theme, width: u16) -> Block<'_> {
    let (border, label, fill) = if focused {
        (
            Style::default().fg(th.accent),
            Style::default().fg(th.accent).add_modifier(Modifier::BOLD),
            th.surface_focus,
        )
    } else {
        (
            Style::default().fg(th.edge),
            Style::default().fg(th.muted),
            th.surface,
        )
    };
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border)
        .style(Style::default().bg(fill))
        // The inner gutter is what stops text from sitting on the border.
        .padding(Padding::new(1, 1, 1, 0))
        .title(Span::styled(
            format!(
                " {} ",
                ellipsize_text(title, width.saturating_sub(4) as usize)
            ),
            label,
        ))
}

pub(super) fn row_rect(inner: Rect, i: usize) -> Option<Rect> {
    row_rect_of(inner, i, ROW_HEIGHT)
}

pub(super) fn row_rect_of(inner: Rect, i: usize, height: u16) -> Option<Rect> {
    let offset = u16::try_from(i).ok()?.checked_mul(height)?;
    let y = inner.y.checked_add(offset)?;
    (y + height <= inner.y + inner.height).then(|| Rect::new(inner.x, y, inner.width, height))
}
