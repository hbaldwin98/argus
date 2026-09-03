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

/// Draws the spine. Its leading columns may be folded away to tabs in the
/// left gutter, in which case their width is ceded to the ones that remain.
pub(super) fn render_columns(f: &mut Frame, app: &mut App, area: Rect) -> Option<CursorPlacement> {
    let th = app.theme;
    let fold = app.fold;
    let hidden = fold.hidden();
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .spacing(GUTTER_COLS)
        .constraints(column_constraints(
            area.width,
            fold,
            app.column_widths.as_deref(),
        ))
        .split(area);

    // Folded columns draw no card, so the survivors occupy `cols[0..]`.
    // `col(i)` is the expanded index, the one the code below thinks in.
    let col = |i: usize| cols[i - hidden];

    // A row is two lines where the card can afford it, and one where the
    // detail line would cost an item. Every column shares the height, and
    // hit-testing needs the answer, so it is decided once here.
    let rows_high = row_height(area.height.saturating_sub(3));
    app.layout.row_height = rows_high;

    // Where each card was scrolled to last frame, read before the tabs
    // overwrite the panels a folded column leaves behind.
    let (projects_first, repositories_first) = (
        app.layout.projects.first,
        app.layout.repositories.first,
    );
    let tabs = render_fold_tabs(f, area, fold, th);
    app.layout.projects = tabs[0];
    app.layout.repositories = tabs[1];

    if !fold.hides(Focus::Projects) {
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
        app.layout.projects = render_column(
            f,
            col(0),
            &projects_title,
            project_rows,
            app.focus == Focus::Projects,
            (!app.tree.is_empty()).then_some(app.sel_project),
            projects_first,
            "no projects yet

n  add one",
            rows_high,
            th,
        );
    }

    if !fold.hides(Focus::Repositories) {
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
            repositories_first,
            "no repositories

n  add one",
            rows_high,
            th,
        );
    }

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
        rows_high,
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
        rows_high,
        th,
    );

    render_content(f, app, col(4), th)
}

/// Widths for the columns this fold still draws. Gutters dragged before
/// folding are absolute preferences, not fractions, so the survivors keep
/// them as-is and the slack lands in the live view; with nothing captured
/// yet the default split is re-dealt over however many columns are left.
pub(super) fn column_constraints(
    total_width: u16,
    fold: Fold,
    preferred: Option<&[u16]>,
) -> Vec<Constraint> {
    let shares: &[u32] = match fold {
        Fold::None => &[16, 17, 17, 18, 32],
        Fold::Projects => &[20, 21, 21, 38],
        Fold::Repositories => &[26, 27, 47],
    };
    spine_constraints(total_width, preferred, shares)
}

/// Shares `total_width` out over one column per entry in `shares`, honouring
/// the floors. `preferred` is the full five-column preference the user has
/// dragged, if any; its tail is taken when the leading columns are folded
/// away, since those widths are absolute rather than fractions.
fn spine_constraints(total_width: u16, preferred: Option<&[u16]>, shares: &[u32]) -> Vec<Constraint> {
    let columns = shares.len();
    let gutters = GUTTER_COLS * (columns as u16 - 1);
    let available = total_width.saturating_sub(gutters);
    if available == 0 {
        return vec![Constraint::Length(0); columns];
    }
    let (floor, content_floor) = floors(available, columns as u16);
    let mut widths: Vec<u16> = match preferred.filter(|w| w.len() == 5) {
        Some(w) => w[5 - columns..].to_vec(),
        None => shares
            .iter()
            .map(|share| (u32::from(available) * share / 100) as u16)
            .collect(),
    };
    fit_widths(&mut widths, available, floor, content_floor);
    widths.into_iter().map(Constraint::Length).collect()
}

/// The floors this many columns can actually be held to in `available`
/// cells. They are the real floors whenever the spine fits; on a terminal
/// too narrow even for that they scale down together, because a column
/// that has shrunk past legibility still beats one that is not drawn.
pub(super) fn floors(available: u16, columns: u16) -> (u16, u16) {
    let nav = columns.saturating_sub(1);
    let wanted = nav * MIN_COLUMN_WIDTH + MIN_CONTENT_WIDTH;
    if available >= wanted || wanted == 0 {
        return (MIN_COLUMN_WIDTH, MIN_CONTENT_WIDTH);
    }
    let scale = |n: u16| {
        ((u32::from(n) * u32::from(available)) / u32::from(wanted)).max(1) as u16
    };
    (scale(MIN_COLUMN_WIDTH), scale(MIN_CONTENT_WIDTH))
}

/// Reconciles preferred widths with what is actually available: nothing
/// below its floor, any spare handed to the last column (the live view,
/// where spare width does the most good).
///
/// Any shortfall is reclaimed from the nav columns first, rightmost first,
/// and only from the live view once they are all at their floor. Taking it
/// from the right unconditionally — which is the cheap way to write this —
/// means the pty is the first thing crushed on a narrow terminal, which is
/// backwards: a squeezed list is still a list, and a squeezed terminal is
/// a program that has stopped drawing.
pub(super) fn fit_widths(widths: &mut [u16], available: u16, floor: u16, content_floor: u16) {
    let last = widths.len().saturating_sub(1);
    let floor_of = |i: usize| if i == last { content_floor } else { floor };
    for (i, width) in widths.iter_mut().enumerate() {
        *width = (*width).max(floor_of(i));
    }
    let mut sum: u32 = widths.iter().map(|w| u32::from(*w)).sum();
    let available = u32::from(available);
    if sum < available {
        if let Some(width) = widths.last_mut() {
            *width = width.saturating_add((available - sum) as u16);
        }
        return;
    }
    // Nav columns, rightmost first, then the live view.
    let order = (0..last).rev().chain(std::iter::once(last));
    for i in order {
        if sum <= available {
            return;
        }
        let take = (sum - available).min(u32::from(widths[i].saturating_sub(floor_of(i))));
        widths[i] -= take as u16;
        sum -= take;
    }
    // Every column is at a floor and they still do not fit, which only
    // happens when `floors` itself had to round up off a tiny terminal.
    // Give away the remainder one cell at a time rather than letting the
    // last column overrun the screen and be clipped.
    while sum > available {
        let Some(i) = (0..widths.len()).rev().find(|i| widths[*i] > 1) else {
            break;
        };
        widths[i] -= 1;
        sum -= 1;
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
    height: u16,
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
    let visible = (inner.height / height.max(1)) as usize;
    let first = scrolled_to_show(scrolled_to, selected, visible, rows.len());
    panel.first = first;

    for (i, item) in rows.into_iter().enumerate().skip(first).take(visible) {
        let Some(row) = row_rect_of(inner, i - first, height) else {
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

/// The folded-away columns: a disclosure mark each in the page's left
/// gutter, stacked beside the first remaining card's top border. The rest
/// of that gutter is empty page, so nothing reads as a narrow column —
/// clicking a mark (or `p`) brings a column back. Losing the names is fine
/// because the live view's title still carries the whole path.
///
/// Returns a panel per leading column, in tree order, so a click lands on
/// something whether or not that column drew a card this frame.
pub(super) fn render_fold_tabs(f: &mut Frame, columns: Rect, fold: Fold, th: Theme) -> [Panel; 2] {
    let mut panels = [Panel::default(); 2];
    let Some(x) = columns.x.checked_sub(GUTTER_COLS) else {
        return panels;
    };
    let gutter = Rect {
        x,
        y: columns.y,
        width: GUTTER_COLS,
        height: columns.height,
    };
    for (i, panel) in panels.iter_mut().enumerate().take(fold.hidden()) {
        let chip = Rect {
            x,
            y: columns.y.saturating_add(i as u16),
            width: 1,
            height: 1,
        };
        if chip.y >= columns.y.saturating_add(columns.height) {
            break;
        }
        f.render_widget(
            Paragraph::new(Span::styled(
                COLLAPSED_TAB,
                Style::default().fg(th.accent).bg(th.surface),
            )),
            chip,
        );
        // The whole gutter is the click target; the mark is only where it
        // is drawn. One cell is too small to ask anyone to hit.
        *panel = Panel {
            outer: gutter,
            inner: chip,
            first: 0,
        };
    }
    panels
}

/// An item: name, then — when the card is tall enough to afford it — a
/// detail line. The selection is a raised bar over the whole row with an
/// accent marker pinning the first line; unselected rows get a blank
/// gutter so text lines up either way. `dim` spans would sink into the
/// selection fill, so they are lifted to `muted` there.
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

    let width = area.width as usize;
    let badge = lift(item.badge, bar, selected, th);
    let name = if badge.is_empty() {
        ellipsize_spans(name, width)
    } else {
        // The badge is a count, and a count survives truncation better than
        // the tail of a name does: "argus-cl…" with a `4 ▣` beside it says
        // more than the two extra letters would. So the badge is reserved
        // for first and the name ellipsized around it — unless doing that
        // would leave the name too short to identify anything, in which
        // case the badge is the part that goes.
        let badge_width: usize = badge.iter().map(Span::width).sum();
        match width
            .checked_sub(badge_width + 1)
            .filter(|room| *room >= NAME_FLOOR)
        {
            Some(room) => {
                let mut name = ellipsize_spans(name, room);
                let used: usize = name.iter().map(Span::width).sum();
                name.push(Span::styled(" ".repeat(width - used - badge_width), bar));
                name.extend(badge);
                name
            }
            None => ellipsize_spans(name, width),
        }
    };

    let mut lines = vec![Line::from(name)];
    if area.height >= ROW_HEIGHT {
        let mut detail = vec![Span::styled(GUTTER, bar), Span::styled("  ", bar)];
        detail.extend(lift(item.detail, bar, selected, th));
        lines.push(Line::from(ellipsize_spans(detail, width)));
    }

    f.render_widget(Paragraph::new(lines).style(bar), area);
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

pub(super) fn row_rect_of(inner: Rect, i: usize, height: u16) -> Option<Rect> {
    let offset = u16::try_from(i).ok()?.checked_mul(height)?;
    let y = inner.y.checked_add(offset)?;
    (y + height <= inner.y + inner.height).then(|| Rect::new(inner.x, y, inner.width, height))
}
