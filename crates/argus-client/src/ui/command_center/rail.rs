//! The contextual rail: the project heading, the selected project's
//! repositories with a card for each of the open one's checkouts holding its
//! panes, and the line the quiet repositories fold behind, with the
//! hit-tests that map a click back onto a row.

use super::*;
use crate::app::QuietFold;

/// Blank cells every row keeps before the rail's border, so the column of
/// counts and status words does not sit against the rule.
const RIGHT_PAD: usize = 2;

/// The rail row a click landed on. The project heading is drawn above the
/// list and the fold's line is not a row the cursor can be on, so each is
/// hit-tested on its own ([`project_header_at`], [`quiet_fold_at`]).
pub(crate) fn rail_target_at(app: &App, x: u16, y: u16) -> Option<RailRow> {
    match list_row_at(app, x, y)? {
        ListRow::Rail(row) => Some(row),
        ListRow::Child(pane, _) => Some(RailRow::Pane(pane)),
        ListRow::CardTop(repository, checkout) | ListRow::CardBottom(repository, checkout) => {
            Some(RailRow::Checkout(repository, checkout))
        }
        ListRow::QuietFold(_) => None,
    }
}

/// Whether a click landed on the line the quiet repositories fold behind.
pub(crate) fn quiet_fold_at(app: &App, x: u16, y: u16) -> bool {
    matches!(list_row_at(app, x, y), Some(ListRow::QuietFold(_)))
}

fn list_row_at(app: &App, x: u16, y: u16) -> Option<ListRow> {
    let panel = app.layout.rail;
    if !contains(panel.inner, x, y) {
        return None;
    }
    list_rows(app)
        .get(panel.first + usize::from(y - panel.inner.y))
        .copied()
}

/// A row drawn in the scrolling list: one of the rows the cursor walks, or
/// one it steps over. A child listed under an agent's row — a CLI started
/// inside it — is something happening inside its pane rather than somewhere
/// else to go, so a click on it lands on the pane. A card's top and bottom
/// edges are its checkout's, so a click on one lands there. The fold's line
/// stands for rows rather than being one.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ListRow {
    Rail(RailRow),
    Child(PaneLocation, usize),
    CardTop(usize, usize),
    CardBottom(usize, usize),
    QuietFold(QuietFold),
}

/// The rows drawn in the scrolling list: everything but the heading, each
/// checkout boxed in a card with its panes, each pane's children under it,
/// and the fold's line where `app/rail` puts it.
fn list_rows(app: &App) -> Vec<ListRow> {
    let fold = app.quiet_fold();
    let mut rows = Vec::new();
    // The card being filled: it closes on the first row that is not one of
    // its panes.
    let mut card = None;
    for row in app.rail_rows() {
        if !matches!(row, RailRow::Pane(_)) {
            rows.extend(card.take().map(|(r, c)| ListRow::CardBottom(r, c)));
        }
        match row {
            RailRow::Project => continue,
            RailRow::Repository(index) => {
                let lid = fold.filter(|fold| fold.above == Some(index));
                rows.extend(lid.map(ListRow::QuietFold));
                rows.push(ListRow::Rail(row));
            }
            RailRow::Pane(pane) => {
                rows.push(ListRow::Rail(row));
                let children = app.pane_at(pane).map_or(0, |info| info.children.len());
                rows.extend((0..children).map(|child| ListRow::Child(pane, child)));
            }
            RailRow::Checkout(repository, checkout) => {
                rows.push(ListRow::CardTop(repository, checkout));
                rows.push(ListRow::Rail(row));
                card = Some((repository, checkout));
            }
        }
    }
    rows.extend(card.map(|(r, c)| ListRow::CardBottom(r, c)));
    let trailing = fold.filter(|fold| fold.above.is_none());
    rows.extend(trailing.map(ListRow::QuietFold));
    rows
}

pub(crate) fn sidebar_contains(app: &App, x: u16, y: u16) -> bool {
    contains(app.layout.rail.outer, x, y)
}

/// The project heading, the rail's first row: clicking it opens the project
/// picker, since the rail shows one project at a time.
pub(crate) fn project_header_at(app: &App, x: u16, y: u16) -> bool {
    let outer = app.layout.rail.outer;
    contains(outer, x, y) && y == outer.y
}

pub(super) fn render_sidebar(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    f.render_widget(
        Block::default()
            .borders(Borders::RIGHT)
            .border_style(Style::default().fg(th.edge))
            .style(Style::default().bg(th.bg)),
        area,
    );
    let inner = Rect {
        width: area.width.saturating_sub(1),
        ..area
    };
    let heading = Rect {
        height: inner.height.min(1),
        ..inner
    };
    render_heading(f, app, heading, th);
    let list = Rect {
        y: heading.bottom(),
        height: inner.height - heading.height,
        ..inner
    };
    render_repositories(f, app, list, th);
    // The rule under the heading meets the border from the left only: the
    // stage's own heading rule is a row lower, so a tee facing right would
    // point at nothing.
    if list.y < area.bottom() && area.width > 0 {
        f.render_widget(
            Paragraph::new(Span::styled("┤", Style::default().fg(th.edge))),
            Rect::new(area.right() - 1, list.y, 1, 1),
        );
    }
    // Repository rows are the rail's interactive interior, while the whole
    // rail remains its hit target and geometry landmark.
    app.layout.rail.outer = area;
}

/// The project's name, which of the projects it is, and how many of its
/// agents are waiting on a person — said only when some are.
fn render_heading(f: &mut Frame, app: &App, area: Rect, th: Theme) {
    let project = app.current_project();
    let name = project.map_or("no project", |p| p.name.as_str());
    let needs = project
        .into_iter()
        .flat_map(|p| p.repositories.iter())
        .flat_map(|r| r.checkouts.iter())
        .flat_map(|c| c.statuses())
        .filter(|s| s.needs_you())
        .count();
    // With more than one project, the heading says which of them this is
    // and that `o` reaches the rest.
    let position = if app.tree.len() > 1 {
        format!(" {}/{} ▾", app.sel_project + 1, app.tree.len())
    } else {
        String::new()
    };
    let need = match needs {
        0 => String::new(),
        n => format!(" {n} need"),
    };

    let width = area.width as usize;
    let left_width = width.saturating_sub(2 + need.chars().count() + RIGHT_PAD);
    let name = ellipsize_text(name, left_width.saturating_sub(position.chars().count()));
    let filler = left_width.saturating_sub(name.chars().count() + position.chars().count());
    // The heading is the rail's first stop: `n` adds a project and `D`
    // removes this one from there, so the cursor has to be seen on it.
    let fill = if app.focus == Focus::Projects {
        th.surface_focus
    } else {
        th.bg
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("▌ ", Style::default().fg(th.accent)),
            Span::styled(name, Style::default().fg(th.text).add_modifier(Modifier::BOLD)),
            Span::styled(position, Style::default().fg(th.dim)),
            Span::raw(" ".repeat(filler)),
            Span::styled(need, Style::default().fg(th.warn)),
        ]))
        .style(Style::default().bg(fill)),
        area,
    );
}

fn render_repositories(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    if area.height < 2 {
        app.layout.rail = Panel::default();
        return;
    }
    f.render_widget(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(th.edge)),
        area,
    );
    let rows_area = Rect {
        y: area.y + 1,
        height: area.height - 1,
        ..area
    };
    let rows = list_rows(app);
    // The least scroll that keeps the cursor on screen, so the keys never
    // walk off the end of what is drawn.
    let cursor = app.rail_cursor();
    let first = scrolled_to_show(
        app.layout.rail.first,
        cursor.and_then(|cursor| rows.iter().position(|row| *row == ListRow::Rail(cursor))),
        rows_area.height as usize,
        rows.len(),
    );
    let viewed = app.pane_location().filter(|_| {
        app.view == View::Workspace && matches!(app.focus, Focus::Panes | Focus::PaneContent)
    });
    let width = rows_area.width as usize;
    // A card's content: its sides, a cell of padding inside each, and a
    // cell of margin outside each.
    let inside = width.saturating_sub(6);
    let drawn = rows.iter().skip(first).take(rows_area.height as usize);
    for (screen, row) in drawn.enumerate() {
        let Some(project) = app.current_project() else {
            break;
        };
        let on_cursor = matches!(*row, ListRow::Rail(row) if cursor == Some(row));
        let line = match *row {
            ListRow::Rail(RailRow::Project) => continue,
            ListRow::QuietFold(fold) => {
                let label = format!("{} quiet", fold.count);
                let label_width = width.saturating_sub(5 + RIGHT_PAD);
                Line::styled(
                    format!(
                        " {} {:<label_width$} z",
                        if fold.open { "−" } else { "+" },
                        ellipsize_text(&label, label_width),
                    ),
                    Style::default().fg(th.dim),
                )
            }
            ListRow::Rail(RailRow::Repository(index)) => {
                let repo = &project.repositories[index];
                let selected = index == app.sel_repository;
                let loudest =
                    PaneStatus::loudest(repo.checkouts.iter().flat_map(|c| c.statuses()));
                let panes = repo
                    .checkouts
                    .iter()
                    .map(|c| c.listed_panes().count())
                    .sum::<usize>();
                let count = match panes {
                    0 => String::new(),
                    n => format!(" {n}"),
                };
                let name_width = width.saturating_sub(3 + count.chars().count() + RIGHT_PAD);
                let name_color = if selected {
                    th.text
                } else if loudest.is_some() {
                    th.muted
                } else {
                    th.dim
                };
                Line::from(vec![
                    Span::styled(
                        if selected { "▌" } else { " " },
                        Style::default().fg(th.accent),
                    ),
                    Span::styled(
                        format!("{} ", loudest.map_or("·", |status| status_glyph(app, status, "●"))),
                        Style::default().fg(loudest.map_or(th.dim, |s| status_color(s, th))),
                    ),
                    Span::styled(
                        format!("{:<name_width$}", ellipsize_text(&repo.name, name_width)),
                        Style::default().fg(name_color).add_modifier(bold_when(selected)),
                    ),
                    Span::styled(count, Style::default().fg(th.dim)),
                ])
            }
            ListRow::Rail(RailRow::Checkout(repository, checkout)) => {
                let item = &project.repositories[repository].checkouts[checkout];
                let git = item.git.as_ref();
                let branch = git.and_then(|g| g.branch.as_deref()).unwrap_or(&item.name);
                let state = git.map(git_state).unwrap_or_default();
                let dirty = git.is_some_and(|g| g.dirty);
                let branch_width = inside.saturating_sub(1 + state.chars().count());
                in_card(
                    vec![
                        Span::styled(
                            format!("{:<branch_width$}", ellipsize_text(branch, branch_width)),
                            Style::default().fg(th.syntax.function),
                        ),
                        Span::styled(
                            format!(" {state}"),
                            Style::default().fg(if dirty { th.warn } else { th.ok }),
                        ),
                    ],
                    card_tone(app, repository, checkout, viewed, th),
                    th,
                )
            }
            ListRow::Rail(RailRow::Pane(location)) => {
                let Some(pane) = app.pane_at(location) else {
                    continue;
                };
                let selected = viewed == Some(location);
                // The row speaks for what runs inside the pane too: an agent
                // idle at its prompt while a CLI it started works is working.
                let shown = pane.loudest_state().status;
                let color = status_color(shown, th);
                let still = if pane.kind == PaneKind::Agent { "◆" } else { "›" };
                // Messages a phone left for the agent's prompt. This row is
                // the only place the count shows, and `u` takes one back.
                let queued = match pane.queued.len() {
                    0 => String::new(),
                    n => format!(" ✉{n}"),
                };
                let word = status_word(shown);
                let title_width = inside
                    .saturating_sub(2 + queued.chars().count() + word.chars().count());
                in_card(
                    vec![
                        Span::styled(
                            format!("{} ", status_glyph(app, shown, still)),
                            Style::default().fg(color),
                        ),
                        Span::styled(
                            format!("{:<title_width$}", ellipsize_text(&pane.title, title_width)),
                            Style::default()
                                .fg(if selected { th.text } else { th.muted })
                                .add_modifier(bold_when(selected)),
                        ),
                        Span::styled(queued, Style::default().fg(th.warn)),
                        Span::styled(word, Style::default().fg(color)),
                    ],
                    card_tone(app, location.repository, location.checkout, viewed, th),
                    th,
                )
            }
            ListRow::Child(location, child) => {
                let Some(child) = app.pane_at(location).and_then(|info| info.children.get(child))
                else {
                    continue;
                };
                // A child has no glyph of its own, so its marker carries the
                // colour of a state the row no longer always spells out.
                let color = status_color(child.status, th);
                let word = status_word(child.status);
                let label_width = inside.saturating_sub(4 + word.chars().count());
                in_card(
                    vec![
                        Span::styled("  ⤷ ", Style::default().fg(color)),
                        Span::styled(
                            format!("{:<label_width$}", ellipsize_text(&child.label, label_width)),
                            Style::default().fg(th.muted),
                        ),
                        Span::styled(word, Style::default().fg(color)),
                    ],
                    card_tone(app, location.repository, location.checkout, viewed, th),
                    th,
                )
            }
            ListRow::CardTop(repository, checkout) | ListRow::CardBottom(repository, checkout) => {
                let (left, right) = if matches!(*row, ListRow::CardTop(..)) {
                    ("╭", "╮")
                } else {
                    ("╰", "╯")
                };
                Line::styled(
                    format!(" {left}{}{right}", "─".repeat(width.saturating_sub(4))),
                    Style::default().fg(card_tone(app, repository, checkout, viewed, th)),
                )
            }
        };
        // One highlight: the cursor's row and the pane on the stage, inside
        // a card between its sides. The selected repository is told by its
        // bar and bold name alone, since a band under everything it holds
        // read as a second cursor.
        let lit = on_cursor || matches!(*row, ListRow::Rail(RailRow::Pane(l)) if viewed == Some(l));
        let fill = if lit { th.surface_focus } else { th.bg };
        f.render_widget(
            Paragraph::new(line).style(Style::default().bg(fill)),
            Rect {
                y: rows_area.y + screen as u16,
                height: 1,
                ..rows_area
            },
        );
    }
    app.layout.rail = Panel {
        outer: area,
        inner: rows_area,
        first,
    };
}

/// A row inside a checkout's card: `content`, exactly the card's inside
/// width, between its sides. The sides keep the rail's ground, so a lit row
/// is a band across the card rather than over its border.
fn in_card(content: Vec<Span<'_>>, tone: Color, th: Theme) -> Line<'_> {
    let side = Style::default().fg(tone).bg(th.bg);
    let mut spans = vec![Span::styled(" │", side), Span::raw(" ")];
    spans.extend(content);
    spans.extend([Span::raw(" "), Span::styled("│ ", side)]);
    Line::from(spans)
}

/// A card's border says what its checkout is doing: the accent while the
/// cursor or the stage is in it, else the colour of the loudest state worth
/// watching, and the rule's colour while everything in it is idle or done.
fn card_tone(
    app: &App,
    repository: usize,
    checkout: usize,
    viewed: Option<PaneLocation>,
    th: Theme,
) -> Color {
    let holds = |location: PaneLocation| {
        (location.repository, location.checkout) == (repository, checkout)
    };
    let cursor_in = match app.rail_cursor() {
        Some(RailRow::Checkout(r, c)) => (r, c) == (repository, checkout),
        Some(RailRow::Pane(location)) => holds(location),
        _ => false,
    };
    if cursor_in || viewed.is_some_and(holds) {
        return th.accent;
    }

    let loudest = app.current_project().and_then(|project| {
        PaneStatus::loudest(project.repositories[repository].checkouts[checkout].statuses())
    });
    match loudest {
        Some(status) if status.urgency() >= PaneStatus::Working.urgency() => {
            status_color(status, th)
        }
        _ => th.edge,
    }
}

/// A status as a row writes it out: the word only for one that is waiting
/// on a person, led by the space that parts it from the title. The glyph
/// already says the rest, and a column of `IDLE` and `RUNNING` is what the
/// word that matters gets lost in.
fn status_word(status: PaneStatus) -> String {
    if !status.needs_you() {
        return String::new();
    }
    format!(" {}", short_status(status))
}

fn bold_when(selected: bool) -> Modifier {
    if selected {
        Modifier::BOLD
    } else {
        Modifier::empty()
    }
}
