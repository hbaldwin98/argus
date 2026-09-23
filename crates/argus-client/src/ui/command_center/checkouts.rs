//! The Checkouts stage: the selected repository's checkouts as an
//! operational table.

use super::*;

/// Lines per row in the checkouts table.
const CHECKOUT_ROW: u16 = 3;

/// The navigation row (`sel_checkout`) under a click in the table.
pub(crate) fn checkout_at(app: &App, x: u16, y: u16) -> Option<usize> {
    let rows = app.layout.checkouts.inner;
    if !contains(rows, x, y) {
        return None;
    }
    let drawn = usize::from((y - rows.y) / CHECKOUT_ROW);
    let index = app.layout.checkouts.first + drawn;
    app.checkout_table_row_indices().get(index).copied()
}

pub(super) fn render_checkouts(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    let repo_name = app
        .current_repository()
        .map(|r| r.name.as_str())
        .unwrap_or("checkouts");
    let detail = if app.checkout_filter_active() {
        format!(
            "checkouts & worktrees · filter {}",
            app.checkout_filter_query_label()
        )
    } else {
        "checkouts & worktrees".to_string()
    };
    render_stage_heading(
        f,
        area,
        repo_name,
        &detail,
        "B BRANCHES · m CHECKOUT · n WORKTREE",
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
    let table_body = Rect {
        y: body.y + 1,
        height: body.height.saturating_sub(1),
        ..body
    };
    let indices = app.checkout_table_row_indices();
    let visible = (table_body.height / CHECKOUT_ROW).max(1) as usize;
    let selected_pos = indices
        .iter()
        .position(|&index| index == app.sel_checkout);
    let first = scrolled_to_show(
        app.layout.checkouts.first,
        selected_pos,
        visible,
        indices.len(),
    );
    app.layout.checkouts = Panel {
        outer: area,
        inner: table_body,
        first,
    };
    render_overflow(
        f,
        table_body,
        first,
        visible,
        indices.len(),
        app.focus_lit(Focus::Checkouts),
        th,
    );
    let rows = app.checkout_rows();
    let Some(repo) = app.current_repository() else {
        return;
    };
    for (drawn, sel) in indices
        .iter()
        .copied()
        .skip(first)
        .take(visible)
        .enumerate()
    {
        let selected = app.sel_checkout == sel;
        let (branch_label, state, state_color, pane_summary, path) = match rows.get(sel).copied() {
            Some(CheckoutRow::Checkout(i)) => {
                let checkout = &repo.checkouts[i];
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
                (
                    checkout.name.clone(),
                    state,
                    state_color,
                    pane_summary,
                    elide_head(&checkout.path, path_width),
                )
            }
            Some(CheckoutRow::Branch(i)) => (
                repo.branches.get(i).cloned().unwrap_or_default(),
                "no checkout".into(),
                th.dim,
                "—".into(),
                "—".into(),
            ),
            Some(CheckoutRow::Remote(i)) => (
                repo.remote_branches
                    .get(i)
                    .cloned()
                    .unwrap_or_default(),
                "on the remote only".into(),
                th.dim,
                "—".into(),
                "—".into(),
            ),
            None => continue,
        };
        let bg = if selected { th.surface } else { th.bg };
        let style = Style::default().bg(bg);
        let branch_style = if matches!(rows.get(sel), Some(CheckoutRow::Checkout(_))) {
            style.fg(th.text).add_modifier(if selected {
                Modifier::BOLD
            } else {
                Modifier::empty()
            })
        } else {
            style.fg(th.muted)
        };
        let line = if wide {
            Line::from(vec![
                Span::styled(
                    if selected { "▌ " } else { "  " },
                    style.fg(if selected { th.accent } else { bg }),
                ),
                Span::styled(
                    format!(
                        "{:<branch_width$}",
                        ellipsize_text(&branch_label, branch_width)
                    ),
                    branch_style,
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
                    format!("  {:<path_width$}", path),
                    style.fg(th.dim),
                ),
            ])
        } else {
            Line::from(vec![
                Span::styled(
                    if selected { "▌ " } else { "  " },
                    style.fg(if selected { th.accent } else { bg }),
                ),
                Span::styled(branch_label, branch_style),
                Span::styled(format!("  {state} · {pane_summary}"), style.fg(state_color)),
            ])
        };
        let top = body.y + 1 + drawn as u16 * CHECKOUT_ROW;
        let row = Rect {
            y: top,
            height: CHECKOUT_ROW.min(body.bottom().saturating_sub(top)),
            ..body
        };
        // The highlight fills the whole row, text on its middle line,
        // and the accent bar runs its full height.
        f.render_widget(Block::default().style(style), row);
        if selected {
            for y in row.y..row.bottom() {
                f.render_widget(
                    Paragraph::new(Span::styled("▌", style.fg(th.accent))),
                    Rect::new(row.x, y, 1, 1),
                );
            }
        }
        if row.height >= 2 {
            f.render_widget(
                Paragraph::new(line).style(style),
                Rect {
                    y: row.y + 1,
                    height: 1,
                    ..row
                },
            );
        }
        if !wide && row.height >= 3 && path != "—" {
            f.render_widget(
                Paragraph::new(Line::styled(
                    format!("  {path}"),
                    style.fg(th.dim),
                )),
                Rect {
                    y: row.y + 2,
                    height: 1,
                    ..row
                },
            );
        }
    }
}
