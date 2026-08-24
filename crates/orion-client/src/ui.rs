//! The renderer. Four columns, always all four (DESIGN.md §4): projects,
//! checkouts, open panes, and the selected pane's live view. Descending
//! moves focus rightward; it never replaces the columns with a full-screen
//! view, so an agent's output is always visible next to the tree it belongs
//! to.
//!
//! Every color goes through [`crate::theme::Theme`] rather than being named
//! here, and the visual language is deliberately narrow:
//!
//! - **Focus** is a rounded accent border plus a filled accent title chip,
//!   and a faint wash over the whole column. Unfocused columns get a
//!   receding `edge` border and a muted title.
//! - **Selection** is a raised background bar with an accent `▌` marker,
//!   never reverse video — reverse fights with the per-row status colors.
//! - **State** is a single `●` dot in the row's status color (§8b), rolled
//!   up to parents by the worst child.

use orion_protocol::{Color as PColor, GitStatus, LineKind, PaneStatus};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Widget, Wrap};
use ratatui::Frame;

use crate::app::{App, Focus, Prompt};
use crate::grid::Grid;
use crate::review::{Row, ReviewView};
use crate::theme::Theme;

/// The selection marker, and the blank gutter every other row gets so text
/// stays aligned whether or not it's selected.
const MARKER: &str = "▌";
const GUTTER: &str = " ";

pub fn render(f: &mut Frame, app: &mut App) {
    let th = app.theme;
    // A blank row above the status bar keeps it off the column borders.
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(2)])
        .split(f.area());

    render_columns(f, app, root[0]);
    render_status(f, app, root[1], th);

    if app.picker.is_some() {
        render_picker(f, app, f.area(), th);
    }
    if app.prompt.is_some() {
        render_prompt(f, app, f.area(), th);
    }
}

/// Always draws all four columns side by side, so an agent's output stays
/// visible next to the rest of the tree instead of taking over the screen.
fn render_columns(f: &mut Frame, app: &mut App, area: Rect) {
    let th = app.theme;
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(18),
            Constraint::Percentage(21),
            Constraint::Percentage(21),
            Constraint::Percentage(40),
        ])
        .split(area);

    let project_rows: Vec<Vec<Span>> = app
        .tree
        .iter()
        .map(|p| {
            let panes: usize = p.checkouts.iter().map(|c| c.panes.len()).sum();
            let status = p
                .checkouts
                .iter()
                .filter_map(worst_pane_status)
                .max_by_key(rank);
            vec![
                status_dot(status, th),
                Span::styled(p.name.clone(), Style::default().fg(th.text)),
                Span::styled(
                    format!("  {}⑂ {}▣", p.checkouts.len(), panes),
                    Style::default().fg(th.dim),
                ),
            ]
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
        cols[0],
        &projects_title,
        project_rows,
        app.focus == Focus::Projects,
        Some(app.sel_project).filter(|_| !app.tree.is_empty()),
        "no projects — n: add",
        th,
    );

    let checkout_rows: Vec<Vec<Span>> = app
        .current_project()
        .map(|p| {
            p.checkouts
                .iter()
                .map(|c| {
                    let mut spans = vec![
                        status_dot(worst_pane_status(c), th),
                        Span::styled(
                            format!("{} ", if c.primary { "⌂" } else { "⧉" }),
                            Style::default().fg(if c.primary { th.muted } else { th.dim }),
                        ),
                        Span::styled(c.name.clone(), Style::default().fg(th.text)),
                    ];
                    // A checkout is usually sitting on the branch it's named
                    // after; repeating it ("master master") wastes the width
                    // these columns don't have. Show the branch only when it
                    // actually differs.
                    spans.extend(git_spans_unless_branch_is(c.git.as_ref(), &c.name, th));
                    if !c.panes.is_empty() {
                        spans.push(Span::styled(
                            format!("  {}▣", c.panes.len()),
                            Style::default().fg(th.dim),
                        ));
                    }
                    spans
                })
                .collect()
        })
        .unwrap_or_default();
    let ncheck = app.current_project().map(|p| p.checkouts.len()).unwrap_or(0);
    app.layout.checkouts = render_column(
        f,
        cols[1],
        "checkouts",
        checkout_rows,
        app.focus == Focus::Checkouts,
        Some(app.sel_checkout).filter(|_| ncheck > 0),
        "no checkouts",
        th,
    );

    let pane_rows: Vec<Vec<Span>> = app
        .current_checkout()
        .map(|c| {
            c.panes
                .iter()
                .map(|p| {
                    vec![
                        status_dot(Some(p.status), th),
                        Span::styled(p.title.clone(), Style::default().fg(th.text)),
                        Span::styled(format!("  #{}", p.id.0), Style::default().fg(th.dim)),
                        Span::styled(exit_note(p.status), Style::default().fg(th.err)),
                    ]
                })
                .collect()
        })
        .unwrap_or_default();
    let npane = app.current_checkout().map(|c| c.panes.len()).unwrap_or(0);
    app.layout.panes = render_column(
        f,
        cols[2],
        "panes",
        pane_rows,
        app.focus == Focus::Panes,
        Some(app.sel_pane).filter(|_| npane > 0),
        "no panes — s: shell   a: agent",
        th,
    );

    render_content(f, app, cols[3], th);
}

/// Renders one bordered column of rows and returns its inner (post-border)
/// area, so the caller can hit-test mouse clicks against the same rows.
#[allow(clippy::too_many_arguments)]
fn render_column(
    f: &mut Frame,
    area: Rect,
    title: &str,
    rows: Vec<Vec<Span>>,
    focused: bool,
    selected: Option<usize>,
    empty_hint: &str,
    th: Theme,
) -> Rect {
    let block = panel_block(title, focused, th);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if focused {
        wash(f.buffer_mut(), inner, th);
    }

    if rows.is_empty() {
        // Wrapped, not truncated: these columns are narrow, and a hint cut
        // off mid-word ("no projects — ") tells the user nothing.
        f.render_widget(
            Paragraph::new(empty_hint)
                .style(Style::default().fg(th.dim))
                .wrap(Wrap { trim: true }),
            inner,
        );
        return inner;
    }

    // Scroll the window so the selection stays on screen in a long list.
    let height = inner.height as usize;
    let first = selected
        .filter(|s| height > 0 && *s >= height)
        .map(|s| s + 1 - height)
        .unwrap_or(0);

    for (i, spans) in rows.into_iter().enumerate().skip(first).take(height) {
        let Some(row) = row_rect(inner, i - first) else { break };
        render_row(f, row, spans, selected == Some(i), focused, th);
    }
    inner
}

/// One full-width row. The selection reads as a raised bar with an accent
/// marker pinning it; unselected rows get a blank gutter so the text lines
/// up either way. `dim` spans would sink into the selection fill, so they
/// are lifted to `muted` there.
fn render_row(f: &mut Frame, area: Rect, spans: Vec<Span>, selected: bool, focused: bool, th: Theme) {
    let bar = match (selected, focused) {
        (true, true) => Style::default().bg(th.sel_bg),
        (true, false) => Style::default().bg(th.sel_bg_dim),
        _ => Style::default(),
    };

    let mut out = vec![if selected && focused {
        Span::styled(MARKER, Style::default().fg(th.accent).bg(th.sel_bg))
    } else {
        Span::styled(GUTTER, bar)
    }];
    out.extend(spans.into_iter().map(|s| {
        let mut style = s.style.patch(bar);
        if selected {
            if style.fg == Some(th.dim) {
                style = style.fg(th.muted);
            }
            if focused {
                style = style.add_modifier(Modifier::BOLD);
            }
        }
        Span::styled(s.content, style)
    }));

    f.render_widget(Paragraph::new(Line::from(out)).style(bar), area);
}

/// Bordered panel frame. Focus has to be unmissable at a glance, so the
/// focused panel gets an accent border and a filled accent title chip
/// against the unfocused panels' receding edge border and muted title.
fn panel_block(title: &str, focused: bool, th: Theme) -> Block<'_> {
    let (border, chip) = if focused {
        (
            Style::default().fg(th.accent),
            Style::default()
                .fg(th.on_accent)
                .bg(th.accent)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        (Style::default().fg(th.edge), Style::default().fg(th.muted))
    };
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border)
        .title(Span::styled(format!(" {title} "), chip))
}

/// Washes a focused panel's background with the accent tint, leaving cells
/// that already painted their own background (a selection bar) alone.
fn wash(buf: &mut Buffer, area: Rect, th: Theme) {
    for y in area.y..area.y.saturating_add(area.height) {
        for x in area.x..area.x.saturating_add(area.width) {
            if let Some(cell) = buf.cell_mut((x, y)) {
                if cell.bg == Color::Reset {
                    cell.bg = th.focus_tint;
                }
            }
        }
    }
}

fn row_rect(inner: Rect, i: usize) -> Option<Rect> {
    let y = inner.y.checked_add(u16::try_from(i).ok()?)?;
    (y < inner.y + inner.height).then(|| Rect::new(inner.x, y, inner.width, 1))
}

/// The rightmost column: the selected pane's live terminal, always drawn
/// alongside the other three rather than taking over the screen. Which pane
/// that is follows the panes column's selection.
fn render_content(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    if app.review.is_some() {
        render_review(f, app, area, th);
        return;
    }
    // Typing focus is what the accent border promises here, so only
    // PaneContent lights it up — merely selecting a pane does not.
    let focused = app.focus == Focus::PaneContent;
    let title = content_title(app);
    let block = panel_block(&title, focused, th);
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.current_pane().is_none() {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("nothing running here — ", Style::default().fg(th.dim)),
                Span::styled("s", Style::default().fg(th.accent)),
                Span::styled(" shell   ", Style::default().fg(th.dim)),
                Span::styled("a", Style::default().fg(th.accent)),
                Span::styled(" agent", Style::default().fg(th.dim)),
            ])),
            inner,
        );
    } else {
        f.render_widget(TermView { grid: &app.grid }, inner);
    }
    app.layout.content = inner;
}

/// Drawn in the column the live pane uses, so the nav columns stay put
/// (DESIGN.md §9 M4).
fn render_review(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    let focused = app.focus == Focus::Review;
    let title = match app.review.as_ref() {
        Some(v) => format!("review › {} changed", v.review.files.len()),
        None => "review".to_string(),
    };
    let block = panel_block(&title, focused, th);
    let inner = block.inner(area);
    f.render_widget(block, area);
    app.layout.content = inner;

    let Some(view) = app.review.as_mut() else { return };
    view.scroll_into_view(inner.height as usize);
    let (from, to) = view.selection();

    let lines: Vec<Line> = view
        .rows
        .iter()
        .enumerate()
        .skip(view.top)
        .take(inner.height as usize)
        .map(|(i, row)| review_line(view, *row, i >= from && i <= to, th))
        .collect();

    f.render_widget(Paragraph::new(lines), inner);
}

/// Four digits covers nearly every file; the code matters more than the rest.
const LINENO_WIDTH: usize = 4;

fn review_line<'a>(view: &'a ReviewView, row: Row, selected: bool, th: Theme) -> Line<'a> {
    let file = &view.review.files[row.file()];
    let pad = " ".repeat(LINENO_WIDTH + 1);

    let spans = match row {
        Row::File { .. } => {
            let mut v = vec![
                Span::styled(format!(" {} ", file.kind.marker()), Style::default().fg(th.on_accent).bg(th.accent)),
                Span::styled(format!(" {}", file.path), Style::default().fg(th.text).add_modifier(Modifier::BOLD)),
            ];
            if file.added_lines() + file.removed_lines() > 0 {
                v.push(Span::styled(format!("  +{}", file.added_lines()), Style::default().fg(th.ok)));
                v.push(Span::styled(format!(" -{}", file.removed_lines()), Style::default().fg(th.err)));
            }
            v
        }
        Row::Hunk { hunk, .. } => vec![Span::styled(
            format!("{pad}{}", file.hunks[hunk].header),
            Style::default().fg(th.muted),
        )],
        Row::Note { .. } => vec![Span::styled(
            format!("{pad}{}", file.note.as_deref().unwrap_or("")),
            Style::default().fg(th.dim).add_modifier(Modifier::ITALIC),
        )],
        Row::Line { hunk, line, .. } => {
            let l = &file.hunks[hunk].lines[line];
            // The old side's number only where there is no new one.
            let no = match l.new_lineno.or(l.old_lineno) {
                Some(n) => format!("{n:>LINENO_WIDTH$}"),
                None => " ".repeat(LINENO_WIDTH),
            };
            let fg = match l.kind {
                LineKind::Added => th.ok,
                LineKind::Removed => th.err,
                LineKind::Context => th.text,
            };
            vec![
                Span::styled(format!("{no} "), Style::default().fg(th.dim)),
                Span::styled(format!("{}{}", crate::review::marker(l.kind), l.text), Style::default().fg(fg)),
            ]
        }
    };

    // A wash rather than a marker column: the left edge is already spent,
    // and a range should read as one block.
    let mut line = Line::from(spans);
    if selected && row.is_line() {
        line = line.style(Style::default().bg(th.sel_bg));
    }
    line
}

/// `project / checkout / pane` for the live view's title, which doubles as
/// the breadcrumb telling you where in the tree the content came from.
fn content_title(app: &App) -> String {
    match (app.current_project(), app.current_checkout(), app.current_pane()) {
        (Some(p), Some(c), Some(pane)) => format!("{} › {} › {}", p.name, c.name, pane.title),
        (Some(p), Some(c), None) => format!("{} › {}", p.name, c.name),
        (Some(p), None, _) => p.name.clone(),
        _ => "live".to_string(),
    }
}

/// The status bar: where you are on the left, what you can press on the
/// right. Context-sensitive, because the same key means different things
/// inside a pane and in the nav columns.
fn render_status(f: &mut Frame, app: &App, area: Rect, th: Theme) {
    // `area` includes the blank padding row; the bar is its last row.
    let area = Rect {
        y: area.y + area.height.saturating_sub(1),
        height: area.height.min(1),
        ..area
    };

    let (hint, tone) = if app.picker.is_some() {
        ("j/k move   enter spawn   esc cancel", th.dim)
    } else if app.prompt.is_some() {
        ("type to edit   enter confirm   esc cancel", th.dim)
    } else if app.leader_pending {
        ("leader…   esc back to panes   x close pane", th.accent)
    } else if app.focus == Focus::Review {
        ("j/k move  ]/[ file  v range  c comment  e edit  r refresh  esc close", th.dim)
    } else if app.focus == Focus::PaneContent {
        ("typing — ctrl-space then esc to leave, x to close", th.dim)
    } else {
        (
            "j/k move  l/h in/out  s shell  a agent  n new  R review  w wksp  D rm  x close  q detach",
            th.dim,
        )
    };

    // A daemon error or a pane exit lands in `status`. That is the one
    // thing on this bar the user *must* read, so it outranks the keymap for
    // space — unlike the breadcrumb, which yields.
    let alert = app.status.starts_with("error:") || app.status.contains("exited");
    let left = if alert {
        Span::styled(app.status.clone(), Style::default().fg(th.err))
    } else {
        Span::styled(breadcrumb(app), Style::default().fg(th.muted))
    };

    let hint_span = Span::styled(hint, Style::default().fg(tone));

    // The keymap is what the user acts on, so it wins the space. If the
    // breadcrumb can't fit beside it with a real gap, drop the breadcrumb
    // rather than letting the two run together and truncate the keys.
    let hint_len = hint_span.content.chars().count();
    let left_len = left.content.chars().count();
    let width = area.width as usize;

    let mut spans = vec![Span::raw(" ")];
    if left_len + hint_len + 3 <= width {
        spans.push(left);
        spans.push(Span::raw(" ".repeat(width - left_len - hint_len - 2)));
        spans.push(hint_span);
    } else if alert {
        // Not enough room for both: the alert stays, the keymap goes. The
        // keys are discoverable elsewhere; a swallowed error is not.
        spans.push(left);
    } else {
        spans.push(Span::raw(" ".repeat(width.saturating_sub(hint_len + 2))));
        spans.push(hint_span);
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn breadcrumb(app: &App) -> String {
    match app.focus {
        Focus::Projects => "projects".to_string(),
        _ => content_title(app),
    }
}

fn render_picker(f: &mut Frame, app: &App, area: Rect, th: Theme) {
    let Some(picker) = &app.picker else { return };
    let height = (picker.items.len() as u16 + 2).min(area.height);
    let widest = picker.items.iter().map(|i| i.chars().count()).max().unwrap_or(0);
    let width = (widest as u16 + 6).clamp(24, 48).min(area.width);
    let popup = centered_rect(width, height, area);

    f.render_widget(Clear, popup);
    let block = panel_block(picker.title, true, th);
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    for (i, name) in picker.items.iter().enumerate() {
        let Some(row) = row_rect(inner, i) else { break };
        render_row(
            f,
            row,
            vec![Span::styled(name.clone(), Style::default().fg(th.text))],
            i == picker.sel,
            true,
            th,
        );
    }
}

/// The modal for all three prompts, drawn over everything else. Destructive
/// confirmations are tinted `err` so a removal never looks like a text
/// field you can dismiss by typing.
fn render_prompt(f: &mut Frame, app: &App, area: Rect, th: Theme) {
    let Some(prompt) = &app.prompt else { return };
    let (title, body, hint, danger) = match prompt {
        Prompt::NewWorktree { input, .. } => (
            "new worktree",
            field(input, th),
            "enter create   esc cancel",
            false,
        ),
        Prompt::AddProject { input } => (
            "add project",
            field(input, th),
            "enter add   esc cancel",
            false,
        ),
        Prompt::Comment { anchor, input } => (
            "comment to the agent",
            Line::from(vec![
                Span::styled(
                    anchor.message(""),
                    Style::default().fg(th.muted),
                ),
                Span::raw("  "),
                field(input, th).spans.remove(0),
            ]),
            "enter send   esc cancel",
            false,
        ),
        Prompt::ConfirmRemoveCheckout { label, .. } => (
            "remove checkout?",
            Line::from(vec![
                Span::styled(label.clone(), Style::default().fg(th.text).add_modifier(Modifier::BOLD)),
                Span::styled(
                    "  — worktree, branch, and its panes",
                    Style::default().fg(th.muted),
                ),
            ]),
            "y/enter remove   n/esc cancel",
            true,
        ),
    };

    let width = 54.min(area.width.saturating_sub(2));
    let height = 4.min(area.height);
    let popup = centered_rect(width, height, area);

    f.render_widget(Clear, popup);
    let accent = if danger { th.err } else { th.accent };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent))
        .title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(th.on_accent)
                .bg(accent)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    f.render_widget(
        Paragraph::new(vec![
            body,
            Line::from(Span::styled(hint, Style::default().fg(th.dim))),
        ]),
        inner,
    );
}

/// A text field with a visible caret. Empty fields show nothing but the
/// caret, so there is always something to look at.
fn field(input: &str, th: Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(input.to_string(), Style::default().fg(th.text)),
        Span::styled("▏", Style::default().fg(th.accent)),
    ])
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}

/// The compact git summary on a checkout row: branch name (or `detached`),
/// commits ahead/behind upstream, and a dirty marker with its file count.
/// Each part carries its own color, so `clean` and `*3` read differently at
/// a glance instead of being one undifferentiated string.
/// [`git_spans`] with the branch name elided when it merely repeats the
/// row's own label. The ahead/behind/dirty markers always survive — those
/// are never implied by the name.
fn git_spans_unless_branch_is(git: Option<&GitStatus>, name: &str, th: Theme) -> Vec<Span<'static>> {
    let redundant = git.and_then(|g| g.branch.as_deref()) == Some(name);
    git_spans(git, th)
        .into_iter()
        .filter(|s| !(redundant && s.content.trim() == name))
        .collect()
}

fn git_spans(git: Option<&GitStatus>, th: Theme) -> Vec<Span<'static>> {
    let Some(g) = git else { return Vec::new() };
    let mut spans = vec![match &g.branch {
        Some(branch) => Span::styled(format!("  {branch}"), Style::default().fg(th.muted)),
        None => Span::styled("  detached".to_string(), Style::default().fg(th.dim)),
    }];
    if g.ahead > 0 {
        spans.push(Span::styled(
            format!(" ↑{}", g.ahead),
            Style::default().fg(th.ok),
        ));
    }
    if g.behind > 0 {
        spans.push(Span::styled(
            format!(" ↓{}", g.behind),
            Style::default().fg(th.warn),
        ));
    }
    if g.dirty {
        spans.push(Span::styled(
            format!(" *{}", g.changed_files),
            Style::default().fg(th.warn),
        ));
    }
    spans
}

/// The exit code appended to a pane row, and only for a failure — a clean
/// exit is already said by the green dot, and repeating it as text would
/// make every finished pane shout.
fn exit_note(status: PaneStatus) -> String {
    match status {
        PaneStatus::Exited { code: Some(0) } => String::new(),
        PaneStatus::Exited { code: Some(c) } => format!("  exit {c}"),
        PaneStatus::Exited { code: None } => "  killed".to_string(),
        _ => String::new(),
    }
}

fn worst_pane_status(c: &orion_protocol::CheckoutInfo) -> Option<PaneStatus> {
    c.panes.iter().map(|p| p.status).max_by_key(rank)
}

/// Parents show the worst child (DESIGN.md §8b): `Waiting` outranks
/// everything else since it's blocked specifically on you, then a failed
/// exit, then the calm day-to-day states, then a clean exit last of all.
fn rank(status: &PaneStatus) -> u8 {
    match status {
        PaneStatus::Exited { code: Some(0) } => 0,
        PaneStatus::Idle | PaneStatus::Working => 1,
        PaneStatus::Exited { .. } => 2,
        PaneStatus::Waiting => 3,
    }
}

/// One dot carries the whole state signal (§8b). A hollow dot means there
/// is nothing running to have a state; a filled one is colored by it.
fn status_dot(status: Option<PaneStatus>, th: Theme) -> Span<'static> {
    let (glyph, color) = match status {
        None => ("○ ", th.dim),
        Some(PaneStatus::Idle) => ("● ", th.ok),
        Some(PaneStatus::Working) => ("● ", th.warn),
        Some(PaneStatus::Waiting) => ("● ", th.err),
        Some(PaneStatus::Exited { code: Some(0) }) => ("✓ ", th.dim),
        Some(PaneStatus::Exited { .. }) => ("✗ ", th.err),
    };
    Span::styled(glyph, Style::default().fg(color))
}

struct TermView<'a> {
    grid: &'a Option<Grid>,
}

impl Widget for TermView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let Some(grid) = self.grid else { return };
        for (r, row) in grid.cells.iter().enumerate() {
            if r as u16 >= area.height {
                break;
            }
            for (c, cell) in row.iter().enumerate() {
                if c as u16 >= area.width {
                    break;
                }
                let Some(target) = buf.cell_mut((area.x + c as u16, area.y + r as u16)) else {
                    continue;
                };
                target.set_symbol(&cell.ch);
                let mut style = Style::default()
                    .fg(to_ratatui_color(cell.fg))
                    .bg(to_ratatui_color(cell.bg));
                if cell.bold {
                    style = style.add_modifier(Modifier::BOLD);
                }
                if cell.italic {
                    style = style.add_modifier(Modifier::ITALIC);
                }
                if cell.underline {
                    style = style.add_modifier(Modifier::UNDERLINED);
                }
                if cell.reverse {
                    style = style.add_modifier(Modifier::REVERSED);
                }
                target.set_style(style);
            }
        }
    }
}

fn to_ratatui_color(c: PColor) -> Color {
    match c {
        PColor::Default => Color::Reset,
        PColor::Idx(i) => Color::Indexed(i),
        PColor::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orion_protocol::{CheckoutId, CheckoutInfo, PaneId, PaneInfo, PaneKind, ProjectId, ProjectInfo};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn git(branch: Option<&str>, dirty: bool, changed: usize, ahead: usize, behind: usize) -> GitStatus {
        GitStatus {
            branch: branch.map(str::to_string),
            dirty,
            changed_files: changed,
            ahead,
            behind,
        }
    }

    fn checkout_with(statuses: &[PaneStatus]) -> CheckoutInfo {
        CheckoutInfo {
            id: CheckoutId(1),
            name: "c".to_string(),
            path: "/c".to_string(),
            primary: true,
            git: None,
            panes: statuses
                .iter()
                .enumerate()
                .map(|(i, s)| PaneInfo {
                    id: PaneId(i as u64),
                    kind: PaneKind::Agent,
                    title: "t".to_string(),
                    status: *s,
                })
                .collect(),
        }
    }

    fn text_of(spans: &[Span]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    // --- git summary --------------------------------------------------------

    #[test]
    fn a_non_repo_checkout_shows_no_git_summary() {
        assert!(git_spans(None, Theme::default()).is_empty());
    }

    #[test]
    fn a_clean_branch_shows_only_its_name() {
        let s = git_spans(Some(&git(Some("master"), false, 0, 0, 0)), Theme::default());
        assert_eq!(text_of(&s).trim(), "master");
    }

    #[test]
    fn a_detached_head_says_so() {
        let s = git_spans(Some(&git(None, false, 0, 0, 0)), Theme::default());
        assert_eq!(text_of(&s).trim(), "detached");
    }

    #[test]
    fn ahead_behind_and_dirty_render_in_a_fixed_order() {
        let s = git_spans(Some(&git(Some("wt"), true, 5, 1, 2)), Theme::default());
        assert_eq!(text_of(&s).trim(), "wt ↑1 ↓2 *5");
    }

    #[test]
    fn zero_counts_are_omitted_rather_than_shown_as_zero() {
        let s = text_of(&git_spans(Some(&git(Some("m"), false, 0, 0, 0)), Theme::default()));
        assert!(!s.contains('↑') && !s.contains('↓') && !s.contains('*'), "{s:?}");
    }

    #[test]
    fn dirty_and_behind_share_the_warn_role_while_ahead_reads_as_ok() {
        // The colors carry meaning on their own; a reader shouldn't have to
        // parse the arrows to know whether something needs attention.
        let th = Theme::default();
        let s = git_spans(Some(&git(Some("m"), true, 1, 1, 1)), th);
        let color_of = |needle: &str| {
            s.iter()
                .find(|sp| sp.content.contains(needle))
                .unwrap_or_else(|| panic!("no span containing {needle:?}"))
                .style
                .fg
        };
        assert_eq!(color_of("↑"), Some(th.ok));
        assert_eq!(color_of("↓"), Some(th.warn));
        assert_eq!(color_of("*"), Some(th.warn));
    }

    // --- rolled-up status ---------------------------------------------------

    #[test]
    fn a_parent_with_no_panes_has_no_status() {
        assert_eq!(worst_pane_status(&checkout_with(&[])), None);
    }

    #[test]
    fn waiting_outranks_everything_because_it_is_blocked_on_you() {
        let c = checkout_with(&[
            PaneStatus::Working,
            PaneStatus::Waiting,
            PaneStatus::Exited { code: Some(1) },
        ]);
        assert_eq!(worst_pane_status(&c), Some(PaneStatus::Waiting));
    }

    #[test]
    fn a_failed_exit_outranks_the_calm_states() {
        let c = checkout_with(&[PaneStatus::Idle, PaneStatus::Exited { code: Some(1) }]);
        assert_eq!(worst_pane_status(&c), Some(PaneStatus::Exited { code: Some(1) }));
    }

    #[test]
    fn a_clean_exit_ranks_below_a_live_pane() {
        let c = checkout_with(&[PaneStatus::Exited { code: Some(0) }, PaneStatus::Idle]);
        assert_eq!(worst_pane_status(&c), Some(PaneStatus::Idle));
    }

    #[test]
    fn a_kill_with_no_exit_code_counts_as_a_failure() {
        assert_eq!(
            rank(&PaneStatus::Exited { code: None }),
            rank(&PaneStatus::Exited { code: Some(1) })
        );
    }

    // --- the status dot -----------------------------------------------------

    #[test]
    fn nothing_running_is_a_hollow_dot_and_anything_running_is_filled() {
        let th = Theme::default();
        assert_eq!(status_dot(None, th).content.trim(), "○");
        for s in [PaneStatus::Idle, PaneStatus::Working, PaneStatus::Waiting] {
            assert_eq!(status_dot(Some(s), th).content.trim(), "●", "for {s:?}");
        }
    }

    #[test]
    fn each_live_state_gets_its_own_color() {
        let th = Theme::default();
        assert_eq!(status_dot(Some(PaneStatus::Idle), th).style.fg, Some(th.ok));
        assert_eq!(status_dot(Some(PaneStatus::Working), th).style.fg, Some(th.warn));
        assert_eq!(status_dot(Some(PaneStatus::Waiting), th).style.fg, Some(th.err));
    }

    #[test]
    fn exits_are_a_tick_or_a_cross_not_a_dot() {
        let th = Theme::default();
        let clean = status_dot(Some(PaneStatus::Exited { code: Some(0) }), th);
        let failed = status_dot(Some(PaneStatus::Exited { code: Some(1) }), th);
        assert_eq!(clean.content.trim(), "✓");
        assert_eq!(failed.content.trim(), "✗");
        assert_eq!(failed.style.fg, Some(th.err), "a failure must be loud");
        assert_eq!(clean.style.fg, Some(th.dim), "a clean exit must not be");
    }

    #[test]
    fn only_a_failing_exit_gets_spelled_out_in_words() {
        assert_eq!(exit_note(PaneStatus::Exited { code: Some(0) }), "");
        assert_eq!(exit_note(PaneStatus::Idle), "");
        assert!(exit_note(PaneStatus::Exited { code: Some(2) }).contains("exit 2"));
        assert!(exit_note(PaneStatus::Exited { code: None }).contains("killed"));
    }

    // --- layout helpers -----------------------------------------------------

    #[test]
    fn a_modal_is_centered_in_its_area() {
        let r = centered_rect(10, 4, Rect::new(0, 0, 30, 20));
        assert_eq!((r.x, r.y, r.width, r.height), (10, 8, 10, 4));
    }

    #[test]
    fn a_modal_larger_than_the_screen_is_pinned_not_wrapped_negative() {
        let r = centered_rect(50, 40, Rect::new(0, 0, 30, 20));
        assert_eq!((r.x, r.y), (0, 0), "saturating, never underflowing");
    }

    #[test]
    fn rows_stack_down_the_panel_and_stop_at_its_bottom() {
        let inner = Rect::new(1, 1, 10, 3);
        assert_eq!(row_rect(inner, 0).unwrap().y, 1);
        assert_eq!(row_rect(inner, 2).unwrap().y, 3);
        assert!(row_rect(inner, 3).is_none(), "must not draw past the border");
    }

    // --- rendering the whole frame -----------------------------------------

    fn tree() -> Vec<ProjectInfo> {
        vec![ProjectInfo {
            id: ProjectId(1),
            name: "orion".to_string(),
            checkouts: vec![
                CheckoutInfo {
                    id: CheckoutId(10),
                    name: "master".to_string(),
                    path: "/repo".to_string(),
                    primary: true,
                    git: Some(git(Some("master"), true, 2, 0, 0)),
                    panes: vec![
                        PaneInfo {
                            id: PaneId(100),
                            kind: PaneKind::Agent,
                            title: "claude".to_string(),
                            status: PaneStatus::Working,
                        },
                        PaneInfo {
                            id: PaneId(101),
                            kind: PaneKind::Shell,
                            title: "shell".to_string(),
                            status: PaneStatus::Idle,
                        },
                    ],
                },
                CheckoutInfo {
                    id: CheckoutId(11),
                    name: "feat".to_string(),
                    path: "/repo/wt".to_string(),
                    primary: false,
                    git: None,
                    panes: vec![],
                },
            ],
        }]
    }

    /// Renders a real frame through ratatui's test backend and hands back
    /// the buffer, so the UI can be asserted on without a terminal.
    fn draw(app: &mut App) -> ratatui::buffer::Buffer {
        draw_at(app, 100, 20)
    }

    fn draw_at(app: &mut App, w: u16, h: u16) -> ratatui::buffer::Buffer {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| render(f, app)).unwrap();
        terminal.backend().buffer().clone()
    }

    fn lines(buf: &ratatui::buffer::Buffer) -> Vec<String> {
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "))
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    fn app_with_tree() -> App {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        // Keep the receiver alive so sends don't fail during render setup.
        std::mem::forget(rx);
        let mut app = App::new(tx);
        app.on_server_msg(orion_protocol::ServerMsg::Tree(tree()));
        app
    }

    #[test]
    fn all_four_columns_are_drawn_at_once() {
        // The core navigational promise (§4): descending never replaces the
        // tree with a full-screen view.
        let mut app = app_with_tree();
        app.focus = Focus::PaneContent;
        let text = lines(&draw(&mut app)).join("\n");
        for title in ["projects", "checkouts", "panes"] {
            assert!(text.contains(title), "{title} column missing while inside a pane");
        }
    }

    #[test]
    fn the_tree_contents_actually_reach_the_screen() {
        let mut app = app_with_tree();
        let text = lines(&draw(&mut app)).join("\n");
        assert!(text.contains("orion"), "project name");
        assert!(text.contains("master"), "checkout name");
        assert!(text.contains("claude"), "pane title");
    }

    #[test]
    fn the_focused_column_alone_gets_the_accent_border() {
        let th = Theme::default();
        let mut app = app_with_tree();
        app.focus = Focus::Checkouts;
        let buf = draw(&mut app);

        // Top-left corner cell of each column's border.
        let corner = |r: Rect| buf.cell((r.x.saturating_sub(1), r.y.saturating_sub(1))).unwrap().fg;
        assert_eq!(corner(app.layout.checkouts), th.accent, "focused column");
        assert_eq!(corner(app.layout.projects), th.edge, "unfocused column");
    }

    #[test]
    fn the_focused_column_is_washed_and_the_others_are_not() {
        let th = Theme::default();
        let mut app = app_with_tree();
        app.focus = Focus::Projects;
        let buf = draw(&mut app);

        let inner = app.layout.projects;
        // A blank cell inside the focused panel, below the last row.
        let bg = buf.cell((inner.x, inner.y + inner.height - 1)).unwrap().bg;
        assert_eq!(bg, th.focus_tint, "focused panel should be washed");

        let other = app.layout.checkouts;
        let other_bg = buf.cell((other.x, other.y + other.height - 1)).unwrap().bg;
        assert_eq!(other_bg, Color::Reset, "unfocused panel stays plain");
    }

    #[test]
    fn the_selected_row_is_marked_and_raised_never_reversed() {
        let th = Theme::default();
        let mut app = app_with_tree();
        app.focus = Focus::Checkouts;
        app.sel_checkout = 1;
        let buf = draw(&mut app);

        let inner = app.layout.checkouts;
        let marker = buf.cell((inner.x, inner.y + 1)).unwrap();
        assert_eq!(marker.symbol(), MARKER, "selection marker on the selected row");
        assert_eq!(marker.fg, th.accent);
        assert_eq!(marker.bg, th.sel_bg);

        let unselected = buf.cell((inner.x, inner.y)).unwrap();
        assert_eq!(unselected.symbol(), GUTTER, "other rows keep an aligned gutter");
        assert!(
            !unselected.modifier.contains(Modifier::REVERSED),
            "reverse video would fight the status colors"
        );
    }

    #[test]
    fn an_unfocused_columns_selection_is_still_visible_but_quieter() {
        let th = Theme::default();
        let mut app = app_with_tree();
        app.focus = Focus::Projects;
        app.sel_checkout = 0;
        let buf = draw(&mut app);

        let inner = app.layout.checkouts;
        let cell = buf.cell((inner.x + 1, inner.y)).unwrap();
        assert_eq!(cell.bg, th.sel_bg_dim, "you should still see where you were");
    }

    #[test]
    fn an_empty_column_explains_itself_instead_of_going_blank() {
        let mut app = app_with_tree();
        app.sel_checkout = 1; // the worktree with no panes
        app.focus = Focus::Panes;
        let text = lines(&draw(&mut app)).join("\n");
        assert!(text.contains("s: shell"), "an empty panes column should say what to press");
    }

    #[test]
    fn the_live_view_titles_itself_with_the_path_through_the_tree() {
        let mut app = app_with_tree();
        let text = lines(&draw(&mut app)).join("\n");
        assert!(
            text.contains("orion › master › claude"),
            "the live view should say where its content came from:\n{text}"
        );
    }

    #[test]
    fn the_status_bar_shows_the_keymap_and_swaps_it_inside_a_pane() {
        let mut app = app_with_tree();
        let nav = lines(&draw(&mut app)).join("\n");
        assert!(nav.contains("q detach"), "nav keymap");

        app.focus = Focus::PaneContent;
        let typing = lines(&draw(&mut app)).join("\n");
        assert!(typing.contains("typing"), "a pane's keys are the child's, and it should say so");
        assert!(!typing.contains("q detach"), "the nav keymap would be a lie here");
    }

    #[test]
    fn a_pending_leader_chord_is_announced() {
        let mut app = app_with_tree();
        app.focus = Focus::PaneContent;
        app.leader_pending = true;
        let text = lines(&draw(&mut app)).join("\n");
        assert!(text.contains("leader"), "a half-entered chord must be visible");
    }

    #[test]
    fn an_error_takes_over_the_status_bar_from_the_breadcrumb() {
        let mut app = app_with_tree();
        app.on_server_msg(orion_protocol::ServerMsg::Error {
            message: "git worktree add failed".to_string(),
        });
        let text = lines(&draw(&mut app)).join("\n");
        assert!(text.contains("git worktree add failed"), "errors must be read, not buried");
    }

    #[test]
    fn a_destructive_prompt_is_colored_as_a_warning() {
        let th = Theme::default();
        let mut app = app_with_tree();
        app.focus = Focus::Checkouts;
        app.sel_checkout = 1;
        app.on_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('D'),
            crossterm::event::KeyModifiers::NONE,
        ));
        let buf = draw(&mut app);
        let text = lines(&buf).join("\n");
        assert!(text.contains("remove checkout?"));
        assert!(
            (0..buf.area.height).any(|y| (0..buf.area.width).any(|x| buf.cell((x, y)).unwrap().fg == th.err)),
            "a removal must not look like an ordinary text field"
        );
    }

    #[test]
    fn a_prompt_draws_over_the_columns_rather_than_beside_them() {
        let mut app = app_with_tree();
        app.on_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('n'),
            crossterm::event::KeyModifiers::NONE,
        ));
        let text = lines(&draw(&mut app)).join("\n");
        assert!(text.contains("add project"));
        assert!(text.contains("enter add"), "a prompt should say how to commit it");
    }

    #[test]
    fn the_agent_picker_lists_templates_over_the_tree() {
        let mut app = app_with_tree();
        app.templates = vec!["claude".to_string(), "codex".to_string()];
        app.on_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('a'),
            crossterm::event::KeyModifiers::NONE,
        ));
        let text = lines(&draw(&mut app)).join("\n");
        assert!(text.contains("spawn agent"));
        assert!(text.contains("codex"));
    }

    #[test]
    fn a_narrow_terminal_still_renders_without_panicking() {
        // Panics here take the whole TUI down, and terminals get resized to
        // silly sizes all the time.
        let mut app = app_with_tree();
        for (w, h) in [(20, 6), (10, 4), (4, 3), (1, 1)] {
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            terminal.draw(|f| render(f, &mut app)).unwrap();
        }
    }

    /// Not an assertion — a way to look at the UI while working on it:
    /// `cargo test -p orion dump_frame -- --ignored --nocapture`. Beats
    /// launching the client into a real terminal just to see the layout.
    #[test]
    #[ignore = "prints a frame for eyeballing; asserts nothing"]
    fn dump_frame() {
        let mut app = app_with_tree();
        app.focus = Focus::Checkouts;
        app.templates = vec!["claude".to_string()];
        let w = std::env::var("DUMP_W").ok().and_then(|v| v.parse().ok()).unwrap_or(100);
        let h = std::env::var("DUMP_H").ok().and_then(|v| v.parse().ok()).unwrap_or(20);
        for line in lines(&draw_at(&mut app, w, h)) {
            println!("|{line}");
        }
    }

    #[test]
    fn an_empty_tree_renders_the_add_project_hint() {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        std::mem::forget(rx);
        let mut app = App::new(tx);
        // The hint wraps across the narrow column, so assert on its words
        // rather than on a contiguous phrase.
        let text = lines(&draw(&mut app)).join("\n");
        assert!(text.contains("no projects"), "a first run should say the tree is empty");
        assert!(text.contains("add"), "and how to start:\n{text}");
    }
    // --- review viewer ------------------------------------------------------

    fn app_with_review() -> App {
        let mut app = app_with_tree();
        app.review = Some(crate::review::ReviewView::new(orion_protocol::Review {
            checkout: CheckoutId(1),
            files: vec![orion_protocol::FileDiff {
                path: "src/thing.rs".to_string(),
                old_path: None,
                kind: orion_protocol::ChangeKind::Modified,
                hunks: vec![orion_protocol::Hunk {
                    header: "@@ -10,3 +10,3 @@ fn f()".to_string(),
                    lines: vec![
                        orion_protocol::DiffLine {
                            kind: orion_protocol::LineKind::Context,
                            old_lineno: Some(10),
                            new_lineno: Some(10),
                            text: "unchanged".to_string(),
                        },
                        orion_protocol::DiffLine {
                            kind: orion_protocol::LineKind::Removed,
                            old_lineno: Some(11),
                            new_lineno: None,
                            text: "gone".to_string(),
                        },
                        orion_protocol::DiffLine {
                            kind: orion_protocol::LineKind::Added,
                            old_lineno: None,
                            new_lineno: Some(11),
                            text: "arrived".to_string(),
                        },
                    ],
                }],
                note: None,
            }],
        }));
        app.focus = Focus::Review;
        app
    }

    #[test]
    fn the_review_never_hides_the_nav_columns() {
        // The standing rule for every view Orion has: one thing opening
        // must not take the others off screen.
        let mut app = app_with_review();
        let out = lines(&draw(&mut app)).join("\n");
        assert!(out.contains("orion"), "the project is still listed:\n{out}");
        assert!(out.contains("src/thing.rs"), "and the diff is up too:\n{out}");
    }

    #[test]
    fn a_file_header_shows_its_marker_path_and_line_counts() {
        let mut app = app_with_review();
        let out = lines(&draw(&mut app)).join("\n");
        assert!(out.contains("src/thing.rs"), "{out}");
        assert!(out.contains("+1"), "one line added:\n{out}");
        assert!(out.contains("-1"), "one line removed:\n{out}");
    }

    #[test]
    fn diff_lines_keep_gits_markers_and_line_numbers() {
        let mut app = app_with_review();
        let out = lines(&draw(&mut app)).join("\n");
        assert!(out.contains("+arrived"), "{out}");
        assert!(out.contains("-gone"), "{out}");
        assert!(out.contains("@@ -10,3 +10,3 @@"), "the hunk header too:\n{out}");
    }

    #[test]
    fn added_and_removed_lines_are_told_apart_by_color() {
        // The markers alone are one character wide; color is what makes a
        // diff scannable.
        let mut app = app_with_review();
        let buf = draw(&mut app);
        let th = app.theme;
        assert_eq!(fg_of(&buf, "+arrived"), Some(th.ok));
        assert_eq!(fg_of(&buf, "-gone"), Some(th.err));
    }

    #[test]
    fn the_selected_line_is_washed_so_a_range_reads_as_one_block() {
        let mut app = app_with_review();
        app.review.as_mut().unwrap().toggle_mark();
        app.review.as_mut().unwrap().move_by(1);
        let buf = draw(&mut app);
        assert_eq!(bg_of(&buf, "unchanged"), Some(app.theme.sel_bg));
        assert_eq!(bg_of(&buf, "-gone"), Some(app.theme.sel_bg), "the whole range");
        assert_ne!(bg_of(&buf, "+arrived"), Some(app.theme.sel_bg), "but no further");
    }

    #[test]
    fn a_diff_taller_than_the_column_scrolls_to_keep_the_cursor_visible() {
        let mut app = app_with_review();
        app.review.as_mut().unwrap().bottom_of_diff();
        let out = lines(&draw_at(&mut app, 100, 10)).join("\n");
        assert!(out.contains("+arrived"), "the cursor's line is on screen:\n{out}");
    }

    fn fg_of(buf: &ratatui::buffer::Buffer, needle: &str) -> Option<Color> {
        cell_at(buf, needle).map(|c| c.fg)
    }

    fn bg_of(buf: &ratatui::buffer::Buffer, needle: &str) -> Option<Color> {
        cell_at(buf, needle).map(|c| c.bg)
    }

    fn cell_at<'a>(buf: &'a ratatui::buffer::Buffer, needle: &str) -> Option<&'a ratatui::buffer::Cell> {
        // Cell-wise: multi-byte glyphs make byte offsets lie.
        let needle: Vec<&str> = needle.split("").filter(|s| !s.is_empty()).collect();
        for y in 0..buf.area.height {
            let row: Vec<&str> = (0..buf.area.width)
                .map(|x| buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "))
                .collect();
            if let Some(x) = row.windows(needle.len()).position(|w| w == needle) {
                return buf.cell((x as u16, y));
            }
        }
        None
    }

    #[test]
    fn the_status_bar_offers_the_reviews_own_keys_while_it_is_up() {
        let mut app = app_with_review();
        let out = lines(&draw(&mut app)).join("\n");
        assert!(out.contains("c comment"), "{out}");
        assert!(!out.contains("s shell"), "not the tree keymap:\n{out}");
    }

    #[test]
    fn the_tree_keymap_advertises_review() {
        let mut app = app_with_tree();
        let out = lines(&draw(&mut app)).join("\n");
        assert!(out.contains("R review"), "{out}");
    }

}
