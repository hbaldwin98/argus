//! The contextual rail: the project heading, the selected project's
//! repositories with the open ones' checkouts and panes, the divider
//! between the active repositories and the quiet ones, and the consoles at
//! its foot, with the hit-tests that map a click back onto a row.

use super::*;

/// Blank cells every row keeps before the rail's border, so the column of
/// counts and status words does not sit against the rule.
const RIGHT_PAD: usize = 2;

/// The rail row a click landed on. The project heading is drawn above the
/// list, so it is hit-tested on its own ([`project_header_at`]).
pub(crate) fn rail_target_at(app: &App, x: u16, y: u16) -> Option<RailRow> {
    let consoles = app.layout.consoles;
    if contains(consoles.inner, x, y) {
        let index = consoles.first + usize::from(y - consoles.inner.y);
        return (index < app.consoles.len()).then_some(RailRow::Console(index));
    }
    match list_row_at(app, x, y)? {
        ListRow::Rail(row) => Some(row),
        ListRow::Child(pane, _) => Some(RailRow::Pane(pane)),
        ListRow::Divider => None,
    }
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
/// else to go, so a click on it lands on the pane. The divider is a rule
/// between rows rather than one.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ListRow {
    Rail(RailRow),
    Child(PaneLocation, usize),
    Divider,
}

/// The rows drawn in the scrolling list: everything but the heading, with
/// each pane's children under it and the divider above the first quiet
/// repository.
fn list_rows(app: &App) -> Vec<ListRow> {
    let divider = app.rail_divider_above();
    let mut rows = Vec::new();
    for row in app.rail_rows() {
        match row {
            RailRow::Project => continue,
            RailRow::Repository(index) => {
                if divider == Some(index) {
                    rows.push(ListRow::Divider);
                }
                rows.push(ListRow::Rail(row));
            }
            RailRow::Pane(pane) => {
                rows.push(ListRow::Rail(row));
                let children = app.pane_at(pane).map_or(0, |info| info.children.len());
                rows.extend((0..children).map(|child| ListRow::Child(pane, child)));
            }
            RailRow::Checkout(..) => rows.push(ListRow::Rail(row)),
            // Drawn at the foot instead, where scrolling the repositories
            // never takes them off screen.
            RailRow::Console(_) => {}
        }
    }
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
    let foot = consoles_height(app, list.height);
    render_repositories(
        f,
        app,
        Rect {
            height: list.height - foot,
            ..list
        },
        th,
    );
    render_consoles(
        f,
        app,
        Rect {
            y: list.bottom() - foot,
            height: foot,
            ..list
        },
        th,
    );
    // The rule under the heading meets the border from the left only: the
    // stage's own heading rule is a row lower, so a tee facing right would
    // point at nothing. The consoles' rule meets it the same way.
    let consoles_rule = (app.layout.consoles.outer.height > 0).then_some(app.layout.consoles.outer.y);
    for y in [Some(list.y), consoles_rule].into_iter().flatten() {
        if y < area.bottom() && area.width > 0 {
            f.render_widget(
                Paragraph::new(Span::styled("┤", Style::default().fg(th.edge))),
                Rect::new(area.right() - 1, y, 1, 1),
            );
        }
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
    let drawn = rows.iter().skip(first).take(rows_area.height as usize);
    for (screen, row) in drawn.enumerate() {
        let Some(project) = app.current_project() else {
            break;
        };
        let on_cursor = matches!(*row, ListRow::Rail(row) if cursor == Some(row));
        let line = match *row {
            ListRow::Rail(RailRow::Project | RailRow::Console(_)) => continue,
            ListRow::Divider => Line::styled(
                format!(" {}", "─".repeat(width.saturating_sub(1 + RIGHT_PAD))),
                Style::default().fg(th.edge),
            ),
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
                let state = git.map(GitStatus::summary).unwrap_or_default();
                let dirty = git.is_some_and(|g| g.dirty);
                // Folded, the row speaks for the panes it hides: how many,
                // and the loudest of them, so one needing a person is not
                // put out of sight with the rest.
                let panes = item.listed_panes().count();
                let folded = panes > 0 && !app.is_checkout_open(item.id);
                let loudest = PaneStatus::loudest(item.statuses()).filter(|_| folded);
                let hidden = if folded {
                    format!(" {} {panes}", loudest.map_or("·", |status| status_glyph(app, status, "●")))
                } else {
                    String::new()
                };
                let branch_width = width.saturating_sub(
                    5 + state.chars().count() + hidden.chars().count() + RIGHT_PAD,
                );
                let lead = match (on_cursor, folded) {
                    (true, true) => "▌ ▸ ",
                    (false, true) => "  ▸ ",
                    (true, false) => "▌   ",
                    (false, false) => "    ",
                };
                Line::from(vec![
                    Span::styled(lead, Style::default().fg(th.accent)),
                    Span::styled(
                        ellipsize_text(branch, branch_width),
                        Style::default().fg(th.syntax.function),
                    ),
                    Span::styled(
                        format!(" {state}"),
                        Style::default().fg(if dirty { th.warn } else { th.ok }),
                    ),
                    Span::styled(
                        hidden,
                        Style::default().fg(loudest.map_or(th.dim, |s| status_color(s, th))),
                    ),
                ])
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
                let word = status_word(shown);
                let title_width = width.saturating_sub(
                    8 + word.chars().count() + RIGHT_PAD,
                );
                Line::from(vec![
                    Span::styled(
                        if selected { "▌     " } else { "      " },
                        Style::default().fg(th.accent),
                    ),
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
                    Span::styled(word, Style::default().fg(color)),
                ])
            }
            ListRow::Child(pane, child) => {
                let Some(child) = app.pane_at(pane).and_then(|info| info.children.get(child))
                else {
                    continue;
                };
                // A child has no glyph of its own, so its marker carries the
                // colour of a state the row no longer always spells out.
                let color = status_color(child.status, th);
                let word = status_word(child.status);
                let label_width =
                    width.saturating_sub(10 + word.chars().count() + RIGHT_PAD);
                Line::from(vec![
                    Span::styled("        ⤷ ", Style::default().fg(color)),
                    Span::styled(
                        format!("{:<label_width$}", ellipsize_text(&child.label, label_width)),
                        Style::default().fg(th.muted),
                    ),
                    Span::styled(word, Style::default().fg(color)),
                ])
            }
        };
        // One highlight: the cursor's row and the pane on the stage. The
        // selected repository is told by its bar and bold name alone, since
        // a band under everything it holds read as a second cursor.
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

/// How many of the rail's `height` rows under its heading the consoles
/// take: their rule and a row each — or one saying how to start one — up to
/// half, so the repositories keep the rail. None from a daemon that keeps
/// no consoles, since there is nothing it could start.
fn consoles_height(app: &App, height: u16) -> u16 {
    if !app.consoles_offered {
        return 0;
    }
    let wanted = 1 + app.consoles.len().max(1);
    let room = usize::from(height / 2);
    if room < 2 {
        return 0;
    }
    wanted.min(room) as u16
}

/// The consoles, under a rule naming them. They are rows of the rail like
/// any pane's — the cursor walks on into them past the last repository —
/// drawn apart because they belong to no project.
fn render_consoles(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    if area.height < 2 {
        app.layout.consoles = Panel::default();
        return;
    }
    let width = area.width as usize;
    let label = " consoles ";
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("─", Style::default().fg(th.edge)),
            Span::styled(label, Style::default().fg(th.dim)),
            Span::styled(
                "─".repeat(width.saturating_sub(1 + label.chars().count())),
                Style::default().fg(th.edge),
            ),
        ]))
        .style(Style::default().bg(th.bg)),
        Rect { height: 1, ..area },
    );
    let rows_area = Rect {
        y: area.y + 1,
        height: area.height - 1,
        ..area
    };
    if app.consoles.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  c", Style::default().fg(th.accent)),
                Span::styled(" starts one", Style::default().fg(th.dim)),
            ]))
            .style(Style::default().bg(th.bg)),
            Rect { height: 1, ..rows_area },
        );
        app.layout.consoles = Panel {
            outer: area,
            inner: rows_area,
            first: 0,
        };
        return;
    }
    // The cursor's row and the console on the stage are one row here: a
    // console is only on the stage while the cursor is on it or typing in it.
    let shown = app.console_index();
    let first = scrolled_to_show(
        app.layout.consoles.first,
        shown,
        rows_area.height as usize,
        app.consoles.len(),
    );
    let drawn = app.consoles.iter().enumerate().skip(first).take(rows_area.height as usize);
    for (screen, (index, console)) in drawn.enumerate() {
        let lit = shown == Some(index);
        let status = console.loudest_state().status;
        let color = status_color(status, th);
        let word = status_word(status);
        let title_width = width.saturating_sub(4 + word.chars().count() + RIGHT_PAD);
        let line = Line::from(vec![
            Span::styled(if lit { "▌ " } else { "  " }, Style::default().fg(th.accent)),
            Span::styled(
                format!("{} ", status_glyph(app, status, "›")),
                Style::default().fg(color),
            ),
            Span::styled(
                format!("{:<title_width$}", ellipsize_text(&console.title, title_width)),
                Style::default()
                    .fg(if lit { th.text } else { th.muted })
                    .add_modifier(bold_when(lit)),
            ),
            Span::styled(word, Style::default().fg(color)),
        ]);
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
    app.layout.consoles = Panel {
        outer: area,
        inner: rows_area,
        first,
    };
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
