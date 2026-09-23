//! The Feature stage: the selected feature read as a document of brief,
//! tasks and decisions, and which row a click lands on. Rendering and
//! hit-testing share `row_heights`, so the two cannot disagree on layout.

use super::*;

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

/// The rows in a command-center feature panel are compact until its cursor
/// enters the panel. The selected row then owns as many lines as its title
/// and prose need, while every other row remains one-line scan material.
fn feature_row_start(
    previous: usize,
    selected: Option<usize>,
    heights: &[u16],
    available: u16,
) -> usize {
    if heights.is_empty() || available == 0 {
        return 0;
    }
    let mut first = previous.min(heights.len() - 1);
    let Some(selected) = selected else {
        return first;
    };
    if selected < first {
        first = selected;
    }
    while first < selected
        && heights[first..=selected]
            .iter()
            .map(|height| usize::from(*height))
            .sum::<usize>()
            > usize::from(available)
    {
        first += 1;
    }
    first
}

fn wrapped_text_lines(text: &str, width: u16) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    text.split('\n')
        .flat_map(|line| wrap(line, width))
        .collect()
}

/// Places wrapped text after a prefix and hangs continuations under its
/// readable text rather than under status or tree markers.
fn hanging_text(
    prefix: Vec<Span<'static>>,
    hang: usize,
    text: &str,
    style: Style,
    width: u16,
) -> Vec<Line<'static>> {
    let used = prefix.iter().map(Span::width).sum::<usize>();
    let room = usize::from(width).saturating_sub(used.max(hang)).max(1) as u16;
    let mut wrapped = wrapped_text_lines(text, room).into_iter();
    let first = wrapped.next().unwrap_or_default();
    let mut lines = vec![Line::from(
        prefix
            .into_iter()
            .chain(std::iter::once(Span::styled(first, style)))
            .collect::<Vec<_>>(),
    )];
    for line in wrapped {
        lines.push(Line::from(vec![
            Span::raw(" ".repeat(hang)),
            Span::styled(line, style),
        ]));
    }
    lines
}

fn line_with_right_label(
    indent: &str,
    hang: usize,
    left: &str,
    left_style: Style,
    right: &str,
    right_style: Style,
    width: u16,
) -> Line<'static> {
    let hang = hang.max(indent.chars().count());
    let right_len = right.chars().count();
    let room = usize::from(width).saturating_sub(hang + right_len);
    Line::from(vec![
        Span::styled(indent.to_string(), left_style),
        Span::styled(
            format!("{:<room$}", ellipsize_text(left, room)),
            left_style,
        ),
        Span::styled(right.to_string(), right_style),
    ])
}

fn task_id_label(id: i64) -> String {
    format!("  #{:>5}", id)
}

fn tree_guide(depth: usize, ancestor_continuations: &[bool], has_next_sibling: bool) -> String {
    let mut guide = String::new();
    if depth > 0 {
        for continues in ancestor_continuations
            .iter()
            .take(depth.saturating_sub(1))
        {
            guide.push_str(if *continues { "│ " } else { "  " });
        }
        guide.push_str(if has_next_sibling { "├ " } else { "└ " });
    }
    guide
}

fn task_guide(row: &argus_protocol::TaskTreeRow<'_>) -> String {
    tree_guide(row.depth, &row.ancestor_continuations, row.has_next_sibling)
}

fn task_children(rows: &[argus_protocol::TaskTreeRow<'_>], index: usize) -> usize {
    let row = &rows[index];
    rows[index + 1..]
        .iter()
        .take_while(|child| child.depth > row.depth)
        .filter(|child| child.depth == row.depth + 1)
        .count()
}

fn task_metadata_extra(task: &argus_protocol::Task, children: usize) -> String {
    let mut parts = Vec::new();
    if children > 0 {
        parts.push(plural(children, "subtask"));
    }
    if let Some(key) = &task.external {
        parts.push(key.clone());
    }
    if let Some(session) = &task.claimed_by {
        parts.push(session.clone());
    }
    parts.join(" · ")
}

fn task_lines(
    row: &argus_protocol::TaskTreeRow<'_>,
    children: usize,
    selected: bool,
    expanded: bool,
    width: u16,
    th: Theme,
) -> Vec<Line<'static>> {
    let task = row.task;
    let guide = task_guide(row);
    let (mark, mark_color) = match task.state {
        TaskState::Todo => ("○", th.dim),
        TaskState::Doing => ("●", th.accent),
        TaskState::Done => ("✓", th.ok),
    };
    let bg = if expanded { th.surface } else { th.bg };
    let base = Style::default().bg(bg);
    let title_style = base.fg(if task.state == TaskState::Done {
        th.dim
    } else if selected {
        th.text
    } else {
        th.muted
    });
    let marker = if selected { "▌ " } else { "  " };
    let prefix = format!("{marker}{guide}{mark} ");
    if !expanded {
        let badge = if children > 0 {
            format!(" {} ", plural(children, "subtask"))
        } else {
            String::new()
        };
        let fixed = prefix.chars().count() + badge.chars().count() + 2 + 6;
        let title_width = usize::from(width).saturating_sub(fixed);
        return vec![Line::from(vec![
            Span::styled(prefix, base.fg(if selected { th.accent } else { th.edge })),
            Span::styled(
                format!("{:<title_width$}", ellipsize_text(&task.title, title_width)),
                title_style,
            ),
            Span::styled(badge, base.fg(th.muted)),
            Span::styled(task_id_label(task.id), base.fg(th.dim)),
        ])];
    }

    let title_prefix = vec![
        Span::styled(marker, base.fg(th.accent)),
        Span::styled(guide, base.fg(th.edge)),
        Span::styled(mark, base.fg(mark_color)),
        Span::raw(" "),
    ];
    let title_hang = title_prefix.iter().map(Span::width).sum::<usize>();
    let mut lines = hanging_text(
        title_prefix,
        title_hang,
        &task.title,
        title_style.add_modifier(Modifier::BOLD),
        width,
    );
    let detail_prefix = " ".repeat(title_hang);
    lines.push(line_with_right_label(
        &detail_prefix,
        title_hang,
        &task_metadata_extra(task, children),
        base.fg(th.dim),
        &task_id_label(task.id),
        base.fg(th.dim),
        width,
    ));
    if let Some(body) = task.body.as_deref().filter(|body| !body.trim().is_empty()) {
        lines.extend(hanging_text(
            vec![Span::styled(detail_prefix, base.fg(th.muted))],
            title_hang,
            body,
            base.fg(th.muted),
            width,
        ));
    }
    lines
}

fn task_row_heights(
    rows: &[argus_protocol::TaskTreeRow<'_>],
    selected: usize,
    focused: bool,
    width: u16,
    th: Theme,
) -> Vec<u16> {
    row_heights(rows.len(), selected, focused, |index, sel, exp| {
        task_lines(&rows[index], task_children(rows, index), sel, exp, width, th).len()
    })
}

fn decision_guide(row: &argus_protocol::DecisionTreeRow<'_>) -> String {
    tree_guide(row.depth, &row.ancestor_continuations, row.has_next_sibling)
}

fn decision_detail(decision: &argus_protocol::Decision) -> String {
    let mut parts = Vec::new();
    if let Some(over) = &decision.over {
        parts.push(format!("over {over}"));
    }
    if let Some(because) = &decision.because {
        parts.push(format!("because {because}"));
    }
    if parts.is_empty() {
        "no alternative or reason recorded".into()
    } else {
        parts.join(" · ")
    }
}

fn decision_lines(
    row: &argus_protocol::DecisionTreeRow<'_>,
    selected: bool,
    expanded: bool,
    width: u16,
    th: Theme,
) -> Vec<Line<'static>> {
    let decision = row.decision;
    let bg = if expanded { th.surface } else { th.bg };
    let base = Style::default().bg(bg);
    let name_style = base.fg(if decision.superseded() {
        th.dim
    } else if selected {
        th.text
    } else {
        th.muted
    });
    let guide = decision_guide(row);
    let marker = if selected { "▌ " } else { "  " };
    let detail = decision_detail(decision);
    let lead = format!("{marker}{guide}");
    let mark_style = base.fg(if decision.superseded() {
        th.dim
    } else {
        th.muted
    });
    if !expanded {
        let id_label = task_id_label(decision.id);
        let pad = super::decision_title_pad(row.depth);
        let fixed = lead.chars().count() + pad.chars().count() + id_label.chars().count();
        let title_width = usize::from(width).saturating_sub(fixed);
        let edge = base.fg(if selected { th.accent } else { th.edge });
        let mut spans = vec![Span::styled(lead, edge)];
        if row.depth == 0 {
            spans.push(Span::styled(super::DECISION_ROOT_MARK, mark_style));
            spans.push(Span::raw(" "));
        } else {
            spans.push(Span::raw("  "));
        }
        spans.extend([
            Span::styled(
                format!("{:<title_width$}", ellipsize_text(&decision.chose, title_width)),
                name_style,
            ),
            Span::styled(id_label, base.fg(th.dim)),
        ]);
        return vec![Line::from(spans)];
    }

    let mut title_prefix = vec![
        Span::styled(marker, base.fg(th.accent)),
        Span::styled(guide, base.fg(th.edge)),
    ];
    if row.depth == 0 {
        title_prefix.push(Span::styled(super::DECISION_ROOT_MARK, mark_style));
        title_prefix.push(Span::raw(" "));
    } else {
        title_prefix.push(Span::raw("  "));
    }
    let title_hang = title_prefix.iter().map(Span::width).sum::<usize>();
    let mut choice = decision.chose.clone();
    if let Some(by) = decision.superseded_by {
        choice.push_str(&format!("  superseded by #{by}"));
    }
    let mut lines = hanging_text(
        title_prefix,
        title_hang,
        &choice,
        name_style.add_modifier(Modifier::BOLD),
        width,
    );
    let detail_prefix = " ".repeat(title_hang);
    let detail_body = format!("{}  {}", detail, task_id_label(decision.id).trim());
    lines.extend(hanging_text(
        vec![Span::raw(detail_prefix)],
        title_hang,
        &detail_body,
        base.fg(th.dim),
        width,
    ));
    lines
}

fn decision_row_heights(
    rows: &[argus_protocol::DecisionTreeRow<'_>],
    selected: usize,
    focused: bool,
    width: u16,
    th: Theme,
) -> Vec<u16> {
    row_heights(rows.len(), selected, focused, |index, sel, exp| {
        decision_lines(&rows[index], sel, exp, width, th).len()
    })
}

/// How many lines each of `count` rows draws to, given which is selected and
/// whether the panel holding them is focused — the one piece of
/// `task_row_heights` and `decision_row_heights` that was byte-for-byte
/// identical; the rendering itself stays in `task_lines`/`decision_lines`,
/// which diverge enough (subtask badges vs. superseded/root marks) that
/// merging them would cost more clarity than the duplication does.
fn row_heights(
    count: usize,
    selected: usize,
    focused: bool,
    mut lines_for: impl FnMut(usize, bool, bool) -> usize,
) -> Vec<u16> {
    (0..count)
        .map(|index| lines_for(index, index == selected, focused && index == selected) as u16)
        .collect()
}

/// Returns the feature row under the pointer using the same variable heights
/// the command-center renderer used for the preceding frame.
pub(crate) fn feature_row_at(
    app: &App,
    which: FeaturePanel,
    x: u16,
    y: u16,
) -> Option<usize> {
    let panel = match which {
        FeaturePanel::Features => app.layout.features,
        FeaturePanel::Tasks => app.layout.feature_tasks,
        FeaturePanel::Diagrams => app.layout.feature_diagrams,
        FeaturePanel::Decisions => app.layout.feature_decisions,
    };
    if !contains(panel.inner, x, y) {
        return None;
    }
    let heights = match which {
        FeaturePanel::Features => vec![1; app.feature_rows().len()],
        FeaturePanel::Tasks => {
            let rows = app.feature_task_rows();
            task_row_heights(
                &rows,
                app.task_sel,
                app.panel == FeaturePanel::Tasks,
                panel.inner.width,
                app.theme.drawn(),
            )
        }
        FeaturePanel::Diagrams => vec![1; app.feature_diagrams().len()],
        FeaturePanel::Decisions => {
            let rows = app.board_rows();
            decision_row_heights(
                &rows,
                app.decision_sel,
                app.panel == FeaturePanel::Decisions,
                panel.inner.width,
                app.theme.drawn(),
            )
        }
    };
    let offset = usize::from(y.saturating_sub(panel.inner.y));
    let mut top = 0usize;
    for (index, height) in heights.iter().enumerate().skip(panel.first) {
        let bottom = top + usize::from(*height);
        if offset < bottom {
            return Some(index);
        }
        top = bottom;
    }
    None
}

pub(super) fn render_feature_document(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
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
    // Unfocused rows cost one line. The row under the keys pays for all of
    // its wrapped title and prose, which keeps the rest of the document
    // scannable without hiding the text being read.
    let tasks_y = brief_area.bottom() + 1;
    if tasks_y + 1 >= bottom {
        return;
    }
    let rows = app.feature_task_rows();
    let decisions = app.board_rows();
    let focused_tasks = app.panel == FeaturePanel::Tasks;
    let focused_decisions = app.panel == FeaturePanel::Decisions;
    let task_heights = task_row_heights(
        &rows,
        app.task_sel,
        focused_tasks,
        body.width,
        th,
    );
    let decision_heights = decision_row_heights(
        &decisions,
        app.decision_sel,
        focused_decisions,
        body.width,
        th,
    );
    drop(decisions);
    let rest = bottom.saturating_sub(tasks_y);
    let wants_tasks = task_heights.iter().sum::<u16>().max(1) + 1;
    // The gap row, the heading, and at least one decision summary.
    let wants_decisions = decision_heights.iter().sum::<u16>().max(1) + 2;
    let task_block = if focused_tasks {
        // Keep the decision section to its smallest useful shape so the row
        // being read can receive all the height its wrapped prose asks for.
        wants_tasks.min(rest.saturating_sub(3)).max(rest.min(2))
    } else if focused_decisions {
        // The same priority in reverse: one compact task row is enough while
        // the selected decision gets the remaining document height.
        wants_tasks
            .min(rest.saturating_sub(wants_decisions.min(rest.saturating_sub(2))))
            .max(rest.min(2))
    } else if wants_tasks + wants_decisions <= rest {
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
    let task_first = feature_row_start(
        app.layout.feature_tasks.first,
        Some(app.task_sel),
        &task_heights,
        tasks_area.height,
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
    let mut task_used = 0u16;
    let mut task_visible = 0usize;
    for (index, row) in rows.iter().enumerate().skip(task_first) {
        let selected = index == app.task_sel;
        let lines = task_lines(
            row,
            task_children(&rows, index),
            selected,
            focused_tasks && selected,
            tasks_area.width,
            th,
        );
        let height = lines.len() as u16;
        let available = tasks_area.height.saturating_sub(task_used);
        if available == 0 {
            break;
        }
        let drawn_height = height.min(available);
        f.render_widget(
            Paragraph::new(lines),
            Rect {
                y: tasks_area.y + task_used,
                height: drawn_height,
                ..tasks_area
            },
        );
        task_used += drawn_height;
        task_visible += 1;
        if drawn_height < height {
            break;
        }
    }
    section_thumb(
        f,
        tasks_area,
        task_first,
        task_visible,
        rows.len(),
        th,
    );
    drop(rows);

    let diagrams = app.feature_diagrams();
    let diagrams_y = tasks_area.bottom() + 1;
    if diagrams_y + 1 >= bottom {
        return;
    }
    let focused_diagrams = app.panel == FeaturePanel::Diagrams;
    section_heading(
        f,
        Rect { y: diagrams_y, ..body },
        "SEQUENCE",
        "a add · enter open",
        th,
    );
    let diagram_height = if diagrams.is_empty() {
        1u16
    } else {
        (diagrams.len() as u16).min(3)
    }
    .min(bottom.saturating_sub(diagrams_y + 2));
    let diagrams_area = Rect {
        y: diagrams_y + 1,
        height: diagram_height,
        ..body
    };
    let diagram_first = scrolled_to_show(
        app.layout.feature_diagrams.first,
        Some(app.diagram_sel),
        diagram_height as usize,
        diagrams.len(),
    );
    if diagrams.is_empty() && diagrams_area.height > 0 {
        f.render_widget(
            Paragraph::new("no flows yet — a adds a sequence diagram")
                .style(Style::default().fg(th.dim)),
            Rect {
                height: 1,
                ..diagrams_area
            },
        );
    }
    for (index, diagram) in diagrams
        .iter()
        .enumerate()
        .skip(diagram_first)
        .take(diagram_height as usize)
    {
        let selected = index == app.diagram_sel;
        let bg = if selected && focused_diagrams {
            th.surface
        } else {
            th.bg
        };
        let style = Style::default().bg(bg);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(if selected { "▌ " } else { "  " }, style.fg(th.accent)),
                Span::styled(
                    ellipsize_text(&diagram.title, diagrams_area.width as usize),
                    style.fg(if selected { th.text } else { th.muted }).add_modifier(
                        if selected {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        },
                    ),
                ),
            ]))
            .style(style),
            Rect {
                y: diagrams_area.y + (index - diagram_first) as u16,
                height: 1,
                ..diagrams_area
            },
        );
    }
    section_thumb(
        f,
        diagrams_area,
        diagram_first,
        diagram_height as usize,
        diagrams.len(),
        th,
    );
    let decisions_y = diagrams_area.bottom() + 1;
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
    let decision_first = feature_row_start(
        app.layout.feature_decisions.first,
        Some(app.decision_sel),
        &decision_heights,
        decisions_area.height,
    );
    let mut decision_used = 0u16;
    let mut decision_visible = 0usize;
    for (index, row) in decisions.iter().enumerate().skip(decision_first) {
        let selected = index == app.decision_sel;
        let lines = decision_lines(
            row,
            selected,
            focused_decisions && selected,
            decisions_area.width,
            th,
        );
        let height = lines.len() as u16;
        let available = decisions_area.height.saturating_sub(decision_used);
        if available == 0 {
            break;
        }
        let drawn_height = height.min(available);
        f.render_widget(
            Paragraph::new(lines),
            Rect {
                y: decisions_area.y + decision_used,
                height: drawn_height,
                ..decisions_area
            },
        );
        decision_used += drawn_height;
        decision_visible += 1;
        if drawn_height < height {
            break;
        }
    }
    section_thumb(
        f,
        decisions_area,
        decision_first,
        decision_visible,
        decisions.len(),
        th,
    );
    drop(decisions);
    app.layout.feature_tasks = Panel {
        outer: tasks_area,
        inner: tasks_area,
        first: task_first,
    };
    app.layout.feature_diagrams = Panel {
        outer: diagrams_area,
        inner: diagrams_area,
        first: diagram_first,
    };
    app.layout.feature_decisions = Panel {
        outer: decisions_area,
        inner: decisions_area,
        first: decision_first,
    };
}
