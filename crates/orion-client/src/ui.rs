use orion_protocol::{Color as PColor, GitStatus, PaneStatus};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Widget};
use ratatui::Frame;

use crate::app::{App, Focus, Prompt};
use crate::grid::Grid;

pub fn render(f: &mut Frame, app: &mut App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(f.area());

    render_columns(f, app, root[0]);
    render_status(f, app, root[1]);

    if app.picker.is_some() {
        render_picker(f, app, f.area());
    }
    if app.prompt.is_some() {
        render_prompt(f, app, f.area());
    }
}

/// Always draws all four columns side by side — projects, checkouts, open
/// agents/shells, and the live pane view — so an agent's output stays
/// visible next to the rest of the tree instead of taking over the screen.
fn render_columns(f: &mut Frame, app: &mut App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(16),
            Constraint::Percentage(20),
            Constraint::Percentage(22),
            Constraint::Percentage(42),
        ])
        .split(area);

    let project_items: Vec<ListItem> = app
        .tree
        .iter()
        .map(|p| {
            let agents: usize = p.checkouts.iter().map(|c| c.panes.len()).sum();
            ListItem::new(format!("{}  {} checkouts, {} panes", p.name, p.checkouts.len(), agents))
        })
        .collect();
    app.layout.projects = render_column(
        f,
        cols[0],
        "projects",
        project_items,
        app.focus == Focus::Projects,
        Some(app.sel_project).filter(|_| !app.tree.is_empty()),
    );

    let checkout_items: Vec<ListItem> = app
        .current_project()
        .map(|p| {
            p.checkouts
                .iter()
                .map(|c| {
                    let status = worst_pane_status(c);
                    let line = format!(
                        "{}{} {}{}  {}p",
                        status_glyph(status),
                        if c.primary { "⌂" } else { "⧉" },
                        c.name,
                        git_suffix(c.git.as_ref()),
                        c.panes.len()
                    );
                    ListItem::new(line).style(status_style(status))
                })
                .collect()
        })
        .unwrap_or_default();
    let ncheck = app.current_project().map(|p| p.checkouts.len()).unwrap_or(0);
    app.layout.checkouts = render_column(
        f,
        cols[1],
        "checkouts",
        checkout_items,
        app.focus == Focus::Checkouts,
        Some(app.sel_checkout).filter(|_| ncheck > 0),
    );

    let pane_items: Vec<ListItem> = app
        .current_checkout()
        .map(|c| {
            c.panes
                .iter()
                .map(|p| {
                    ListItem::new(format!("{} {} #{}", status_glyph(Some(p.status)), p.title, p.id.0))
                        .style(status_style(Some(p.status)))
                })
                .collect()
        })
        .unwrap_or_default();
    let npane = app.current_checkout().map(|c| c.panes.len()).unwrap_or(0);
    app.layout.panes = render_column(
        f,
        cols[2],
        "open agents",
        pane_items,
        app.focus == Focus::Panes,
        Some(app.sel_pane).filter(|_| npane > 0),
    );

    render_content(f, app, cols[3]);
}

/// Renders a bordered column and returns its inner (post-border) area, so
/// the caller can hit-test mouse clicks against the same rows drawn here.
fn render_column(
    f: &mut Frame,
    area: Rect,
    title: &str,
    items: Vec<ListItem>,
    active: bool,
    selected: Option<usize>,
) -> Rect {
    let border_style = if active {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let items: Vec<ListItem> = items
        .into_iter()
        .enumerate()
        .map(|(i, item)| {
            if Some(i) == selected {
                item.style(Style::default().add_modifier(Modifier::REVERSED))
            } else {
                item
            }
        })
        .collect();
    f.render_widget(List::new(items), inner);
    inner
}

/// The rightmost column: the selected pane's live terminal content, always
/// rendered alongside the other three columns rather than taking over the
/// screen. Which pane that is follows the "open agents" column's selection.
fn render_content(f: &mut Frame, app: &mut App, area: Rect) {
    let active = matches!(app.focus, Focus::Panes | Focus::PaneContent);
    let border_style = if active {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let title = match (app.current_project(), app.current_checkout(), app.current_pane()) {
        (Some(p), Some(c), Some(pane)) => format!(" {} / {} / {} #{} ", p.name, c.name, pane.title, pane.id.0),
        _ => " agent ".to_string(),
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.current_pane().is_none() {
        f.render_widget(
            Paragraph::new("no pane selected — s: shell   a: agent").style(Style::default().fg(Color::DarkGray)),
            inner,
        );
    } else {
        f.render_widget(TermView { grid: &app.grid }, inner);
    }
    app.layout.content = inner;
}

fn render_status(f: &mut Frame, app: &App, area: Rect) {
    let hint = if app.picker.is_some() {
        "j/k move  enter: spawn  esc: cancel".to_string()
    } else if app.focus == Focus::PaneContent {
        if app.leader_pending {
            "leader…  esc: back to panes  x: close pane".to_string()
        } else {
            "ctrl-space then esc: back to panes, x: close".to_string()
        }
    } else {
        app.status.clone()
    };
    f.render_widget(Paragraph::new(Line::from(Span::raw(hint))), area);
}

fn render_picker(f: &mut Frame, app: &App, area: Rect) {
    let Some(picker) = &app.picker else { return };
    let height = (picker.items.len() as u16 + 2).min(area.height);
    let width = 30.min(area.width);
    let popup = centered_rect(width, height, area);

    f.render_widget(Clear, popup);
    let block = Block::default()
        .title(" spawn agent ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let items: Vec<ListItem> = picker
        .items
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let item = ListItem::new(name.as_str());
            if i == picker.sel {
                item.style(Style::default().add_modifier(Modifier::REVERSED))
            } else {
                item
            }
        })
        .collect();
    f.render_widget(List::new(items), inner);
}

/// The modal for `Prompt::NewWorktree` (a text field) and
/// `Prompt::ConfirmRemoveCheckout` (a yes/no), drawn over everything else
/// the same way `render_picker` is.
fn render_prompt(f: &mut Frame, app: &App, area: Rect) {
    let Some(prompt) = &app.prompt else { return };
    let (title, lines): (&str, Vec<Line>) = match prompt {
        Prompt::NewWorktree { input, .. } => (
            " new worktree — branch name ",
            vec![
                Line::from(Span::raw(format!("{input}▏"))),
                Line::from(Span::styled(
                    "enter: create   esc: cancel",
                    Style::default().fg(Color::DarkGray),
                )),
            ],
        ),
        Prompt::ConfirmRemoveCheckout { label, .. } => (
            " remove checkout? ",
            vec![
                Line::from(Span::raw(format!("{label}  (worktree, branch, and its panes)"))),
                Line::from(Span::styled(
                    "y/enter: remove   n/esc: cancel",
                    Style::default().fg(Color::DarkGray),
                )),
            ],
        ),
        Prompt::AddProject { input } => (
            " add project — directory path ",
            vec![
                Line::from(Span::raw(format!("{input}▏"))),
                Line::from(Span::styled(
                    "enter: add   esc: cancel",
                    Style::default().fg(Color::DarkGray),
                )),
            ],
        ),
    };

    let width = 50.min(area.width.saturating_sub(2));
    let height = 4.min(area.height);
    let popup = centered_rect(width, height, area);

    f.render_widget(Clear, popup);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    f.render_widget(Paragraph::new(lines), inner);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}

/// Compact " branch ↑2 ↓1 *3" suffix appended to a checkout row: branch name
/// (or "(detached)"), commits ahead/behind the upstream, and a dirty marker
/// with a changed-file count. Empty when the checkout isn't a git repo.
fn git_suffix(git: Option<&GitStatus>) -> String {
    let Some(g) = git else { return String::new() };
    let mut s = String::new();
    match &g.branch {
        Some(branch) => {
            s.push(' ');
            s.push_str(branch);
        }
        None => s.push_str(" (detached)"),
    }
    if g.ahead > 0 {
        s.push_str(&format!(" ↑{}", g.ahead));
    }
    if g.behind > 0 {
        s.push_str(&format!(" ↓{}", g.behind));
    }
    if g.dirty {
        s.push_str(&format!(" *{}", g.changed_files));
    }
    s
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

fn status_glyph(status: Option<PaneStatus>) -> &'static str {
    match status {
        None => "·",
        Some(PaneStatus::Idle) => "·",
        Some(PaneStatus::Working) => "◐",
        Some(PaneStatus::Waiting) => "?",
        Some(PaneStatus::Exited { code: Some(0) }) => "✓",
        Some(PaneStatus::Exited { .. }) => "✗",
    }
}

fn status_style(status: Option<PaneStatus>) -> Style {
    match status {
        None | Some(PaneStatus::Idle) => Style::default().fg(Color::DarkGray),
        Some(PaneStatus::Working) => Style::default().fg(Color::Blue),
        Some(PaneStatus::Waiting) => Style::default().fg(Color::Yellow),
        Some(PaneStatus::Exited { code: Some(0) }) => Style::default().fg(Color::Green),
        Some(PaneStatus::Exited { .. }) => Style::default().fg(Color::Red),
    }
}

struct TermView<'a> {
    grid: &'a Option<Grid>,
}

impl<'a> Widget for TermView<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let Some(grid) = self.grid else { return };
        for (row_idx, row) in grid.cells.iter().enumerate() {
            if row_idx as u16 >= area.height {
                break;
            }
            for (col_idx, cell) in row.iter().enumerate() {
                if col_idx as u16 >= area.width {
                    break;
                }
                let x = area.x + col_idx as u16;
                let y = area.y + row_idx as u16;
                let bc = &mut buf[(x, y)];
                bc.set_symbol(&cell.ch);
                bc.fg = to_ratatui_color(cell.fg);
                bc.bg = to_ratatui_color(cell.bg);
                let mut modifier = Modifier::empty();
                if cell.bold {
                    modifier |= Modifier::BOLD;
                }
                if cell.italic {
                    modifier |= Modifier::ITALIC;
                }
                if cell.underline {
                    modifier |= Modifier::UNDERLINED;
                }
                if cell.reverse {
                    modifier |= Modifier::REVERSED;
                }
                bc.modifier = modifier;
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
    use orion_protocol::{CheckoutId, PaneId, PaneInfo, PaneKind};

    fn git(branch: Option<&str>, dirty: bool, changed: usize, ahead: usize, behind: usize) -> GitStatus {
        GitStatus {
            branch: branch.map(str::to_string),
            dirty,
            changed_files: changed,
            ahead,
            behind,
        }
    }

    fn checkout_with(statuses: &[PaneStatus]) -> orion_protocol::CheckoutInfo {
        orion_protocol::CheckoutInfo {
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

    // --- git suffix ---------------------------------------------------------

    #[test]
    fn a_non_repo_checkout_gets_no_suffix() {
        assert_eq!(git_suffix(None), "");
    }

    #[test]
    fn a_clean_branch_shows_only_its_name() {
        assert_eq!(git_suffix(Some(&git(Some("master"), false, 0, 0, 0))), " master");
    }

    #[test]
    fn a_detached_head_says_so() {
        assert_eq!(git_suffix(Some(&git(None, false, 0, 0, 0))), " (detached)");
    }

    #[test]
    fn dirty_shows_the_changed_file_count() {
        assert_eq!(git_suffix(Some(&git(Some("main"), true, 3, 0, 0))), " main *3");
    }

    #[test]
    fn ahead_and_behind_render_in_that_order() {
        assert_eq!(git_suffix(Some(&git(Some("main"), false, 0, 2, 1))), " main ↑2 ↓1");
    }

    #[test]
    fn zero_counts_are_omitted_rather_than_shown_as_zero() {
        let s = git_suffix(Some(&git(Some("main"), false, 0, 0, 0)));
        assert!(!s.contains('↑') && !s.contains('↓') && !s.contains('*'), "{s:?}");
    }

    #[test]
    fn everything_at_once_reads_in_a_fixed_order() {
        assert_eq!(git_suffix(Some(&git(Some("wt"), true, 5, 1, 2))), " wt ↑1 ↓2 *5");
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
        assert_eq!(rank(&PaneStatus::Exited { code: None }), rank(&PaneStatus::Exited { code: Some(1) }));
    }

    #[test]
    fn the_rank_order_is_the_documented_one() {
        let mut all = vec![
            PaneStatus::Waiting,
            PaneStatus::Exited { code: Some(0) },
            PaneStatus::Idle,
            PaneStatus::Exited { code: Some(2) },
        ];
        all.sort_by_key(rank);
        assert_eq!(
            all,
            vec![
                PaneStatus::Exited { code: Some(0) },
                PaneStatus::Idle,
                PaneStatus::Exited { code: Some(2) },
                PaneStatus::Waiting,
            ]
        );
    }

    // --- glyphs -------------------------------------------------------------

    #[test]
    fn every_status_has_a_distinct_glyph_except_the_two_quiet_ones() {
        assert_eq!(status_glyph(Some(PaneStatus::Working)), "◐");
        assert_eq!(status_glyph(Some(PaneStatus::Waiting)), "?");
        assert_eq!(status_glyph(Some(PaneStatus::Exited { code: Some(0) })), "✓");
        assert_eq!(status_glyph(Some(PaneStatus::Exited { code: Some(1) })), "✗");
        assert_eq!(status_glyph(Some(PaneStatus::Exited { code: None })), "✗");
        // "no panes" and "idle" deliberately look the same: both are quiet.
        assert_eq!(status_glyph(None), status_glyph(Some(PaneStatus::Idle)));
    }

    #[test]
    fn glyph_and_style_agree_on_which_states_are_alarming() {
        for (status, expected) in [
            (Some(PaneStatus::Waiting), Color::Yellow),
            (Some(PaneStatus::Working), Color::Blue),
            (Some(PaneStatus::Exited { code: Some(0) }), Color::Green),
            (Some(PaneStatus::Exited { code: Some(1) }), Color::Red),
            (Some(PaneStatus::Idle), Color::DarkGray),
            (None, Color::DarkGray),
        ] {
            assert_eq!(status_style(status).fg, Some(expected), "for {status:?}");
        }
    }

    // --- layout -------------------------------------------------------------

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
}
