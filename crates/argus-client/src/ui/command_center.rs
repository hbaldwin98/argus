//! The flat command-center shell specified by `Argus Command Center.dc.html`.
//!
//! This module owns presentation only. Every selection still points into the
//! daemon's tree; the shell merely gives that tree a stable rail and focused
//! operational surfaces.

use super::*;
use crate::app::FeaturePanel;
use argus_protocol::{PaneKind, PaneStatus, TaskState};

pub const SIDEBAR_WIDTH: u16 = 32;
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
        let expanded = app.expanded_repositories.contains(&repo.id)
            || (repository == app.sel_repository
                && app.view == View::Spine
                && app.focus == Focus::PaneContent);
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

    let rail_width = SIDEBAR_WIDTH.min(frame[1].width.saturating_sub(24).max(1));
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
    if frame[0].height > 1 && rail_width > 0 {
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
    let summary_height = 7.min(inner.height);
    let agents = app
        .current_project()
        .map(|p| {
            p.repositories
                .iter()
                .flat_map(|r| r.checkouts.iter())
                .flat_map(|c| c.listed_panes())
                .filter(|p| p.kind == PaneKind::Agent)
                .count()
        })
        .unwrap_or(0);
    let needs = app
        .current_project()
        .map(|p| {
            p.repositories
                .iter()
                .flat_map(|r| r.checkouts.iter())
                .flat_map(|c| c.listed_panes())
                .filter(|p| p.status.needs_you())
                .count()
        })
        .unwrap_or(0);
    let project = app.current_project();
    let repo_count = project.map(|p| p.repositories.len()).unwrap_or(0);
    let mut summary = vec![
        Line::styled("ACTIVE WORKSPACE", Style::default().fg(th.dim)),
        Line::from(vec![
            Span::styled("▌ ", Style::default().fg(th.accent)),
            Span::styled(
                if app.open_workspace.is_empty() {
                    "default"
                } else {
                    &app.open_workspace
                },
                Style::default().fg(th.text).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::styled(
            project.map(|p| p.name.as_str()).unwrap_or("no project"),
            Style::default().fg(th.muted),
        ),
        Line::raw(""),
        Line::from(vec![
            Span::styled(format!(" {repo_count} repos "), badge(th.muted, th)),
            Span::raw(" "),
            Span::styled(format!(" {agents} agents "), badge(th.ok, th)),
        ]),
    ];
    if needs > 0 {
        summary.push(Line::styled(
            format!(" {needs} needs you"),
            Style::default().fg(th.warn),
        ));
    }
    f.render_widget(
        Paragraph::new(summary).block(Block::default().padding(Padding::new(2, 1, 1, 0))),
        Rect {
            height: summary_height,
            ..inner
        },
    );

    let agent_rows = 5.min(inner.height.saturating_sub(summary_height));
    let repos_area = Rect {
        y: inner.y.saturating_add(summary_height),
        height: inner.height.saturating_sub(summary_height + agent_rows),
        ..inner
    };
    render_repositories(f, app, repos_area, th);
    let agents_area = Rect {
        y: repos_area.y.saturating_add(repos_area.height),
        height: agent_rows,
        ..inner
    };
    render_agents(f, app, agents_area, th);
    for y in [repos_area.y, agents_area.y] {
        if y < area.bottom() && area.width > 0 {
            f.render_widget(
                Paragraph::new(Span::styled("├", Style::default().fg(th.edge))),
                Rect {
                    x: area.right() - 1,
                    y,
                    width: 1,
                    height: 1,
                },
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
    if area.height < 2 {
        return;
    }
    f.render_widget(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(th.edge)),
        area,
    );
    let heading = Rect {
        y: area.y + 1,
        height: 1,
        ..area
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  REPOSITORIES", Style::default().fg(th.dim)),
            Span::styled("                 j/k", Style::default().fg(th.edge)),
        ])),
        heading,
    );
    let rows_area = Rect {
        y: heading.y + 1,
        height: area.bottom().saturating_sub(heading.y + 1),
        ..area
    };
    let targets = rail_targets(app);
    let first = app
        .layout
        .projects
        .first
        .min(targets.len().saturating_sub(1));
    for (screen, target) in targets.iter().skip(first).enumerate() {
        if screen as u16 >= rows_area.height {
            break;
        }
        let y = rows_area.y + screen as u16;
        let Some(project) = app.current_project() else {
            break;
        };
        let line = match *target {
            RailTarget::Repository(index) => {
                let repo = &project.repositories[index];
                let selected = index == app.sel_repository;
                let panes = repo
                    .checkouts
                    .iter()
                    .map(|c| c.listed_panes().count())
                    .sum::<usize>();
                let active = panes > 0;
                let bg = if selected { th.surface } else { th.bg };
                let name_width = rows_area.width.saturating_sub(11) as usize;
                Line::from(vec![
                    Span::styled(
                        if selected { "▌" } else { " " },
                        Style::default()
                            .fg(if selected { th.accent } else { bg })
                            .bg(bg),
                    ),
                    Span::styled(
                        if active { " ● " } else { " · " },
                        Style::default()
                            .fg(if active { th.ok } else { th.dim })
                            .bg(bg),
                    ),
                    Span::styled(
                        format!("{:<name_width$}", ellipsize_text(&repo.name, name_width)),
                        Style::default()
                            .fg(if selected { th.text } else { th.muted })
                            .bg(bg)
                            .add_modifier(if selected {
                                Modifier::BOLD
                            } else {
                                Modifier::empty()
                            }),
                    ),
                    Span::styled(
                        if panes == 0 {
                            String::new()
                        } else {
                            format!("{panes}p  ")
                        },
                        Style::default().fg(th.dim).bg(bg),
                    ),
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
                        ellipsize_text(branch, 12),
                        Style::default().fg(th.syntax.function),
                    ),
                    Span::styled(
                        format!("  {state}"),
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
                    && app.focus == Focus::PaneContent;
                let bg = if selected { th.surface } else { th.bg };
                let color = status_color(pane.status, th);
                Line::from(vec![
                    Span::styled(
                        if selected { "▌      " } else { "       " },
                        Style::default().fg(if selected { th.accent } else { th.edge }).bg(bg),
                    ),
                    Span::styled("└ ", Style::default().fg(th.edge).bg(bg)),
                    Span::styled(
                        if pane.kind == PaneKind::Agent { "◆ " } else { "› " },
                        Style::default().fg(color).bg(bg),
                    ),
                    Span::styled(
                        ellipsize_text(&pane.title, rows_area.width.saturating_sub(14) as usize),
                        Style::default()
                            .fg(if selected { th.text } else { th.muted })
                            .bg(bg)
                            .add_modifier(if selected { Modifier::BOLD } else { Modifier::empty() }),
                    ),
                ])
            }
        };
        f.render_widget(
            Paragraph::new(line),
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

fn render_agents(f: &mut Frame, app: &App, area: Rect, th: Theme) {
    if area.height < 2 {
        return;
    }
    f.render_widget(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(th.edge)),
        area,
    );
    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };
    f.render_widget(
        Paragraph::new(Line::styled("AGENTS", Style::default().fg(th.dim))),
        Rect { height: 1, ..inner },
    );
    if let Some(project) = app.current_project() {
        for (index, (repo, checkout, pane)) in project
            .repositories
            .iter()
            .flat_map(|repo| {
                repo.checkouts.iter().flat_map(move |checkout| {
                    checkout
                        .listed_panes()
                        .map(move |pane| (repo, checkout, pane))
                })
            })
            .filter(|(_, _, pane)| pane.kind == PaneKind::Agent)
            .take(inner.height.saturating_sub(1) as usize)
            .enumerate()
        {
            let color = status_color(pane.status, th);
            let status = short_status(pane.status);
            let status_width = status.chars().count();
            let name_width = inner.width.saturating_sub(status_width as u16 + 2) as usize;
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("▪ ", Style::default().fg(color)),
                    Span::styled(
                        format!(
                            "{:<name_width$}",
                            ellipsize_text(
                                &format!("{} · {} #{}", repo.name, checkout.name, pane.id.0),
                                name_width
                            )
                        ),
                        Style::default().fg(th.muted),
                    ),
                    Span::styled(status, Style::default().fg(color)),
                ])),
                Rect {
                    y: inner.y + 1 + index as u16,
                    height: 1,
                    ..inner
                },
            );
        }
    }
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
        header.push_span(Span::styled(
            format!("   {} ", short_status(pane.status)),
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
    let terminal = inset(split[1], 2, 1);
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

fn render_feature_document(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    forget_feature_view(app);
    let (title, counts, brief_text) = app
        .selected_feature()
        .map(|feature| {
            (
                feature.title.clone(),
                format!("{}/{} TASKS", feature.tasks.done, feature.tasks.total()),
                if feature.body.is_empty() {
                    "No brief yet — e edits the feature brief.".to_string()
                } else {
                    feature.body.clone()
                },
            )
        })
        .unwrap_or_else(|| {
            (
                "No feature selected".to_string(),
                String::new(),
                "No brief yet — e edits the feature brief.".to_string(),
            )
        });
    render_stage_heading(f, area, "features /", &title, &counts, th);
    app.layout.features = Panel {
        outer: Rect {
            height: 3.min(area.height),
            ..area
        },
        inner: Rect {
            height: 3.min(area.height),
            ..area
        },
        first: app.feature_sel,
    };

    let body = Rect {
        x: area.x.saturating_add(2),
        y: area.y.saturating_add(4.min(area.height)),
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(5),
    };
    if body.height == 0 {
        return;
    }
    f.render_widget(
        Paragraph::new(Line::styled("BRIEF", Style::default().fg(th.dim))),
        Rect { height: 1, ..body },
    );
    let brief_height = 4.min(body.height.saturating_sub(1));
    let brief_area = Rect {
        y: body.y + 1,
        height: brief_height,
        ..body
    };
    f.render_widget(
        Paragraph::new(brief_text)
            .style(Style::default().fg(th.muted))
            .wrap(Wrap { trim: true })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(th.edge))
                    .padding(Padding::horizontal(1)),
            ),
        brief_area,
    );
    app.layout.feature_brief = Panel {
        outer: brief_area,
        inner: inset(brief_area, 1, 1),
        first: 0,
    };

    let tasks_y = brief_area.bottom().saturating_add(1);
    if tasks_y >= body.bottom() {
        return;
    }
    f.render_widget(
        Paragraph::new("TASKS    one level · enter opens brief").style(Style::default().fg(th.dim)),
        Rect {
            y: tasks_y,
            height: 1,
            ..body
        },
    );
    let rows = app.feature_task_rows();
    let task_height = (rows.len() as u16)
        .min(7)
        .min(body.bottom().saturating_sub(tasks_y + 1));
    let tasks_area = Rect {
        y: tasks_y + 1,
        height: task_height,
        ..body
    };
    for (index, row) in rows.iter().take(task_height as usize).enumerate() {
        let selected = index == app.task_sel && app.panel == FeaturePanel::Tasks;
        let mark = match row.task.state {
            TaskState::Todo => "○",
            TaskState::Doing => "●",
            TaskState::Done => "✓",
        };
        let color = match row.task.state {
            TaskState::Todo => th.dim,
            TaskState::Doing => th.accent,
            TaskState::Done => th.ok,
        };
        let indent = "  ".repeat(row.depth.min(3));
        let style = Style::default().bg(if selected { th.surface } else { th.bg });
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    if selected { "▌" } else { " " },
                    style.fg(if selected { th.accent } else { th.bg }),
                ),
                Span::styled(format!(" {indent}{mark} "), style.fg(color)),
                Span::styled(
                    ellipsize_text(
                        &row.task.title,
                        tasks_area.width.saturating_sub(16) as usize,
                    ),
                    style.fg(if row.task.state == TaskState::Done {
                        th.dim
                    } else {
                        th.text
                    }),
                ),
                Span::styled(format!("  #{}", row.task.id), style.fg(th.dim)),
            ])),
            Rect {
                y: tasks_area.y + index as u16,
                height: 1,
                ..tasks_area
            },
        );
    }
    app.layout.feature_tasks = Panel {
        outer: tasks_area,
        inner: tasks_area,
        first: 0,
    };

    let decisions_y = tasks_area.bottom().saturating_add(1);
    if decisions_y >= body.bottom() {
        return;
    }
    f.render_widget(
        Paragraph::new("DECISIONS").style(Style::default().fg(th.dim)),
        Rect {
            y: decisions_y,
            height: 1,
            ..body
        },
    );
    let decisions_area = Rect {
        y: decisions_y + 1,
        height: body.bottom().saturating_sub(decisions_y + 1),
        ..body
    };
    let rows = app.board_rows();
    let mut y = decisions_area.y;
    for (index, row) in rows.iter().enumerate() {
        if y >= decisions_area.bottom() {
            break;
        }
        let selected = index == app.decision_sel && app.panel == FeaturePanel::Decisions;
        let decision = row.decision;
        let reason = decision
            .because
            .as_deref()
            .or(decision.over.as_deref())
            .unwrap_or("No reasoning recorded");
        let style = Style::default().bg(if selected { th.surface } else { th.bg });
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    if selected { "▌" } else { "│" },
                    style.fg(if selected { th.accent } else { th.edge }),
                ),
                Span::styled(
                    format!(" #{}  {}", decision.id, decision.chose),
                    style.fg(if decision.superseded() {
                        th.dim
                    } else {
                        th.text
                    }),
                ),
            ])),
            Rect {
                y,
                height: 1,
                ..decisions_area
            },
        );
        y += 1;
        if y < decisions_area.bottom() {
            f.render_widget(
                Paragraph::new(format!("     {reason}")).style(Style::default().fg(th.dim)),
                Rect {
                    y,
                    height: 1,
                    ..decisions_area
                },
            );
            y += 1;
        }
    }
    app.layout.feature_decisions = Panel {
        outer: decisions_area,
        inner: decisions_area,
        first: 0,
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
            .style(Style::default().bg(if selected { th.surface } else { th.bg }));
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
    let path_width = 18usize;
    let gaps = 6usize;
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
                        format!(
                            "  {:<path_width$}",
                            ellipsize_text(&checkout.path, path_width)
                        ),
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

fn status_color(status: PaneStatus, th: Theme) -> Color {
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
