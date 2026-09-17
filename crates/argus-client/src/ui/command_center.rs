//! The flat command-center shell specified by `Argus Command Center.dc.html`.
//!
//! This module owns presentation only. Every selection still points into the
//! daemon's tree; the shell merely gives that tree a stable rail and focused
//! operational surfaces.

use super::*;
use crate::app::FeaturePanel;
use argus_protocol::{PaneKind, PaneStatus, TaskState};

/// The mark, the WORKSPACE tab, and the FEATURE tab: the rail's border
/// continues the FEATURE tab's right edge.
pub const SIDEBAR_WIDTH: u16 = 11 + 13 + 11;
const HEADER_HEIGHT: u16 = 2;

fn contains(area: Rect, x: u16, y: u16) -> bool {
    x >= area.x
        && x < area.x.saturating_add(area.width)
        && y >= area.y
        && y < area.y.saturating_add(area.height)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RailTarget {
    Repository(usize),
    Checkout(usize, usize),
    Pane(PaneLocation),
}

fn ordered_repository_indices(app: &App) -> Vec<usize> {
    let Some(project) = app.current_project() else {
        return Vec::new();
    };
    let mut indices: Vec<_> = (0..project.repositories.len()).collect();
    indices.sort_by_key(|index| {
        let active = project.repositories[*index]
            .checkouts
            .iter()
            .any(|checkout| checkout.listed_panes().next().is_some());
        (!active, *index)
    });
    indices
}

fn rail_targets(app: &App) -> Vec<RailTarget> {
    let Some(project) = app.current_project() else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for repository in ordered_repository_indices(app) {
        rows.push(RailTarget::Repository(repository));
        let repo = &project.repositories[repository];
        // One repository is open at a time: the clicked one, or the one
        // holding the pane the keys are on when that is somewhere else.
        let in_pane =
            app.view == View::Spine && matches!(app.focus, Focus::Panes | Focus::PaneContent);
        let expanded = match app.current_repository() {
            Some(current) if in_pane && !app.expanded_repositories.contains(&current.id) => {
                repository == app.sel_repository
            }
            _ => app.expanded_repositories.contains(&repo.id),
        };
        if !expanded {
            continue;
        }
        for (checkout, item) in repo.checkouts.iter().enumerate() {
            let panes: Vec<_> = item.listed_panes().collect();
            if panes.is_empty() {
                continue;
            }
            rows.push(RailTarget::Checkout(repository, checkout));
            rows.extend(panes.into_iter().enumerate().map(|(pane, _)| {
                RailTarget::Pane(PaneLocation {
                    project: app.sel_project,
                    repository,
                    checkout,
                    pane,
                })
            }));
        }
    }
    rows
}

pub(crate) fn rail_target_at(app: &App, x: u16, y: u16) -> Option<RailTarget> {
    let panel = app.layout.projects;
    contains(panel.inner, x, y)
        .then(|| {
            rail_targets(app)
                .get(panel.first + usize::from(y - panel.inner.y))
                .copied()
        })
        .flatten()
}

pub(crate) fn sidebar_contains(app: &App, x: u16, y: u16) -> bool {
    contains(app.layout.projects.outer, x, y)
}

fn pane_card_rect(body: Rect, index: usize) -> Rect {
    let columns = if body.width >= 90 {
        3
    } else if body.width >= 56 {
        2
    } else {
        1
    };
    let card_width = body.width / columns;
    let card_height = 7;
    let col = index as u16 % columns;
    let row = index as u16 / columns;
    Rect {
        x: body.x + col * card_width,
        y: body.y + row * card_height,
        width: card_width,
        height: card_height.min(body.bottom().saturating_sub(body.y + row * card_height)),
    }
}

pub(crate) fn checkout_at(app: &App, x: u16, y: u16) -> Option<usize> {
    let rows = app.layout.checkouts.inner;
    if !contains(rows, x, y) {
        return None;
    }
    let index = usize::from(y.saturating_sub(rows.y) / 2);
    (index < app.current_repository()?.checkouts.len()).then_some(index)
}

pub(crate) fn pane_at(app: &App, x: u16, y: u16) -> Option<PaneLocation> {
    let body = app.layout.panes.inner;
    app.overview_pane_locations()
        .into_iter()
        .enumerate()
        .find_map(|(index, location)| {
            contains(pane_card_rect(body, index), x, y).then_some(location)
        })
}

pub(super) fn render_command_center(
    f: &mut Frame,
    app: &mut App,
    area: Rect,
    th: Theme,
) -> Option<CursorPlacement> {
    let frame = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(HEADER_HEIGHT.min(area.height)),
            Constraint::Min(1),
        ])
        .split(area);
    render_view_tabs(f, app, frame[0], th);

    if app.tree.is_empty() {
        forget_spine(app);
        render_first_run(f, frame[1], th);
        return None;
    }

    let rail_width = rail_width(frame[1].width);
    let sidebar = Rect {
        width: rail_width,
        ..frame[1]
    };
    let stage = Rect {
        x: frame[1].x.saturating_add(rail_width),
        width: frame[1].width.saturating_sub(rail_width),
        ..frame[1]
    };
    render_sidebar(f, app, sidebar, th);
    // The junction only joins a plain rule; over the open tab's accent bar
    // it would cut the bar in two.
    let junction = (
        sidebar.right().saturating_sub(1),
        frame[0].bottom().saturating_sub(1),
    );
    let plain_rule = f
        .buffer_mut()
        .cell(junction)
        .is_some_and(|cell| cell.symbol() == "─");
    if frame[0].height > 1 && rail_width > 0 && plain_rule {
        f.render_widget(
            Paragraph::new(Span::styled("┬", Style::default().fg(th.edge))),
            Rect {
                x: sidebar.right().saturating_sub(1),
                y: frame[0].bottom().saturating_sub(1),
                width: 1,
                height: 1,
            },
        );
    }

    app.layout.repositories = Panel::default();
    app.layout.checkouts = Panel::default();
    app.layout.panes = Panel::default();
    app.layout.content = Panel::default();

    match app.view {
        View::Spine => render_workspace(f, app, stage, th),
        View::Feature => {
            render_feature_document(f, app, stage, th);
            None
        }
        View::Panes => {
            forget_feature_view(app);
            render_panes(f, app, stage, th);
            None
        }
        View::Checkouts => {
            forget_feature_view(app);
            render_checkouts(f, app, stage, th);
            None
        }
    }
}

/// The rail's width for a frame this wide. The tab strip reads it too, so
/// the rail's edge and the end of the first tab are one line.
pub(super) fn rail_width(total: u16) -> u16 {
    SIDEBAR_WIDTH.min(total.saturating_sub(24).max(1))
}

/// Every agent in the workspace, in rail order, with where it lives.
fn agent_rows(app: &App) -> Vec<PaneLocation> {
    let Some(project) = app.current_project() else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for (repository, repo) in project.repositories.iter().enumerate() {
        for (checkout, item) in repo.checkouts.iter().enumerate() {
            for (pane, info) in item.listed_panes().enumerate() {
                if info.kind == PaneKind::Agent {
                    rows.push(PaneLocation {
                        project: app.sel_project,
                        repository,
                        checkout,
                        pane,
                    });
                }
            }
        }
    }
    rows
}

/// The agent a click in the rail's AGENTS list landed on.
pub(crate) fn agent_at(app: &App, x: u16, y: u16) -> Option<PaneLocation> {
    let panel = app.layout.agents;
    if !contains(panel.inner, x, y) {
        return None;
    }
    agent_rows(app)
        .get(panel.first + usize::from(y - panel.inner.y))
        .copied()
}

fn render_sidebar(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
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
    let listed: Vec<_> = app
        .current_project()
        .map(|p| {
            p.repositories
                .iter()
                .flat_map(|r| r.checkouts.iter())
                .flat_map(|c| c.listed_panes())
                .map(|p| (p.kind, p.status))
                .collect()
        })
        .unwrap_or_default();
    let agents = listed.iter().filter(|(k, _)| *k == PaneKind::Agent).count();
    let needs = listed.iter().filter(|(_, s)| s.needs_you()).count();
    let project = app.current_project();
    let repo_count = project.map(|p| p.repositories.len()).unwrap_or(0);
    let name = project
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "no project".into());
    let workspace = if app.open_workspace.is_empty() {
        "default".to_string()
    } else {
        app.open_workspace.clone()
    };

    // Header: label, the project under an accent bar, and where it is.
    let x = inner.x + 2;
    let width = inner.width.saturating_sub(3);
    let put = |f: &mut Frame, y: u16, line: Line| {
        if y < inner.bottom() {
            f.render_widget(Paragraph::new(line), Rect::new(x, y, width, 1));
        }
    };
    put(
        f,
        inner.y + 1,
        Line::styled("ACTIVE WORKSPACE", Style::default().fg(th.dim)),
    );
    put(
        f,
        inner.y + 2,
        Line::from(vec![
            Span::styled("▌ ", Style::default().fg(th.accent)),
            Span::styled(
                ellipsize_text(&name, width.saturating_sub(2) as usize),
                Style::default().fg(th.text).add_modifier(Modifier::BOLD),
            ),
        ]),
    );
    put(
        f,
        inner.y + 3,
        Line::styled(
            format!("  {workspace} workspace"),
            Style::default().fg(th.dim),
        ),
    );

    // Badges: filled chips on one row, wrapping only when the rail is too
    // narrow to hold them side by side.
    let mut badges = vec![
        (format!(" {repo_count} repos "), th.muted),
        (format!(" {agents} agents "), th.ok),
    ];
    if needs > 0 {
        badges.push((format!(" {needs} needs you "), th.warn));
    }
    let mut bx = x;
    let mut by = inner.y + 5;
    for (text, color) in badges {
        let w = text.chars().count() as u16;
        if bx > x && bx + w > x + width {
            bx = x;
            by += 1;
        }
        if by < inner.bottom() {
            f.render_widget(
                Paragraph::new(Span::styled(text, badge(color, th))),
                Rect::new(bx, by, w.min((x + width).saturating_sub(bx)), 1),
            );
        }
        bx += w + 1;
    }
    let summary_height = (by + 2).saturating_sub(inner.y).min(inner.height);

    // Agents take what they list, up to a third of the rail.
    let agent_count = agent_rows(app).len() as u16;
    let rest = inner.height.saturating_sub(summary_height);
    let agent_height = (agent_count.max(1) + 4).min(rest / 3).min(rest);
    let repos_area = Rect {
        y: inner.y + summary_height,
        height: rest.saturating_sub(agent_height),
        ..inner
    };
    render_repositories(f, app, repos_area, th);
    let agents_area = Rect {
        y: repos_area.bottom(),
        height: agent_height,
        ..inner
    };
    render_agents(f, app, agents_area, th);
    for y in [repos_area.y, agents_area.y] {
        if y < area.bottom() && area.width > 0 {
            f.render_widget(
                Paragraph::new(Span::styled("├", Style::default().fg(th.edge))),
                Rect::new(area.right() - 1, y, 1, 1),
            );
        }
    }
    // Repository rows are the rail's interactive interior, while the whole
    // rail remains its hit target and geometry landmark.
    app.layout.projects.outer = area;
}

fn badge(fg: Color, th: Theme) -> Style {
    Style::default().fg(fg).bg(th.surface)
}

fn render_repositories(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    if area.height < 3 {
        app.layout.projects = Panel::default();
        return;
    }
    f.render_widget(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(th.edge)),
        area,
    );
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(
                    "  {:<width$}",
                    "REPOSITORIES",
                    width = (area.width as usize).saturating_sub(6)
                ),
                Style::default().fg(th.dim),
            ),
            Span::styled("j/k", Style::default().fg(th.edge)),
        ])),
        Rect::new(area.x, area.y + 1, area.width, 1),
    );
    let rows_area = Rect {
        y: area.y + 2,
        height: area.height.saturating_sub(2),
        ..area
    };
    let targets = rail_targets(app);
    let first = app
        .layout
        .projects
        .first
        .min(targets.len().saturating_sub(1));
    let width = rows_area.width as usize;
    for (screen, target) in targets.iter().skip(first).enumerate() {
        if screen as u16 >= rows_area.height {
            break;
        }
        let y = rows_area.y + screen as u16;
        let Some(project) = app.current_project() else {
            break;
        };
        // The selected repository and everything open under it read as one
        // raised block; the selected pane is lifted one step further.
        let in_selected = match *target {
            RailTarget::Repository(index) | RailTarget::Checkout(index, _) => {
                index == app.sel_repository
            }
            RailTarget::Pane(location) => location.repository == app.sel_repository,
        };
        let row_bg = if in_selected { th.surface } else { th.bg };
        let line = match *target {
            RailTarget::Repository(index) => {
                let repo = &project.repositories[index];
                let selected = index == app.sel_repository;
                let panes = repo
                    .checkouts
                    .iter()
                    .map(|c| c.listed_panes().count())
                    .sum::<usize>();
                let loudest = loudest_status(
                    repo.checkouts
                        .iter()
                        .flat_map(|c| c.listed_panes())
                        .map(|p| &p.status),
                );
                let bg = if selected { th.surface } else { th.bg };
                let checkouts = plural(repo.checkouts.len(), "checkout");
                let name_len = repo.name.chars().count();
                // The template's full meta when the name still fits beside
                // it, and the more telling half when it does not.
                let meta = [
                    if panes > 0 {
                        format!("{checkouts} · {}", plural(panes, "pane"))
                    } else {
                        checkouts.clone()
                    },
                    if panes > 0 {
                        plural(panes, "pane")
                    } else {
                        checkouts
                    },
                ]
                .into_iter()
                .find(|meta| name_len + meta.chars().count() + 8 <= width)
                .unwrap_or_default();
                let name_width = width.saturating_sub(meta.chars().count() + 6);
                Line::from(vec![
                    Span::styled(
                        if selected { "▌ " } else { "  " },
                        Style::default().fg(th.accent).bg(bg),
                    ),
                    Span::styled(
                        match loudest {
                            Some(status) => format!("{}  ", status_glyph(app, status, "●")),
                            None => "·  ".to_string(),
                        },
                        Style::default()
                            .fg(loudest.map(|s| status_color(s, th)).unwrap_or(th.dim))
                            .bg(bg),
                    ),
                    Span::styled(
                        format!("{:<name_width$}", ellipsize_text(&repo.name, name_width)),
                        Style::default()
                            .fg(if selected {
                                th.text
                            } else if loudest.is_some() {
                                th.muted
                            } else {
                                th.dim
                            })
                            .bg(bg)
                            .add_modifier(if selected {
                                Modifier::BOLD
                            } else {
                                Modifier::empty()
                            }),
                    ),
                    Span::styled(format!("{meta:>}  "), Style::default().fg(th.dim).bg(bg)),
                ])
            }
            RailTarget::Checkout(repository, checkout) => {
                let item = &project.repositories[repository].checkouts[checkout];
                let git = item.git.as_ref();
                let branch = git.and_then(|g| g.branch.as_deref()).unwrap_or(&item.name);
                let state = git
                    .map(|g| {
                        if g.dirty {
                            format!("!{}W", g.changed_files)
                        } else if g.ahead > 0 {
                            format!("↑{} clean", g.ahead)
                        } else {
                            "clean".into()
                        }
                    })
                    .unwrap_or_default();
                Line::from(vec![
                    Span::styled("     └ ", Style::default().fg(th.edge)),
                    Span::styled(
                        ellipsize_text(branch, width.saturating_sub(18)),
                        Style::default().fg(th.syntax.function),
                    ),
                    Span::styled(
                        format!(" {state}"),
                        Style::default().fg(if git.is_some_and(|g| g.dirty) {
                            th.warn
                        } else {
                            th.ok
                        }),
                    ),
                ])
            }
            RailTarget::Pane(location) => {
                let Some(pane) = app.pane_at(location) else {
                    continue;
                };
                let selected = app.pane_location() == Some(location)
                    && app.view == View::Spine
                    && matches!(app.focus, Focus::Panes | Focus::PaneContent);
                let bg = if selected { th.surface_focus } else { row_bg };
                let color = status_color(pane.status, th);
                let status = short_status(pane.status);
                let title_width = width.saturating_sub(13 + status.len());
                Line::from(vec![
                    Span::styled(
                        if selected { "▌      " } else { "       " },
                        Style::default().fg(th.accent).bg(bg),
                    ),
                    Span::styled("└ ", Style::default().fg(th.edge).bg(bg)),
                    Span::styled(
                        format!(
                            "{} ",
                            status_glyph(
                                app,
                                pane.status,
                                if pane.kind == PaneKind::Agent {
                                    "◆"
                                } else {
                                    "›"
                                }
                            )
                        ),
                        Style::default().fg(color).bg(bg),
                    ),
                    Span::styled(
                        format!("{:<title_width$}", ellipsize_text(&pane.title, title_width)),
                        Style::default()
                            .fg(if selected { th.text } else { th.muted })
                            .bg(bg)
                            .add_modifier(if selected {
                                Modifier::BOLD
                            } else {
                                Modifier::empty()
                            }),
                    ),
                    Span::styled(format!(" {status}  "), Style::default().fg(color).bg(bg)),
                ])
            }
        };
        let fill = if matches!(*target, RailTarget::Pane(l) if app.pane_location() == Some(l)
            && app.view == View::Spine
            && matches!(app.focus, Focus::Panes | Focus::PaneContent))
        {
            th.surface_focus
        } else {
            row_bg
        };
        f.render_widget(
            Paragraph::new(line).style(Style::default().bg(fill)),
            Rect {
                y,
                height: 1,
                ..rows_area
            },
        );
    }
    app.layout.projects = Panel {
        outer: area,
        inner: rows_area,
        first,
    };
    app.layout.row_height = 1;
}

fn render_agents(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    app.layout.agents = Panel::default();
    if area.height < 3 {
        return;
    }
    f.render_widget(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(th.edge)),
        area,
    );
    let inner = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(3),
        height: area.height.saturating_sub(2),
    };
    f.render_widget(
        Paragraph::new(Line::styled("AGENTS", Style::default().fg(th.dim))),
        Rect { height: 1, ..inner },
    );
    let rows_area = Rect {
        y: inner.y + 2,
        height: inner.height.saturating_sub(2),
        ..inner
    };
    let rows = agent_rows(app);
    let current = app
        .pane_location()
        .filter(|_| matches!(app.focus, Focus::Panes | Focus::PaneContent));
    let first = scrolled_to_show(
        0,
        current.and_then(|c| rows.iter().position(|r| *r == c)),
        rows_area.height as usize,
        rows.len(),
    );
    for (index, location) in rows
        .iter()
        .enumerate()
        .skip(first)
        .take(rows_area.height as usize)
    {
        let (Some(pane), Some((_, repo, checkout))) =
            (app.pane_at(*location), app.pane_path(*location))
        else {
            continue;
        };
        let selected = current == Some(*location);
        let color = status_color(pane.status, th);
        let status = short_status(pane.status);
        let name_width = (rows_area.width as usize).saturating_sub(status.len() + 3);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!("{} ", status_glyph(app, pane.status, "■")),
                    Style::default().fg(color),
                ),
                Span::styled(
                    format!(
                        "{:<name_width$}",
                        ellipsize_text(&format!("{repo} · {checkout} #{}", pane.id.0), name_width)
                    ),
                    Style::default()
                        .fg(if selected { th.text } else { th.muted })
                        .add_modifier(if selected {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
                Span::styled(format!(" {status}"), Style::default().fg(color)),
            ])),
            Rect {
                y: rows_area.y + (index - first) as u16,
                height: 1,
                ..rows_area
            },
        );
    }
    app.layout.agents = Panel {
        outer: area,
        inner: rows_area,
        first,
    };
}

fn render_workspace(
    f: &mut Frame,
    app: &mut App,
    area: Rect,
    th: Theme,
) -> Option<CursorPlacement> {
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3.min(area.height)), Constraint::Min(1)])
        .split(area);
    let (repo, checkout) = (
        app.current_repository()
            .map(|r| r.name.as_str())
            .unwrap_or("repository"),
        app.current_checkout()
            .map(|c| c.name.as_str())
            .unwrap_or("checkout"),
    );
    let pane = app.current_pane();
    let mut header = Line::from(vec![
        Span::styled(
            format!("  {repo}"),
            Style::default().fg(th.text).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" / ", Style::default().fg(th.edge)),
        Span::styled(
            checkout.to_string(),
            Style::default().fg(th.syntax.function),
        ),
    ]);
    if let Some(pane) = pane {
        header.push_span(Span::styled(" / ", Style::default().fg(th.edge)));
        header.push_span(Span::styled(
            format!("{} #{}", pane.title, pane.id.0),
            Style::default().fg(th.muted),
        ));
        // Plain space before the chip, and one cell of padding inside it.
        header.push_span(Span::raw("  "));
        header.push_span(Span::styled(
            format!(" {} ", short_status(pane.status)),
            badge(status_color(pane.status, th), th),
        ));
    }
    f.render_widget(
        Paragraph::new(header).block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(th.edge))
                .padding(Padding::new(0, 0, 1, 0)),
        ),
        split[0],
    );
    if split[0].x > 0 && split[0].height > 0 {
        f.render_widget(
            Paragraph::new(Span::styled("├", Style::default().fg(th.edge))),
            Rect {
                x: split[0].x - 1,
                y: split[0].bottom() - 1,
                width: 1,
                height: 1,
            },
        );
    }
    // The terminal fills the stage edge to edge, right up to the header rule.
    let terminal = split[1];
    let cursor = if let Some(id) = app.column_pane() {
        render_term(
            f,
            app.grids.get(&id),
            terminal,
            app.focus == Focus::PaneContent,
            Some(id),
            app.selection.as_ref(),
        )
    } else {
        f.render_widget(
            Paragraph::new("No pane selected\n\na  agent    s  shell")
                .style(Style::default().fg(th.dim)),
            terminal,
        );
        None
    };
    app.layout.content = Panel {
        outer: area,
        inner: terminal,
        first: 0,
    };
    cursor
}

/// A section label with a dim hint beside it.
fn section_heading(f: &mut Frame, area: Rect, label: &str, hint: &str, th: Theme) {
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(label.to_string(), Style::default().fg(th.dim)),
            Span::styled(format!("   {hint}"), Style::default().fg(th.edge)),
        ])),
        Rect { height: 1, ..area },
    );
}

/// The one-cell scroll thumb at a section's right edge, drawn only when the
/// section holds more rows than it shows.
fn section_thumb(f: &mut Frame, area: Rect, first: usize, visible: usize, len: usize, th: Theme) {
    if len <= visible || area.height == 0 || area.width == 0 {
        return;
    }
    let span = len - visible;
    let offset = (first.min(span) * usize::from(area.height.saturating_sub(1)) / span) as u16;
    f.render_widget(
        Paragraph::new(Span::styled(SCROLL_THUMB, Style::default().fg(th.dim))),
        Rect {
            x: area.right() - 1,
            y: area.y + offset,
            width: 1,
            height: 1,
        },
    );
}

fn render_feature_document(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    forget_feature_view(app);
    let features = app.feature_rows();
    let decided = app.board_rows().len();
    let (title, counts, brief_text) = app
        .selected_feature()
        .map(|feature| {
            (
                feature.title.clone(),
                {
                    // The loaded list when it has arrived, so the heading
                    // cannot disagree with the rows beneath it.
                    let rows = app.feature_task_rows();
                    let (done, total) = if rows.is_empty() {
                        (feature.tasks.done, feature.tasks.total())
                    } else {
                        (
                            rows.iter()
                                .filter(|r| r.task.state == TaskState::Done)
                                .count(),
                            rows.len(),
                        )
                    };
                    format!("{done}/{total} TASKS · {decided} DECIDED")
                },
                feature.body.clone(),
            )
        })
        .unwrap_or_else(|| {
            (
                "No feature selected".to_string(),
                String::new(),
                String::new(),
            )
        });
    render_stage_heading(f, area, "features /", &title, &counts, th);

    let body = Rect {
        x: area.x.saturating_add(3),
        y: area.y.saturating_add(4.min(area.height)),
        width: area.width.saturating_sub(6),
        height: area.height.saturating_sub(5),
    };
    if body.height < 2 || body.width < 8 {
        return;
    }
    let bottom = body.bottom();

    // Features: a short list that scrolls, so every feature is reachable
    // without the stage giving up the brief and tasks beneath it.
    let focused_features = app.panel == FeaturePanel::Features;
    section_heading(f, body, "FEATURES", "h list · tab panels · enter brief", th);
    let feature_height = (features.len().max(1) as u16)
        .min(5)
        .min(bottom.saturating_sub(body.y + 1));
    let features_area = Rect {
        y: body.y + 1,
        height: feature_height,
        ..body
    };
    let first = scrolled_to_show(
        app.layout.features.first,
        Some(app.feature_sel),
        feature_height as usize,
        features.len(),
    );
    if features.is_empty() {
        f.render_widget(
            Paragraph::new("no features yet — a starts one").style(Style::default().fg(th.dim)),
            features_area,
        );
    }
    for (index, row) in features
        .iter()
        .enumerate()
        .skip(first)
        .take(feature_height as usize)
    {
        let selected = index == app.feature_sel;
        let bg = if selected && focused_features {
            th.surface
        } else {
            th.bg
        };
        let style = Style::default().bg(bg);
        let detail_width = (features_area.width as usize / 3).min(row.detail.chars().count());
        let title_width = (features_area.width as usize).saturating_sub(detail_width + 6);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(if selected { "▌ " } else { "  " }, style.fg(th.accent)),
                Span::styled(
                    format!("{:<title_width$}", ellipsize_text(&row.title, title_width)),
                    style
                        .fg(if row.attention.is_some() {
                            th.warn
                        } else if selected {
                            th.text
                        } else {
                            th.muted
                        })
                        .add_modifier(if selected {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
                Span::styled(
                    format!("  {}", ellipsize_text(&row.detail, detail_width)),
                    style.fg(th.dim),
                ),
            ]))
            .style(style),
            Rect {
                y: features_area.y + (index - first) as u16,
                height: 1,
                ..features_area
            },
        );
    }
    section_thumb(
        f,
        features_area,
        first,
        feature_height as usize,
        features.len(),
        th,
    );
    app.layout.features = Panel {
        outer: features_area,
        inner: features_area,
        first,
    };

    // Brief: wrapped in full and scrolled, never clipped to its first lines.
    let brief_y = features_area.bottom() + 1;
    if brief_y + 3 >= bottom {
        return;
    }
    section_heading(
        f,
        Rect { y: brief_y, ..body },
        "BRIEF",
        "wheel scrolls · e edits",
        th,
    );
    let text_width = body.width.saturating_sub(4);
    let lines: Vec<String> = if brief_text.is_empty() {
        vec!["No brief yet — e edits the feature brief.".to_string()]
    } else {
        brief_text
            .lines()
            .flat_map(|line| wrap(line, text_width))
            .collect()
    };
    let remaining = bottom.saturating_sub(brief_y + 1);
    let brief_height = (lines.len() as u16 + 2)
        .min(8)
        .min(remaining.saturating_sub(4).max(3));
    let brief_area = Rect {
        y: brief_y + 1,
        height: brief_height,
        ..body
    };
    let visible = brief_height.saturating_sub(2) as usize;
    let max_scroll = lines.len().saturating_sub(visible) as u16;
    app.feature_brief_scroll = app.feature_brief_scroll.min(max_scroll);
    let scroll = app.feature_brief_scroll;
    f.render_widget(
        Paragraph::new(
            lines
                .iter()
                .skip(scroll as usize)
                .take(visible)
                .map(|line| Line::raw(line.clone()))
                .collect::<Vec<_>>(),
        )
        .style(Style::default().fg(th.muted))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(th.edge))
                .padding(Padding::horizontal(1)),
        ),
        brief_area,
    );
    section_thumb(
        f,
        Rect {
            y: brief_area.y + 1,
            height: brief_area.height.saturating_sub(2),
            ..brief_area
        },
        scroll as usize,
        visible,
        lines.len(),
        th,
    );
    app.layout.feature_brief = Panel {
        outer: brief_area,
        inner: inset(brief_area, 1, 1),
        first: scroll as usize,
    };

    // Tasks and decisions share what is left; each scrolls to its cursor.
    let tasks_y = brief_area.bottom() + 1;
    if tasks_y + 1 >= bottom {
        return;
    }
    let rows = app.feature_task_rows();
    let rest = bottom.saturating_sub(tasks_y);
    let wants_tasks = rows.len().max(1) as u16 + 1;
    // The gap row, the heading, and at least one two-line decision.
    let wants_decisions = decided.max(1) as u16 * 2 + 2;
    let task_block = if wants_tasks + wants_decisions <= rest {
        wants_tasks
    } else {
        wants_tasks
            .min(rest.saturating_sub((wants_decisions + 1).min(rest / 2)))
            .max(rest.min(3))
    };
    section_heading(
        f,
        Rect { y: tasks_y, ..body },
        "TASKS",
        "a add · s subtask · x drop · ⏎ opens",
        th,
    );
    let tasks_area = Rect {
        y: tasks_y + 1,
        height: task_block.saturating_sub(1),
        ..body
    };
    let focused_tasks = app.panel == FeaturePanel::Tasks;
    let task_first = scrolled_to_show(
        0,
        Some(app.task_sel),
        tasks_area.height as usize,
        rows.len(),
    );
    if rows.is_empty() && tasks_area.height > 0 {
        f.render_widget(
            Paragraph::new("nothing to do here yet — a adds a task")
                .style(Style::default().fg(th.dim)),
            Rect {
                height: 1,
                ..tasks_area
            },
        );
    }
    for (index, row) in rows
        .iter()
        .enumerate()
        .skip(task_first)
        .take(tasks_area.height as usize)
    {
        let selected = index == app.task_sel;
        let (mark, color) = match row.task.state {
            TaskState::Todo => ("○", th.dim),
            TaskState::Doing => ("●", th.accent),
            TaskState::Done => ("✓", th.ok),
        };
        let bg = if selected && focused_tasks {
            th.surface
        } else {
            th.bg
        };
        let style = Style::default().bg(bg);
        let mut guide = String::new();
        if row.depth > 0 {
            for continues in row
                .ancestor_continuations
                .iter()
                .take(row.depth.saturating_sub(1))
            {
                guide.push_str(if *continues { "│ " } else { "  " });
            }
            guide.push_str(if row.has_next_sibling { "├ " } else { "└ " });
        }
        let children = rows[index + 1..]
            .iter()
            .take_while(|child| child.depth > row.depth)
            .filter(|child| child.depth == row.depth + 1)
            .count();
        let badge = if children > 0 {
            format!(" {} ", plural(children, "subtask"))
        } else {
            String::new()
        };
        let id = format!("#{}", row.task.id);
        let fixed = 2 + guide.chars().count() + 2 + badge.chars().count() + 2 + 6;
        let title_width = (tasks_area.width as usize).saturating_sub(fixed);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(if selected { "▌ " } else { "  " }, style.fg(th.accent)),
                Span::styled(guide, style.fg(th.edge)),
                Span::styled(format!("{mark} "), style.fg(color)),
                Span::styled(
                    format!(
                        "{:<title_width$}",
                        ellipsize_text(&row.task.title, title_width)
                    ),
                    style.fg(if row.task.state == TaskState::Done {
                        th.dim
                    } else {
                        th.text
                    }),
                ),
                Span::styled(badge, Style::default().fg(th.muted).bg(th.surface)),
                Span::styled(format!("  {id:>6}"), style.fg(th.dim)),
            ]))
            .style(style),
            Rect {
                y: tasks_area.y + (index - task_first) as u16,
                height: 1,
                ..tasks_area
            },
        );
    }
    section_thumb(
        f,
        tasks_area,
        task_first,
        tasks_area.height as usize,
        rows.len(),
        th,
    );
    drop(rows);
    app.layout.feature_tasks = Panel {
        outer: tasks_area,
        inner: tasks_area,
        first: task_first,
    };

    let decisions_y = tasks_area.bottom() + 1;
    if decisions_y + 1 >= bottom {
        return;
    }
    section_heading(
        f,
        Rect {
            y: decisions_y,
            ..body
        },
        "DECISIONS",
        "",
        th,
    );
    let decisions_area = Rect {
        y: decisions_y + 1,
        height: bottom.saturating_sub(decisions_y + 1),
        ..body
    };
    let focused_decisions = app.panel == FeaturePanel::Decisions;
    let decisions = app.board_rows();
    if decisions.is_empty() && decisions_area.height > 0 {
        f.render_widget(
            Paragraph::new("nothing decided for this feature yet")
                .style(Style::default().fg(th.dim)),
            Rect {
                height: 1,
                ..decisions_area
            },
        );
    }
    let decision_first = scrolled_to_show(
        0,
        Some(app.decision_sel),
        (decisions_area.height / 2) as usize,
        decisions.len(),
    );
    for (index, row) in decisions
        .iter()
        .enumerate()
        .skip(decision_first)
        .take((decisions_area.height / 2) as usize)
    {
        let selected = index == app.decision_sel;
        let decision = row.decision;
        let reason = decision
            .because
            .as_deref()
            .or(decision.over.as_deref())
            .unwrap_or("No reasoning recorded");
        let bg = if selected && focused_decisions {
            th.surface
        } else {
            th.bg
        };
        let style = Style::default().bg(bg);
        let bar = Span::styled(
            if selected { "▌ " } else { "│ " },
            style.fg(if selected { th.accent } else { th.edge }),
        );
        let text_width = (decisions_area.width as usize).saturating_sub(10);
        let y = decisions_area.y + (index - decision_first) as u16 * 2;
        f.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    bar.clone(),
                    Span::styled(
                        format!("{:<6}", format!("#{}", decision.id)),
                        style.fg(th.dim),
                    ),
                    Span::styled(
                        ellipsize_text(&decision.chose, text_width),
                        style.fg(if decision.superseded() {
                            th.dim
                        } else if selected {
                            th.text
                        } else {
                            th.muted
                        }),
                    ),
                ]),
                Line::from(vec![
                    bar,
                    Span::styled("      ", style),
                    Span::styled(ellipsize_text(reason, text_width), style.fg(th.dim)),
                ]),
            ])
            .style(style),
            Rect {
                y,
                height: 2,
                ..decisions_area
            },
        );
    }
    section_thumb(
        f,
        decisions_area,
        decision_first,
        (decisions_area.height / 2) as usize,
        decisions.len(),
        th,
    );
    drop(decisions);
    app.layout.feature_decisions = Panel {
        outer: decisions_area,
        inner: decisions_area,
        first: decision_first,
    };
}

fn render_panes(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    let repo = app
        .current_repository()
        .map(|repo| repo.name.as_str())
        .unwrap_or("repository");
    let detail = if app.show_all_panes {
        "all workspace panes".to_string()
    } else {
        format!("{repo} · repository panes")
    };
    render_stage_heading(f, area, "panes", &detail, "A ALL · a AGENT · s SHELL", th);
    let body = Rect {
        x: area.x + 2,
        y: area.y + 4.min(area.height),
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(5),
    };
    let locations = app.overview_pane_locations();
    app.layout.panes = Panel {
        outer: area,
        inner: body,
        first: 0,
    };
    for (index, location) in locations.iter().enumerate() {
        let rect = pane_card_rect(body, index);
        if rect.height == 0 {
            break;
        }
        let Some(pane) = app.pane_at(*location) else {
            continue;
        };
        let selected = app.pane_location() == Some(*location);
        let color = status_color(pane.status, th);
        let path = app
            .pane_path(*location)
            .map(|(_, r, c)| format!("{r} · {c}"))
            .unwrap_or_default();
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if selected { th.accent } else { th.edge }))
            // Cards sit on the page itself; selection is the accent border.
            .style(Style::default().bg(th.bg));
        let inner = block.inner(rect);
        f.render_widget(block, rect);
        f.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    Span::styled("■ ", Style::default().fg(color)),
                    Span::styled(short_status(pane.status), Style::default().fg(color)),
                    Span::styled(format!("  #{}", pane.id.0), Style::default().fg(th.dim)),
                ]),
                Line::styled(
                    ellipsize_text(&pane.title, inner.width as usize),
                    Style::default().fg(th.text).add_modifier(Modifier::BOLD),
                ),
                Line::styled(
                    pane.note.clone().unwrap_or_else(|| "running pane".into()),
                    Style::default().fg(th.dim),
                ),
                Line::styled(path, Style::default().fg(th.muted)),
            ]),
            inner,
        );
    }
}

fn render_checkouts(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    let repo_name = app
        .current_repository()
        .map(|r| r.name.as_str())
        .unwrap_or("checkouts");
    render_stage_heading(
        f,
        area,
        repo_name,
        "checkouts & worktrees",
        "m CHECKOUT · n WORKTREE",
        th,
    );
    let body = Rect {
        x: area.x + 2,
        y: area.y + 4.min(area.height),
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(5),
    };
    let wide = body.width >= 70;
    let state_width = 16usize;
    let panes_width = 16usize;
    let path_width = (body.width as usize / 3).max(18);
    // The selection gutter plus the two-space gap before each other column.
    let gaps = 8usize;
    let branch_width =
        (body.width as usize).saturating_sub(state_width + panes_width + path_width + gaps);
    if wide {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!("  {:<branch_width$}", "BRANCH"),
                    Style::default().fg(th.dim),
                ),
                Span::styled(
                    format!("  {:<state_width$}", "STATE"),
                    Style::default().fg(th.dim),
                ),
                Span::styled(
                    format!("  {:<panes_width$}", "PANES"),
                    Style::default().fg(th.dim),
                ),
                Span::styled(
                    format!("  {:<path_width$}", "PATH"),
                    Style::default().fg(th.dim),
                ),
            ])),
            Rect { height: 1, ..body },
        );
    }
    app.layout.checkouts = Panel {
        outer: area,
        inner: Rect {
            y: body.y + 1,
            height: body.height.saturating_sub(1),
            ..body
        },
        first: 0,
    };
    if let Some(repo) = app.current_repository() {
        for (index, checkout) in repo
            .checkouts
            .iter()
            .enumerate()
            .take((body.height.saturating_sub(1) / 2) as usize)
        {
            let selected = app.current_checkout().is_some_and(|c| c.id == checkout.id);
            let git = checkout.git.as_ref();
            let state = git
                .map(|g| {
                    if g.dirty {
                        format!("!{} modified", g.changed_files)
                    } else if g.ahead > 0 {
                        format!("↑{} clean", g.ahead)
                    } else if g.behind > 0 {
                        format!("↓{} behind", g.behind)
                    } else {
                        "clean".into()
                    }
                })
                .unwrap_or_else(|| "not a repository".into());
            let state_color = if git.is_some_and(|g| g.dirty) {
                th.warn
            } else {
                th.ok
            };
            let panes: Vec<_> = checkout.listed_panes().collect();
            let agents = panes
                .iter()
                .filter(|pane| pane.kind == PaneKind::Agent)
                .count();
            let pane_summary = if panes.is_empty() {
                "—".to_string()
            } else if agents == 0 {
                format!("{} · shell", panes.len())
            } else {
                format!(
                    "{} · {} agent{}",
                    panes.len(),
                    agents,
                    if agents == 1 { "" } else { "s" }
                )
            };
            let bg = if selected { th.surface } else { th.bg };
            let style = Style::default().bg(bg);
            let line = if wide {
                Line::from(vec![
                    Span::styled(
                        if selected { "▌ " } else { "  " },
                        style.fg(if selected { th.accent } else { bg }),
                    ),
                    Span::styled(
                        format!(
                            "{:<branch_width$}",
                            ellipsize_text(&checkout.name, branch_width)
                        ),
                        style.fg(th.text).add_modifier(if selected {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                    ),
                    Span::styled(
                        format!("  {:<state_width$}", ellipsize_text(&state, state_width)),
                        style.fg(state_color),
                    ),
                    Span::styled(
                        format!(
                            "  {:<panes_width$}",
                            ellipsize_text(&pane_summary, panes_width)
                        ),
                        style.fg(th.muted),
                    ),
                    Span::styled(
                        format!("  {:<path_width$}", elide_head(&checkout.path, path_width)),
                        style.fg(th.dim),
                    ),
                ])
            } else {
                Line::from(vec![
                    Span::styled(
                        if selected { "▌ " } else { "  " },
                        style.fg(if selected { th.accent } else { bg }),
                    ),
                    Span::styled(&checkout.name, style.fg(th.text)),
                    Span::styled(format!("  {state} · {pane_summary}"), style.fg(state_color)),
                ])
            };
            let row = Rect {
                y: body.y + 1 + index as u16 * 2,
                height: 2.min(body.bottom().saturating_sub(body.y + 1 + index as u16 * 2)),
                ..body
            };
            f.render_widget(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(th.surface)),
                row,
            );
            f.render_widget(
                Paragraph::new(line).style(style),
                Rect {
                    y: row.y + 1,
                    height: row.height.saturating_sub(1),
                    ..row
                },
            );
        }
    }
}

fn render_stage_heading(
    f: &mut Frame,
    area: Rect,
    title: &str,
    detail: &str,
    action: &str,
    th: Theme,
) {
    let header = Rect {
        height: 3.min(area.height),
        ..area
    };
    f.render_widget(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(th.edge)),
        header,
    );
    if header.x > 0 && header.height > 0 {
        f.render_widget(
            Paragraph::new(Span::styled("├", Style::default().fg(th.edge))),
            Rect {
                x: header.x - 1,
                y: header.bottom() - 1,
                width: 1,
                height: 1,
            },
        );
    }
    let line_area = Rect {
        x: header.x + 2,
        y: header.y + 1.min(header.height.saturating_sub(1)),
        width: header.width.saturating_sub(4),
        height: 1.min(header.height),
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                title,
                Style::default().fg(th.text).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  {detail}"), Style::default().fg(th.dim)),
        ])),
        line_area,
    );
    let action_width = action.chars().count().min(line_area.width as usize) as u16;
    f.render_widget(
        Paragraph::new(action).style(Style::default().fg(th.muted)),
        Rect {
            x: line_area.right().saturating_sub(action_width),
            width: action_width,
            ..line_area
        },
    );
}

fn render_first_run(f: &mut Frame, area: Rect, th: Theme) {
    let width = 52.min(area.width);
    let height = 10.min(area.height);
    let box_area = centered_rect(width, height, area);
    f.render_widget(Paragraph::new(vec![
        Line::styled("■  NO WORKSPACE", Style::default().fg(th.dim)),
        Line::raw(""),
        Line::styled("Point Argus at a directory of repositories.", Style::default().fg(th.text).add_modifier(Modifier::BOLD)),
        Line::raw(""),
        Line::styled("Argus will index checkouts and worktrees, then keep panes attached to the branch they belong to.", Style::default().fg(th.muted)),
        Line::raw(""),
        Line::from(vec![Span::styled("› ", Style::default().fg(th.accent)), Span::styled("n  choose a directory to scan", Style::default().fg(th.muted))]),
    ]).wrap(Wrap { trim: false }), box_area);
}

/// A status's glyph: the shared spinner while it works, a still mark
/// otherwise, so progress reads in the rail without opening the pane.
fn status_glyph(app: &App, status: PaneStatus, still: &'static str) -> &'static str {
    if status == PaneStatus::Working {
        crate::motion::spinner(app.frame_now(), app.epoch())
    } else {
        still
    }
}

/// The status most worth seeing among some panes: one asking for a person,
/// then one working, then one finished.
fn loudest_status<'a>(statuses: impl Iterator<Item = &'a PaneStatus>) -> Option<PaneStatus> {
    statuses.copied().max_by_key(|status| match status {
        PaneStatus::Waiting | PaneStatus::NeedsReview | PaneStatus::Failed => 4,
        PaneStatus::Working => 3,
        PaneStatus::Done => 2,
        PaneStatus::Idle => 1,
        PaneStatus::Exited { .. } => 0,
    })
}

pub(super) fn status_color(status: PaneStatus, th: Theme) -> Color {
    match status {
        PaneStatus::Waiting | PaneStatus::NeedsReview | PaneStatus::Failed => th.warn,
        PaneStatus::Working => th.ok,
        PaneStatus::Done => th.ok,
        PaneStatus::Idle | PaneStatus::Exited { .. } => th.dim,
    }
}

fn short_status(status: PaneStatus) -> &'static str {
    match status {
        PaneStatus::Idle => "IDLE",
        PaneStatus::Working => "RUNNING",
        PaneStatus::Waiting => "NEEDS YOU",
        PaneStatus::NeedsReview => "REVIEW",
        PaneStatus::Done => "DONE",
        PaneStatus::Failed => "FAILED",
        PaneStatus::Exited { .. } => "EXITED",
    }
}
