//! The contextual rail: the workspace summary, the selected project's
//! repositories and checkouts, and the live agents rolled up beneath them,
//! with the hit-tests that map a click back onto a row.

use super::*;

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

/// The project name and the workspace line under it: clicking either opens
/// the project picker, since the rail shows one project at a time.
pub(crate) fn project_header_at(app: &App, x: u16, y: u16) -> bool {
    let outer = app.layout.projects.outer;
    contains(outer, x, y) && (outer.y + 1..=outer.y + 2).contains(&y)
}

fn project_root_line(app: &App, width: u16) -> String {
    let Some(project) = app.current_project() else {
        return String::new();
    };
    let max = usize::from(width.saturating_sub(4));
    if let Some(root) = project.root.as_deref() {
        return ellipsize_text(&display_path(root), max);
    }
    project
        .repositories
        .iter()
        .flat_map(|r| r.checkouts.iter())
        .find(|c| c.primary)
        .or_else(|| {
            project
                .repositories
                .iter()
                .flat_map(|r| r.checkouts.iter())
                .next()
        })
        .map(|c| ellipsize_text(&display_path(&c.path), max))
        .unwrap_or_default()
}

const BADGE_CHIP_HEIGHT: u16 = 1;

fn summary_chip_label(count: usize, unit: &str) -> String {
    format!("{count:>2} {}", unit.to_ascii_uppercase())
}

fn badge_chip_label(text: &str) -> String {
    format!(" {} ", text.to_ascii_uppercase())
}

fn badge_chip_width(text: &str) -> u16 {
    badge_chip_label(text).chars().count() as u16
}

fn render_badge_chip(f: &mut Frame, area: Rect, text: &str, fg: Color, th: Theme) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    f.render_widget(
        Paragraph::new(badge_chip_label(text))
            .style(filled_badge(fg, th))
            .alignment(Alignment::Center),
        area,
    );
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

    // Header: label, the project under an accent bar, and its root path.
    let x = inner.x + 1;
    let width = inner.width.saturating_sub(2);
    let root_line = project_root_line(app, width);
    let put = |f: &mut Frame, y: u16, line: Line| {
        if y < inner.bottom() {
            f.render_widget(Paragraph::new(line), Rect::new(x, y, width, 1));
        }
    };
    put(
        f,
        inner.y,
        Line::styled("ACTIVE WORKSPACE", Style::default().fg(th.dim)),
    );
    // With more than one project, the header says which of them this is
    // and that `o` reaches the rest.
    let position = if app.tree.len() > 1 {
        format!(" {}/{} ▾", app.sel_project + 1, app.tree.len())
    } else {
        String::new()
    };
    let name_width = (width as usize).saturating_sub(2 + position.chars().count());
    put(
        f,
        inner.y + 1,
        Line::from(vec![
            Span::styled("▌ ", Style::default().fg(th.accent)),
            Span::styled(
                ellipsize_text(&name, name_width),
                Style::default().fg(th.text).add_modifier(Modifier::BOLD),
            ),
            Span::styled(position, Style::default().fg(th.dim)),
        ]),
    );
    if !root_line.is_empty() {
        put(
            f,
            inner.y + 2,
            Line::styled(
                format!("  {root_line}"),
                Style::default().fg(th.dim),
            ),
        );
    }

    // Badges: one-row chips on a raised surface fill when the rail is wide enough.
    let mut badges = vec![
        (summary_chip_label(repo_count, "repos"), th.muted),
        (summary_chip_label(agents, "agents"), th.ok),
    ];
    if needs > 0 {
        badges.push((summary_chip_label(needs, "need"), th.warn));
    }
    let gap = 1u16;
    let badge_row_end = x.saturating_add(width);
    let header_rows = 2 + u16::from(!root_line.is_empty());
    let mut bx = x;
    let mut by = inner.y + header_rows;
    for (text, color) in badges {
        let chip_w = badge_chip_width(&text);
        if bx > x && bx.saturating_add(chip_w) > badge_row_end {
            bx = x;
            by = by.saturating_add(BADGE_CHIP_HEIGHT);
        }
        if by.saturating_add(BADGE_CHIP_HEIGHT) <= inner.bottom() {
            let w = chip_w.min(badge_row_end.saturating_sub(bx));
            render_badge_chip(
                f,
                Rect::new(bx, by, w, BADGE_CHIP_HEIGHT),
                &text,
                color,
                th,
            );
        }
        bx = bx.saturating_add(chip_w + gap);
    }
    let summary_height = (by + BADGE_CHIP_HEIGHT)
        .saturating_sub(inner.y)
        .min(inner.height);

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
