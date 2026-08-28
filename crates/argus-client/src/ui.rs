//! The renderer. Its normal spine has five columns: projects, repositories,
//! checkouts, open panes, and the selected pane's live view. A pane can
//! temporarily take the main content area while its terminal has focus.
//!
//! Every color goes through [`crate::theme::Theme`] rather than being named
//! here, and the visual language is deliberately narrow:
//!
//! - **Elevation** carries structure: the page sits at `bg`, an unfocused
//!   panel at `surface`, the focused one at `surface_focus`. Panels are
//!   padded and separated by a gutter, so they read as cards rather than
//!   as boxes drawn in a terminal.
//! - **Focus** is that elevation plus an accent border and title.
//! - **Selection** is a raised bar with an accent `▌` marker, never reverse
//!   video — reverse fights with the per-row status colors.
//! - **State** is a shape-distinct glyph in the row's status color (§8b),
//!   rolled up to parents by the worst descendant.
//! - **Rows are two lines**: what the thing is, then a dimmer line of what
//!   is true about it. Packing both onto one line is what made the old
//!   layout feel cramped.

use argus_protocol::{
    ChildAgentInfo, Color as PColor, FileDiff, GitStatus, HighlightKind, HighlightSpan, LineKind,
    PaneStatus,
};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph, Widget, Wrap};
use ratatui::Frame;

use crate::app::{App, CheckoutRow, Focus, Overlay, Panel, PickerKind, Prompt, Setting};
use crate::dirpicker::DirRow;
use crate::grid::Grid;
use crate::history::{HistoryRow, HistoryView};
use crate::review::{ReviewView, Row};
use crate::theme::Theme;
use argus_protocol::CursorShape;

/// The selection marker, and the blank gutter every other row gets so text
/// stays aligned whether or not it's selected.
/// The text caret, drawn rather than using the terminal cursor: the
/// cursor belongs to whichever pane is focused.
const CARET: &str = "▏";

const MARKER: &str = "▌";
const GUTTER: &str = " ";

/// Every list item is a name line plus a detail line. `app` hit-tests
/// clicks against this, so it is shared rather than local.
pub const ROW_HEIGHT: u16 = 2;

/// Blank columns between panels, and between the panels and the screen
/// edge. Without it the cards touch and stop reading as separate surfaces.
const GUTTER_COLS: u16 = 1;

/// A dragged column cannot be collapsed beyond this outer width. The
/// renderer scales the floor down only when the terminal itself is too
/// narrow to fit five such columns.
pub const MIN_COLUMN_WIDTH: u16 = 8;

/// Folded-away projects: a disclosure mark in the left page gutter, not a
/// full-height rail. The rest of that gutter is the click target.
const COLLAPSED_TAB: &str = "▸";

/// One list item: what it is, a dimmer line of what's true about it, and
/// an optional count pinned to the right of the name line. The badge is
/// there because these columns are narrow — a count appended to the detail
/// line is the first thing to get truncated away.
pub struct Item<'a> {
    pub name: Vec<Span<'a>>,
    pub detail: Vec<Span<'a>>,
    pub badge: Vec<Span<'a>>,
}

impl<'a> Item<'a> {
    fn new(name: Vec<Span<'a>>, detail: Vec<Span<'a>>) -> Self {
        Item {
            name,
            detail,
            badge: Vec::new(),
        }
    }

    fn badged(mut self, badge: Vec<Span<'a>>) -> Self {
        self.badge = badge;
        self
    }
}

pub fn render(f: &mut Frame, app: &mut App) {
    let th = app.theme;
    // The page owns its background; leaving it `Reset` would inherit
    // whatever the host terminal happens to be, and the elevation between
    // page and panel is what makes the panels read as cards.
    f.render_widget(Block::default().style(Style::default().bg(th.bg)), f.area());

    let page = inset(f.area(), GUTTER_COLS);
    // A blank row above the status bar keeps it off the panel borders.
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(2)])
        .split(page);

    // The hardware cursor is decided once, here, and applied last.
    //
    // Ratatui keeps a single cursor position per frame, so every widget
    // that sets one overwrites whatever was drawn before it. Deciding
    // per-widget means a layer on top can only ever *add* a position,
    // never take one away: an overlay whose child had hidden its cursor
    // left the content column's cursor stranded on top of the overlay.
    // Each layer replaces the decision outright, `None` included.
    let fullscreen =
        app.pane_fullscreen && app.focus == Focus::PaneContent && app.column_pane().is_some();
    let mut cursor = if fullscreen {
        app.layout.projects = Panel::default();
        app.layout.repositories = Panel::default();
        app.layout.checkouts = Panel::default();
        app.layout.panes = Panel::default();
        render_content(f, app, root[0], th)
    } else {
        render_columns(f, app, root[0])
    };
    render_status(f, app, root[1], th);

    // Above the columns, below the modals: a picker opened from an
    // overlay still has to be reachable.
    let overlay_cursor = render_overlay(f, app, page, th);
    if app.overlay.is_some() {
        cursor = overlay_cursor;
    }

    // These draw their own caret and cover what is under them, so no child
    // terminal's cursor has any business showing through.
    if app.picker.is_some() {
        render_picker(f, app, f.area(), th);
        cursor = None;
    }
    if app.dir_picker.is_some() {
        render_dir_picker(f, app, f.area(), th);
        cursor = None;
    }
    if app.prompt.is_some() {
        render_prompt(f, app, f.area(), th);
        cursor = None;
    }

    app.layout.cursor = cursor;
    if let Some(placement) = cursor {
        f.set_cursor_position(placement.position);
    }
}

/// Draws the normal five-column spine. The projects column may be folded
/// away to a tab in the left gutter, in which case its width is ceded to
/// the other four.
fn render_columns(f: &mut Frame, app: &mut App, area: Rect) -> Option<CursorPlacement> {
    let th = app.theme;
    let collapsed = app.projects_collapsed;
    let constraints = if collapsed {
        collapsed_projects_constraints(area.width, app.column_widths.as_deref())
    } else {
        column_constraints(area.width, app.column_widths.as_deref())
    };
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .spacing(GUTTER_COLS)
        .constraints(constraints)
        .split(area);

    // When projects is folded away there is no leading card, so the
    // remaining four occupy `cols[0..4]`. `col(i)` is the expanded index.
    let col = |i: usize| cols[if collapsed { i - 1 } else { i }];

    app.layout.projects = if collapsed {
        render_collapsed_projects(f, area, th)
    } else {
        let project_rows: Vec<Item> = app
            .tree
            .iter()
            .map(|p| {
                let panes: usize = p
                    .repositories
                    .iter()
                    .flat_map(|r| r.checkouts.iter())
                    .map(|c| c.listed_panes().count())
                    .sum();
                let status = p
                    .repositories
                    .iter()
                    .flat_map(|r| r.checkouts.iter())
                    .filter_map(worst_pane_status)
                    .max_by_key(rank);
                let item = Item::new(
                    vec![
                        status_dot(status, th),
                        Span::styled(
                            p.name.clone(),
                            Style::default().fg(th.text).add_modifier(Modifier::BOLD),
                        ),
                    ],
                    vec![Span::styled(
                        plural(p.repositories.len(), "repository"),
                        Style::default().fg(th.dim),
                    )],
                );
                if panes == 0 {
                    item
                } else {
                    item.badged(vec![Span::styled(
                        format!("{panes} ▣"),
                        Style::default().fg(th.dim),
                    )])
                }
            })
            .collect();
        // The projects column is scoped to the open workspace, so it says so
        // in its own title rather than leaving the scope to be inferred.
        let projects_title = if app.open_workspace.is_empty() {
            "projects".to_string()
        } else {
            format!("projects · {}", app.open_workspace)
        };
        render_column(
            f,
            col(0),
            &projects_title,
            project_rows,
            app.focus == Focus::Projects,
            (!app.tree.is_empty()).then_some(app.sel_project),
            app.layout.projects.first,
            "no projects yet

n  add one",
            th,
        )
    };

    let repository_rows: Vec<Item> = app
        .current_project()
        .map(|p| {
            p.repositories
                .iter()
                .map(|r| {
                    let panes: usize = r.checkouts.iter().map(|c| c.listed_panes().count()).sum();
                    let status = r
                        .checkouts
                        .iter()
                        .filter_map(worst_pane_status)
                        .max_by_key(rank);
                    let item = Item::new(
                        vec![
                            status_dot(status, th),
                            Span::styled(
                                r.name.clone(),
                                Style::default().fg(th.text).add_modifier(Modifier::BOLD),
                            ),
                        ],
                        vec![Span::styled(
                            plural(r.checkouts.len(), "checkout"),
                            Style::default().fg(th.dim),
                        )],
                    );
                    if panes == 0 {
                        item
                    } else {
                        item.badged(vec![Span::styled(
                            format!("{panes} ▣"),
                            Style::default().fg(th.dim),
                        )])
                    }
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
        app.layout.repositories.first,
        "no repositories

n  add one",
        th,
    );

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
        th,
    );

    let pane_rows: Vec<Item> = app
        .current_checkout()
        .map(|c| {
            c.listed_panes()
                .flat_map(|p| {
                    let flash = if app.pane_is_flashing(p.id) {
                        Style::default().bg(th.sel_bg_dim)
                    } else {
                        Style::default()
                    };
                    let mut state = status_dot(Some(p.status), th);
                    state.style = state.style.patch(flash);
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
                        pane_detail(p, th),
                    )
                    .badged(vec![Span::styled(
                        format!("#{}", p.id.0),
                        Style::default().fg(th.dim),
                    )]);
                    std::iter::once(parent).chain(p.children.iter().map(|c| child_item(c, th)))
                })
                .collect()
        })
        .unwrap_or_default();
    // The selection is a pane, but the rows it sits among include the
    // children listed under each one, so the highlight has to be moved
    // onto the row that pane actually occupies.
    let selected_row = app.current_checkout().and_then(|c| {
        pane_row_owners(c)
            .iter()
            .position(|owner| *owner == app.sel_pane)
    });
    app.layout.panes = render_column(
        f,
        col(3),
        "panes",
        pane_rows,
        app.focus == Focus::Panes,
        selected_row,
        app.layout.panes.first,
        "nothing running

s  shell
a  agent",
        th,
    );

    render_content(f, app, col(4), th)
}

fn column_constraints(total_width: u16, preferred: Option<&[u16]>) -> Vec<Constraint> {
    let Some(mut widths) = preferred
        .filter(|widths| widths.len() == 5)
        .map(<[u16]>::to_vec)
    else {
        return vec![
            Constraint::Percentage(16),
            Constraint::Percentage(17),
            Constraint::Percentage(17),
            Constraint::Percentage(18),
            Constraint::Percentage(32),
        ];
    };

    let available = total_width.saturating_sub(GUTTER_COLS * 4);
    if available == 0 {
        return vec![Constraint::Length(0); 5];
    }
    let floor = MIN_COLUMN_WIDTH.min(available / 5).max(1);
    fit_widths(&mut widths, available, floor);
    widths.into_iter().map(Constraint::Length).collect()
}

/// The collapsed layout: no projects card, its width passing to the other
/// four columns. Gutters dragged before collapsing are absolute preferences,
/// not fractions, so the four survivors keep them as-is and the slack lands
/// in the live view; with nothing captured yet the default split is re-dealt
/// over four columns instead of five.
fn collapsed_projects_constraints(total_width: u16, preferred: Option<&[u16]>) -> Vec<Constraint> {
    let available = total_width.saturating_sub(GUTTER_COLS * 3);
    if available == 0 {
        return vec![Constraint::Length(0); 4];
    }
    let floor = MIN_COLUMN_WIDTH.min(available / 4).max(1);
    let mut widths: Vec<u16> = match preferred.filter(|widths| widths.len() == 5) {
        Some(widths) => widths[1..].to_vec(),
        None => [20u32, 21, 21, 38]
            .iter()
            .map(|share| (u32::from(available) * share / 100).max(u32::from(floor)) as u16)
            .collect(),
    };
    fit_widths(&mut widths, available, floor);
    widths.into_iter().map(Constraint::Length).collect()
}

/// Reconciles preferred widths with what is actually available: nothing
/// below the floor, any shortfall reclaimed from the right, any spare
/// handed to the last column (the live view, where spare width does the
/// most good).
fn fit_widths(widths: &mut [u16], available: u16, floor: u16) {
    for width in widths.iter_mut() {
        *width = (*width).max(floor);
    }
    let mut sum: u32 = widths.iter().map(|w| u32::from(*w)).sum();
    let available = u32::from(available);
    if sum < available {
        if let Some(last) = widths.last_mut() {
            *last = last.saturating_add((available - sum) as u16);
        }
    } else {
        for width in widths.iter_mut().rev() {
            if sum <= available {
                break;
            }
            let take = (sum - available).min(u32::from(width.saturating_sub(floor)));
            *width -= take as u16;
            sum -= take;
        }
    }
}

/// Renders one bordered column of rows and returns its inner (post-border)
/// area, so the caller can hit-test mouse clicks against the same rows.
#[allow(clippy::too_many_arguments)]
fn render_column(
    f: &mut Frame,
    area: Rect,
    title: &str,
    rows: Vec<Item>,
    focused: bool,
    selected: Option<usize>,
    scrolled_to: usize,
    empty_hint: &str,
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
    let visible = (inner.height / ROW_HEIGHT) as usize;
    let first = scrolled_to_show(scrolled_to, selected, visible, rows.len());
    panel.first = first;

    for (i, item) in rows.into_iter().enumerate().skip(first).take(visible) {
        let Some(row) = row_rect(inner, i - first) else {
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
fn scrolled_to_show(
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

/// The projects column folded away: a disclosure mark in the page's left
/// gutter, sitting beside the first remaining card's top border. The rest
/// of that gutter is empty page, so nothing reads as a second column —
/// clicking it (or `p`) brings the column back. Losing the project name is
/// fine because the breadcrumb still carries it.
fn render_collapsed_projects(f: &mut Frame, columns: Rect, th: Theme) -> Panel {
    let Some(x) = columns.x.checked_sub(GUTTER_COLS) else {
        return Panel::default();
    };
    let gutter = Rect {
        x,
        y: columns.y,
        width: GUTTER_COLS,
        height: columns.height,
    };
    let chip = Rect {
        x,
        y: columns.y,
        width: 1,
        height: 1,
    };
    f.render_widget(
        Paragraph::new(Span::styled(
            COLLAPSED_TAB,
            Style::default().fg(th.accent).bg(th.surface),
        )),
        chip,
    );
    Panel {
        outer: gutter,
        inner: chip,
        first: 0,
    }
}

/// A two-line item: name, then detail. The selection is a raised bar over
/// both lines with an accent marker pinning the first; unselected rows get
/// a blank gutter so text lines up either way. `dim` spans would sink into
/// the selection fill, so they are lifted to `muted` there.
fn render_row(f: &mut Frame, area: Rect, item: Item, selected: bool, focused: bool, th: Theme) {
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

    let badge = lift(item.badge, bar, selected, th);
    if !badge.is_empty() {
        let used: usize = name.iter().chain(badge.iter()).map(Span::width).sum();
        // One trailing column of air, and nothing at all if it won't fit.
        if let Some(pad) = (area.width as usize).checked_sub(used + 1) {
            name.push(Span::styled(" ".repeat(pad), bar));
            name.extend(badge);
        }
    }
    let mut detail = vec![Span::styled(GUTTER, bar), Span::styled("  ", bar)];
    detail.extend(lift(item.detail, bar, selected, th));

    let name = ellipsize_spans(name, area.width as usize);
    let detail = ellipsize_spans(detail, area.width as usize);

    f.render_widget(
        Paragraph::new(vec![Line::from(name), Line::from(detail)]).style(bar),
        area,
    );
}

/// A padded card. Focus has to be unmissable at a glance, so the focused
/// panel is lifted a step in elevation and given an accent border and
/// title, against the unfocused panels' receding edge and muted label.
fn panel_block(title: &str, focused: bool, th: Theme, width: u16) -> Block<'_> {
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

fn ellipsize_text(text: &str, width: usize) -> String {
    ellipsize_spans(vec![Span::raw(text.to_string())], width)
        .into_iter()
        .map(|span| span.content.into_owned())
        .collect()
}

fn ellipsize_spans<'a>(spans: Vec<Span<'a>>, width: usize) -> Vec<Span<'a>> {
    if spans.iter().map(Span::width).sum::<usize>() <= width {
        return spans;
    }
    if width == 0 {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut remaining = width - 1;
    let mut ellipsis_style = Style::default();
    'spans: for span in spans {
        let style = span.style;
        ellipsis_style = style;
        for ch in span.content.chars() {
            let cell_width = Span::raw(ch.to_string()).width();
            if cell_width > remaining {
                break 'spans;
            }
            out.push(Span::styled(ch.to_string(), style));
            remaining -= cell_width;
        }
    }
    out.push(Span::styled("…", ellipsis_style));
    out
}

/// Shrinks a rect by `n` on every side, clamping rather than underflowing.
fn inset(area: Rect, n: u16) -> Rect {
    Rect {
        x: area.x.saturating_add(n),
        y: area.y.saturating_add(n),
        width: area.width.saturating_sub(n * 2),
        height: area.height.saturating_sub(n * 2),
    }
}

fn row_rect(inner: Rect, i: usize) -> Option<Rect> {
    row_rect_of(inner, i, ROW_HEIGHT)
}

fn row_rect_of(inner: Rect, i: usize, height: u16) -> Option<Rect> {
    let offset = u16::try_from(i).ok()?.checked_mul(height)?;
    let y = inner.y.checked_add(offset)?;
    (y + height <= inner.y + inner.height).then(|| Rect::new(inner.x, y, inner.width, height))
}

/// The selected pane's live terminal. It normally occupies the rightmost
/// column, but fullscreen gives it the whole main content area. Which pane
/// it shows follows the panes column's selection.
fn render_content(f: &mut Frame, app: &mut App, area: Rect, th: Theme) -> Option<CursorPlacement> {
    // Typing focus is what the accent border promises here, so only
    // PaneContent lights it up — merely selecting a pane does not.
    let focused = app.focus == Focus::PaneContent;
    // A parked pane looks exactly like a quiet one, so the title has to say
    // that the rows on screen are history rather than the current output.
    let title = match app.scroll_indicator() {
        Some(where_) => format!("{where_} · {}", content_title(app)),
        None => content_title(app),
    };
    let block = panel_block(&title, focused, th, area.width);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let cursor = if app.current_pane().is_none() {
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
        None
    } else {
        let grid = app.column_pane().and_then(|id| app.grids.get(&id));
        render_term(f, grid, inner, focused)
    };
    app.layout.content = Panel {
        outer: area,
        inner,
        first: 0,
    };
    cursor
}

/// Drawn in the column the live pane uses, so the nav columns stay put
/// (DESIGN.md §9 M4).
/// The diff itself. The window around it is drawn by `render_overlay`,
/// which owns the border and the title.
fn render_review(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    let Some(view) = app.review.as_mut() else {
        return;
    };
    view.scroll_into_view(area.height as usize);
    let (from, to) = view.selection();

    let lines: Vec<Line> = view
        .rows
        .iter()
        .enumerate()
        .skip(view.top)
        .take(area.height as usize)
        .map(|(i, row)| review_line(view, *row, i >= from && i <= to, area.width as usize, th))
        .collect();

    f.render_widget(Paragraph::new(lines), area);
}

/// Four digits covers nearly every file; the code matters more than the rest.
const LINENO_WIDTH: usize = 4;

/// Splits one diff line into styled runs. Text no span covers is drawn plain,
/// which is most identifiers and all of any file with no grammar.
///
/// Offsets arrive from another process, so every one of them is checked
/// against this string before it is used to slice it. A bad span is dropped
/// rather than trusted: colour is not worth a panic in the renderer.
fn highlighted<'a>(text: &'a str, spans: &[HighlightSpan], th: Theme) -> Vec<Span<'a>> {
    if spans.is_empty() {
        return vec![Span::styled(text, Style::default().fg(th.text))];
    }
    let mut out = Vec::with_capacity(spans.len() * 2 + 1);
    let mut at = 0usize;
    for span in spans {
        let (start, end) = (span.start as usize, span.end as usize);
        if start < at
            || end > text.len()
            || start >= end
            || !text.is_char_boundary(start)
            || !text.is_char_boundary(end)
        {
            continue;
        }
        if start > at {
            out.push(Span::styled(&text[at..start], Style::default().fg(th.text)));
        }
        out.push(Span::styled(&text[start..end], syntax_style(span.kind, th)));
        at = end;
    }
    if at < text.len() {
        out.push(Span::styled(&text[at..], Style::default().fg(th.text)));
    }
    out
}

fn syntax_style(kind: HighlightKind, th: Theme) -> Style {
    let s = th.syntax;
    match kind {
        HighlightKind::Keyword => Style::default().fg(s.keyword),
        HighlightKind::Str => Style::default().fg(s.string),
        HighlightKind::Comment => Style::default()
            .fg(s.comment)
            .add_modifier(Modifier::ITALIC),
        HighlightKind::Number | HighlightKind::Constant => Style::default().fg(s.number),
        HighlightKind::Type => Style::default().fg(s.type_name),
        HighlightKind::Function => Style::default().fg(s.function),
        HighlightKind::Property => Style::default().fg(s.property),
        HighlightKind::Operator => Style::default().fg(s.operator),
        HighlightKind::Punctuation => Style::default().fg(s.punctuation),
    }
}

fn review_line<'a>(
    view: &'a ReviewView,
    row: Row,
    selected: bool,
    width: usize,
    th: Theme,
) -> Line<'a> {
    let file = &view.review.files[row.file()];
    let pad = " ".repeat(LINENO_WIDTH + 1);

    let spans = match row {
        Row::File { .. } => {
            let mut v = vec![
                Span::styled(
                    format!(" {} ", file.kind.marker()),
                    Style::default().fg(th.on_accent).bg(th.accent),
                ),
                Span::styled(
                    format!(" {}", file.path),
                    Style::default().fg(th.text).add_modifier(Modifier::BOLD),
                ),
            ];
            if file.added_lines() + file.removed_lines() > 0 {
                v.push(Span::styled(
                    format!("  +{}", file.added_lines()),
                    Style::default().fg(th.ok),
                ));
                v.push(Span::styled(
                    format!(" -{}", file.removed_lines()),
                    Style::default().fg(th.err),
                ));
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
        // The only row that splits. Headers and notes span the width in
        // either view, and the wash below is skipped because each side
        // carries its own.
        Row::Pair {
            hunk, left, right, ..
        } => {
            // Cells, not bytes: the rule is a three-byte glyph.
            let half = width.saturating_sub(DIVIDER.chars().count()) / 2;
            let mut spans = diff_side(file, hunk, left, true, half, selected, th);
            spans.push(Span::styled(DIVIDER, Style::default().fg(th.edge)));
            spans.extend(diff_side(file, hunk, right, false, half, selected, th));
            spans
        }
        Row::Line { hunk, line, .. } => {
            let l = &file.hunks[hunk].lines[line];
            // The old side's number only where there is no new one.
            let no = match l.new_lineno.or(l.old_lineno) {
                Some(n) => format!("{n:>LINENO_WIDTH$}"),
                None => " ".repeat(LINENO_WIDTH),
            };
            // The marker keeps its colour even though the wash already says
            // which side this is: the wash is gone wherever a terminal drops
            // backgrounds, and the marker column is what is left.
            let marker_fg = match l.kind {
                LineKind::Added => th.ok,
                LineKind::Removed => th.err,
                LineKind::Context => th.dim,
            };
            let mut spans = vec![
                Span::styled(format!("{no} "), Style::default().fg(th.dim)),
                Span::styled(
                    crate::review::marker(l.kind).to_string(),
                    Style::default().fg(marker_fg),
                ),
            ];
            spans.extend(highlighted(&l.text, &l.spans, th));
            spans
        }
    };

    // A wash rather than a marker column: the left edge is already spent,
    // and a range should read as one block. Added and removed lines carry
    // their own wash whether or not they are selected, which is what frees
    // the foreground for syntax.
    let mut line = Line::from(spans);
    if let Row::Line {
        hunk, line: idx, ..
    } = row
    {
        if let Some(bg) = wash(file.hunks[hunk].lines[idx].kind, selected, th) {
            line = line.style(Style::default().bg(bg));
        }
    }
    line
}

/// Between the two sides. Air either side of the rule keeps one side's
/// wash from running into the other's.
const DIVIDER: &str = " │ ";

/// One half of a split row, padded to exactly `width` so the divider and
/// the far side land in the same columns on every row. Each side is
/// ellipsized on its own: half a screen is narrower than a diff line often
/// is, and letting one side overrun would push the other off the row.
///
/// `old` picks which of the line's two numbers this side is showing, which
/// is the whole reason a split view can label both. Where a run of removals
/// is longer than the run that replaced it the far side has no line at all;
/// it is drawn recessed rather than blank, so the gap reads as absence
/// rather than as an empty line of code.
fn diff_side<'a>(
    file: &'a FileDiff,
    hunk: usize,
    line: Option<usize>,
    old: bool,
    width: usize,
    selected: bool,
    th: Theme,
) -> Vec<Span<'a>> {
    let Some(i) = line else {
        return vec![Span::styled(
            " ".repeat(width),
            Style::default().bg(th.surface),
        )];
    };
    let l = &file.hunks[hunk].lines[i];
    let no = match if old { l.old_lineno } else { l.new_lineno } {
        Some(n) => format!("{n:>LINENO_WIDTH$}"),
        None => " ".repeat(LINENO_WIDTH),
    };
    let marker_fg = match l.kind {
        LineKind::Added => th.ok,
        LineKind::Removed => th.err,
        LineKind::Context => th.dim,
    };
    let mut spans = vec![
        Span::styled(format!("{no} "), Style::default().fg(th.dim)),
        Span::styled(
            crate::review::marker(l.kind).to_string(),
            Style::default().fg(marker_fg),
        ),
    ];
    spans.extend(highlighted(&l.text, &l.spans, th));

    let mut spans = ellipsize_spans(spans, width);
    let used: usize = spans.iter().map(Span::width).sum();
    spans.push(Span::raw(" ".repeat(width.saturating_sub(used))));
    if let Some(bg) = wash(l.kind, selected, th) {
        for span in &mut spans {
            span.style = span.style.bg(bg);
        }
    }
    spans
}

/// Which side of the diff a line is on, as a background. Selecting a line
/// brightens its wash rather than replacing it, so a selected range still
/// shows both sides.
fn wash(kind: LineKind, selected: bool, th: Theme) -> Option<Color> {
    match (kind, selected) {
        (LineKind::Added, false) => Some(th.add_bg),
        (LineKind::Added, true) => Some(th.add_bg_sel),
        (LineKind::Removed, false) => Some(th.del_bg),
        (LineKind::Removed, true) => Some(th.del_bg_sel),
        (LineKind::Context, true) => Some(th.sel_bg),
        (LineKind::Context, false) => None,
    }
}

fn render_history(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    let Some(view) = app.history.as_mut() else {
        return;
    };
    view.scroll_into_view(area.height as usize);

    let lines: Vec<Line> = view
        .rows
        .iter()
        .enumerate()
        .skip(view.top)
        .take(area.height as usize)
        .map(|(i, row)| history_line(view, *row, i == view.sel, th))
        .collect();

    f.render_widget(Paragraph::new(lines), area);
}

fn history_line<'a>(view: &'a HistoryView, row: HistoryRow, selected: bool, th: Theme) -> Line<'a> {
    let commit = &view.commits[row.commit()];
    let spans = match row {
        HistoryRow::Commit { .. } => vec![
            // The fold marker is the only sign that a header has anything
            // under it: its files are not fetched until it is opened.
            Span::styled(
                format!("{} ", fold_marker(commit)),
                Style::default().fg(th.muted),
            ),
            Span::styled(
                format!(" {} ", commit.info.short),
                Style::default().fg(th.on_accent).bg(th.accent),
            ),
            Span::styled(
                format!(" {}", commit.info.summary),
                Style::default().fg(th.text).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}", commit.info.author),
                Style::default().fg(th.muted),
            ),
        ],
        HistoryRow::File { .. } => {
            let Some(file) = view.file_at(row) else {
                return Line::default();
            };
            let mut v = vec![
                Span::styled(
                    format!("  {} ", file.kind.marker()),
                    Style::default().fg(th.on_accent).bg(th.accent),
                ),
                Span::styled(format!(" {}", file.path), Style::default().fg(th.text)),
            ];
            if let Some(old) = &file.old_path {
                v.push(Span::styled(
                    format!(" ← {old}"),
                    Style::default().fg(th.muted),
                ));
            }
            v
        }
    };
    let mut line = Line::from(spans);
    if selected {
        line = line.style(Style::default().bg(th.sel_bg));
    }
    line
}

fn fold_marker(commit: &crate::history::HistoryEntry) -> &'static str {
    match (commit.expanded, commit.pending) {
        (_, true) => "…",
        (true, _) => "▾",
        (false, _) => "▸",
    }
}

/// `project / checkout / pane` for the live view's title, which doubles as
/// the breadcrumb telling you where in the tree the content came from.
fn content_title(app: &App) -> String {
    match (
        app.current_project(),
        app.current_repository(),
        app.current_checkout(),
        app.current_pane(),
    ) {
        (Some(p), Some(r), Some(c), Some(pane)) => {
            format!("{} › {} › {} › {}", p.name, r.name, c.name, pane.title)
        }
        (Some(p), Some(r), Some(c), None) => format!("{} › {} › {}", p.name, r.name, c.name),
        (Some(p), Some(r), None, _) => format!("{} › {}", p.name, r.name),
        (Some(p), None, _, _) => p.name.clone(),
        _ => "live".to_string(),
    }
}

/// The status bar: where you are on the left, what you can press on the
/// right. Context-sensitive, because the same key means different things
/// inside a pane and in the nav columns.
///
/// The left half is the breadcrumb's seat, on loan to whatever the last
/// action reported. `App::on_key` hands it back on the next keypress, so a
/// report is read once and then gets out of the way.
fn render_status(f: &mut Frame, app: &App, area: Rect, th: Theme) {
    // `area` includes the blank padding row; the bar is its last row.
    let area = Rect {
        y: area.y + area.height.saturating_sub(1),
        height: area.height.min(1),
        ..area
    };

    let (hint, tone) = if let Some(p) = &app.picker {
        // What Enter does differs per picker, and "spawn" on the theme list
        // would be a small lie.
        let hint = match p.kind {
            PickerKind::Agent => "j/k move   enter spawn   esc cancel",
            PickerKind::Workspace { .. } => {
                "type to filter or name a new one   ↑/↓ move   enter open   esc cancel"
            }
            PickerKind::Theme => "j/k move   enter apply   esc cancel",
            PickerKind::Branch { .. } => "type to filter   ↑/↓ move   enter switch   esc cancel",
            PickerKind::File { .. } => "type to filter   ↑/↓ move   enter open   esc cancel",
            PickerKind::Change => "type to filter   ↑/↓ move   enter jump   esc cancel",
            PickerKind::ReviewRecipient { .. } => "j/k move   enter send   esc cancel",
        };
        (hint, th.dim)
    } else if app.prompt.is_some() {
        ("type to edit   enter confirm   esc cancel", th.dim)
    } else if app.leader_pending {
        let hint = if app.pane_fullscreen {
            "leader…   esc back   f restore   N attention   x close"
        } else {
            "leader…   esc back   f fullscreen   N attention   x close"
        };
        (hint, th.accent)
    } else if matches!(app.overlay, Some(Overlay::Settings { .. })) {
        ("j/k move   h/l change   esc close", th.dim)
    } else if matches!(app.overlay, Some(Overlay::Review)) {
        // A commit reached from the history overlay goes back to it rather
        // than flipping a side that means nothing there. `s` names where it
        // would take you, not where you are.
        let from_history = app
            .review
            .as_ref()
            .is_some_and(|v| v.review.commit.is_some())
            && app.history.is_some();
        let hint = match (from_history, app.review_split) {
            (true, false) => {
                "j/k  ]/[ file  f jump  c comment  e edit  s split  h history  esc close"
            }
            (true, true) => {
                "j/k  ]/[ file  f jump  c comment  e edit  s unified  h history  esc close"
            }
            (false, false) => {
                "j/k  ]/[ file  f jump  c comment  e edit  s split  b staged/unstaged  esc close"
            }
            (false, true) => {
                "j/k  ]/[ file  f jump  c comment  e edit  s unified  b staged/unstaged  esc close"
            }
        };
        (hint, th.dim)
    } else if matches!(app.overlay, Some(Overlay::History)) {
        (
            "j/k  ]/[ commit  l files/open  h fold  r refresh  R review  esc close",
            th.dim,
        )
    } else if app.overlay.is_some() {
        (
            "floating — ctrl-space then esc to close, x to kill   ctrl-v paste",
            th.dim,
        )
    } else if app.focus == Focus::PaneContent {
        // A parked pane is not taking input anywhere the operator can see,
        // so the way back to the live screen outranks the usual keymap.
        if app.scroll_indicator().is_some() {
            (
                "scrolled back   shift-pgup/pgdn move   type or scroll down to return",
                th.accent,
            )
        } else if app.pane_fullscreen {
            (
                "typing   ctrl-space: esc leave  f restore  x close   shift-pgup scroll",
                th.dim,
            )
        } else {
            (
                "typing   ctrl-space: esc leave  f fullscreen  x close   shift-pgup scroll",
                th.dim,
            )
        }
    } else {
        // Per column rather than one list of everything: the bar cannot
        // hold every key at once, and most of them only apply somewhere.
        let keys = match app.focus {
            Focus::Projects => {
                "j/k  l open  N needs  n add  D rm  w wksp  p fold  S settings  q detach"
            }
            Focus::Repositories => {
                "j/k move  l open  N attention  s shell  a agent  b branch  f file  n add  i init  D rm  q detach"
            }
            Focus::Checkouts => {
                "j/k move  l open  b branch  B all  F fetch  P pull  f file  R review  H history  n worktree  D rm  q detach"
            }
            _ => "j/k move  l open  N attention  s shell  a agent  b branch  f file  R review  H history  x close  q detach",
        };
        (keys, th.dim)
    };

    // An alert is the one thing on this bar the user *must* read, so it
    // outranks the keymap for space. An ordinary report is news rather than
    // an alarm: brighter than the breadcrumb it stands in for, but it yields
    // to the keys the same way the breadcrumb does.
    let alert = app.status_alert;
    let left = if app.status.is_empty() {
        Span::styled(breadcrumb(app), Style::default().fg(th.muted))
    } else {
        Span::styled(
            app.status.clone(),
            Style::default().fg(if alert { th.err } else { th.text }),
        )
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

/// How much of the screen a floating window takes. Big enough that vim is
/// usable, small enough that the tree still frames it — losing your place
/// is the thing the whole layout exists to prevent.
const OVERLAY_FRACTION: (u16, u16) = (82, 78);

/// Returns where the hardware cursor belongs while an overlay is up. An
/// overlay covers the content column, so `None` here means the cursor is
/// not drawn at all this frame — the column underneath does not get to
/// keep it (see [`render`]).
fn render_overlay(f: &mut Frame, app: &mut App, area: Rect, th: Theme) -> Option<CursorPlacement> {
    let Some(overlay) = &app.overlay else {
        app.layout.overlay = Panel::default();
        return None;
    };

    let width = (area.width * OVERLAY_FRACTION.0 / 100).max(20.min(area.width));
    let minimum_height = if matches!(overlay, Overlay::Settings { .. }) {
        // Two border rows and the panel's vertical padding sit outside the
        // setting lines and the two-line save-location footer.
        (Setting::ALL.len() as u16 * 3 + 7).min(area.height)
    } else {
        6.min(area.height)
    };
    let height = (area.height * OVERLAY_FRACTION.1 / 100).max(minimum_height);
    let popup = centered_rect(width, height, area);

    // The way out is in the title because a floating pane eats every other
    // key on purpose, and a window you cannot leave is worse than no window.
    let title = match overlay {
        Overlay::Pane { title, .. } => format!("{title}  ·  ctrl-space esc / F12 to close"),
        Overlay::Settings { .. } => "settings".to_string(),
        Overlay::Review => match app.review.as_ref() {
            Some(v) => match &v.review.commit {
                Some(c) => format!("review · {}  {}", c.short, c.summary),
                None => format!("review · {}", v.review.base.label()),
            },
            None => "review".to_string(),
        },
        Overlay::History => match app.history.as_ref() {
            Some(h) => format!("history · {} commits", h.commits.len()),
            None => "history".to_string(),
        },
    };

    f.render_widget(Clear, popup);
    let block = panel_block(&title, true, th, popup.width);
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    app.layout.overlay = Panel {
        outer: popup,
        inner,
        first: 0,
    };

    match overlay {
        Overlay::Pane { pane, .. } => render_term(f, app.grids.get(pane), inner, true),
        Overlay::Settings { sel } => {
            render_settings(f, app, inner, *sel, th);
            None
        }
        Overlay::Review => {
            render_review(f, app, inner, th);
            None
        }
        Overlay::History => {
            render_history(f, app, inner, th);
            None
        }
    }
}

/// Each setting gets a name, its current value, and a line saying what
/// choosing it does — the reason a panel exists rather than another picker.
fn render_settings(f: &mut Frame, app: &App, area: Rect, sel: usize, th: Theme) {
    let mut lines: Vec<Line> = Vec::new();
    for (i, setting) in Setting::ALL.iter().enumerate() {
        let selected = i == sel;
        let bar = if selected {
            Style::default().bg(th.sel_bg)
        } else {
            Style::default()
        };
        let (value, detail) = match setting {
            Setting::Editor => (
                app.settings.editor.label().to_string(),
                app.settings.editor.detail().to_string(),
            ),
            Setting::EditorCmd => {
                let value = if app.settings.editor_cmd.is_empty() {
                    "(from $VISUAL / $EDITOR)".to_string()
                } else {
                    app.settings.editor_cmd.clone()
                };
                (
                    value,
                    "the command to run, flags and all — enter to change".to_string(),
                )
            }
            Setting::Theme => (
                app.settings.theme.clone(),
                "colours for the whole client".to_string(),
            ),
            Setting::Notifications => (
                app.settings.notifications.label().to_string(),
                app.settings.notifications.detail().to_string(),
            ),
        };

        let marker = if selected {
            Span::styled(MARKER, Style::default().fg(th.accent).patch(bar))
        } else {
            Span::styled(GUTTER, bar)
        };
        // Only a value you can cycle gets the arrows; free text would be
        // promising a carousel that isn't there.
        let cyclable = *setting != Setting::EditorCmd;
        let (open, close) = if cyclable {
            ("‹ ", " ›")
        } else {
            ("  ", "")
        };
        lines.push(Line::from(vec![
            marker,
            Span::styled(
                format!(" {:<16}", setting.label()),
                Style::default().fg(th.text).patch(bar),
            ),
            Span::styled(open, Style::default().fg(th.dim).patch(bar)),
            Span::styled(
                value,
                Style::default()
                    .fg(th.accent)
                    .add_modifier(Modifier::BOLD)
                    .patch(bar),
            ),
            Span::styled(close, Style::default().fg(th.dim).patch(bar)),
        ]));
        lines.push(Line::from(vec![
            Span::styled(GUTTER, bar),
            Span::styled(
                format!("  {detail}"),
                Style::default().fg(th.dim).patch(bar),
            ),
        ]));
        lines.push(Line::raw(""));
    }

    lines.push(Line::from(vec![Span::styled(
        " changes save as you make them",
        Style::default().fg(th.dim).add_modifier(Modifier::ITALIC),
    )]));
    lines.push(Line::from(vec![Span::styled(
        format!(" {}", crate::settings::path().display()),
        Style::default().fg(th.dim),
    )]));

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

/// Rows a fuzzy picker will show at once. Past this it scrolls, so a
/// 5000-file list is still a modal and not a wall.
const PICKER_ROWS: usize = 12;

fn render_picker(f: &mut Frame, app: &App, area: Rect, th: Theme) {
    let Some(picker) = &app.picker else { return };
    let fuzzy = picker.is_fuzzy();

    let rows = picker
        .len()
        .min(if fuzzy { PICKER_ROWS } else { usize::MAX });
    // Borders, the top pad, and the query line with its own blank beneath.
    let chrome = 3 + if fuzzy { 2 } else { 0 };
    let height = (rows as u16 + chrome as u16).min(area.height);
    let widest = picker
        .items
        .iter()
        .map(|i| i.chars().count())
        .max()
        .unwrap_or(0);
    let floor = if fuzzy { 44 } else { 28 };
    let width = (widest as u16 + 8).clamp(floor, 72).min(area.width);
    let popup = centered_rect(width, height, area);

    f.render_widget(Clear, popup);
    let block = panel_block(picker.title, true, th, popup.width);
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    if inner.height == 0 {
        return;
    }

    let list = if fuzzy {
        let mut spans = vec![Span::styled("› ", Style::default().fg(th.accent))];
        spans.extend(field(&picker.query, th).spans);
        if picker.query.is_empty() {
            spans.push(Span::styled(" type to filter", Style::default().fg(th.dim)));
        }
        f.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect { height: 1, ..inner },
        );
        // Skip the query line and the blank under it.
        Rect {
            y: inner.y + 2,
            height: inner.height.saturating_sub(2),
            ..inner
        }
    } else {
        inner
    };

    // Scroll so the cursor stays on screen once the list outgrows the box.
    let visible = list.height as usize;
    let first = if picker.sel >= visible {
        picker.sel + 1 - visible
    } else {
        0
    };

    for slot in 0..visible {
        let i = first + slot;
        if i >= picker.len() {
            break;
        }
        let Some(row) = row_rect_of(list, slot, 1) else {
            break;
        };
        render_row(
            f,
            row,
            picker_item(picker, i, th),
            i == picker.sel,
            true,
            th,
        );
    }
}

/// One picker row. The last row of a branch picker can be the offer to
/// create the branch whose name you just typed.
fn picker_item<'a>(picker: &'a crate::app::Picker, i: usize, th: Theme) -> Item<'a> {
    if let (Some(name), true) = (&picker.create, i == picker.shown.len()) {
        return Item::new(
            vec![
                Span::styled("+ ", Style::default().fg(th.ok)),
                Span::styled("create ", Style::default().fg(th.dim)),
                Span::styled(name.clone(), Style::default().fg(th.ok)),
            ],
            Vec::new(),
        );
    }
    let name = picker
        .shown
        .get(i)
        .and_then(|idx| picker.items.get(*idx))
        .cloned()
        .unwrap_or_default();
    Item::new(
        vec![Span::styled(name, Style::default().fg(th.text))],
        Vec::new(),
    )
}

/// The directory browser. Wider than a picker and taller than a prompt,
/// because it has to show three things at once: where you are, what is
/// under it, and which of those are repositories.
fn render_dir_picker(f: &mut Frame, app: &App, area: Rect, th: Theme) {
    let Some(picker) = &app.dir_picker else {
        return;
    };

    let rows = picker.len().min(PICKER_ROWS);
    // Borders, the block's top pad, breadcrumb, query, the blank under it,
    // and the key hint.
    let height = (rows as u16 + 7).min(area.height);
    let width = 64.min(area.width);
    let popup = centered_rect(width, height, area);

    f.render_widget(Clear, popup);
    let block = panel_block(picker.title(), true, th, popup.width);
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    if inner.height < 3 {
        return;
    }

    // The tail of the path, not its head: the segments nearest the cursor
    // are the ones that tell you where you are.
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            elide_head(&picker.path, inner.width as usize),
            Style::default().fg(th.muted),
        ))),
        Rect { height: 1, ..inner },
    );

    let mut query = vec![Span::styled("› ", Style::default().fg(th.accent))];
    query.extend(field(&picker.query, th).spans);
    if picker.query.is_empty() {
        query.push(Span::styled(" type to filter", Style::default().fg(th.dim)));
    }
    f.render_widget(
        Paragraph::new(Line::from(query)),
        Rect {
            y: inner.y + 1,
            height: 1,
            ..inner
        },
    );

    let hint_y = inner.y + inner.height - 1;
    let list = Rect {
        y: inner.y + 3,
        height: hint_y.saturating_sub(inner.y + 3),
        ..inner
    };

    if let Some(error) = &picker.error {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                ellipsize_text(error, list.width as usize),
                Style::default().fg(th.err),
            ))),
            Rect { height: 1, ..list },
        );
    } else {
        let visible = list.height as usize;
        let first = picker.sel.saturating_sub(visible.saturating_sub(1));
        for slot in 0..visible {
            let i = first + slot;
            let Some(row) = picker.row(i) else { break };
            let Some(rect) = row_rect_of(list, slot, 1) else {
                break;
            };
            render_row(
                f,
                rect,
                dir_item(row, picker.here_label(), th),
                i == picker.sel,
                true,
                th,
            );
        }
    }

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("tab open   ← up   {}   esc cancel", picker.choose_label()),
            Style::default().fg(th.dim),
        ))),
        Rect {
            y: hint_y,
            height: 1,
            ..inner
        },
    );
}

fn dir_item(row: &DirRow, here: &str, th: Theme) -> Item<'static> {
    match row {
        DirRow::Here => Item::new(
            vec![
                Span::styled("· ", Style::default().fg(th.accent)),
                Span::styled(here.to_string(), Style::default().fg(th.text)),
            ],
            Vec::new(),
        ),
        DirRow::Child { name, is_repo } => {
            let item = Item::new(
                vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(name.clone(), Style::default().fg(th.text)),
                ],
                Vec::new(),
            );
            // Which children are repositories is invisible from the name,
            // and is usually the whole question being asked here.
            if *is_repo {
                item.badged(vec![Span::styled("git", Style::default().fg(th.ok))])
            } else {
                item
            }
        }
    }
}

/// Truncates from the left, keeping the end. The opposite of
/// [`ellipsize_text`], and the right choice for a path.
fn elide_head(text: &str, width: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= width || width == 0 {
        return text.to_string();
    }
    let tail: String = chars[chars.len() + 1 - width..].iter().collect();
    format!("…{tail}")
}

/// How wide a prompt box gets. A comment is a sentence and gets the wider
/// box; everything else here is an identifier — a branch name, a path — and
/// a wide box would only be a wide box.
const PROMPT_WIDTH: u16 = 54;
const COMMENT_WIDTH: u16 = 76;
/// How tall the text being typed may grow before the box stops growing and
/// starts scrolling instead. A prompt is still a modal: it must not become
/// the screen.
const PROMPT_MAX_ROWS: usize = 8;

/// The modal for all five prompts, drawn over everything else. Destructive
/// confirmations are tinted `err` so a removal never looks like a text
/// field you can dismiss by typing.
///
/// Text wraps rather than running off the edge, and the box grows with it:
/// a field you cannot read back is a field you cannot check before sending.
fn render_prompt(f: &mut Frame, app: &App, area: Rect, th: Theme) {
    let Some(prompt) = &app.prompt else { return };

    let wanted = match prompt {
        Prompt::Comment { .. } => COMMENT_WIDTH,
        _ => PROMPT_WIDTH,
    };
    let width = wanted.min(area.width.saturating_sub(2));
    // Borders and the gutter inside them, which the block below pads.
    let inner_width = width.saturating_sub(4);

    let (title, body, hint, danger) = match prompt {
        Prompt::NewWorktree { input, .. } => (
            "new worktree",
            wrapped_field(input, inner_width, th),
            "enter create   esc cancel",
            false,
        ),
        Prompt::NewRepository { parent, input, .. } => {
            // The path it will land at, resolved as you type: the name
            // alone would leave the one thing worth checking — where this
            // is going — off the screen that asks for it.
            let name = input.trim();
            let dest = if name.is_empty() {
                parent.clone()
            } else {
                crate::dirpicker::join(parent, name)
            };
            let mut lines = vec![Line::from(Span::styled(
                elide_head(&dest, inner_width as usize),
                Style::default().fg(th.muted),
            ))];
            lines.extend(wrapped_field(input, inner_width, th));
            (
                "new repository",
                lines,
                "empty to use the directory itself   enter create   esc cancel",
                false,
            )
        }
        Prompt::EditorCommand { input } => (
            "editor command",
            wrapped_field(input, inner_width, th),
            "empty to use $EDITOR   enter save   esc cancel",
            false,
        ),
        Prompt::Comment { anchor, input } => {
            // The anchor gets lines of its own. Sharing one with the text
            // left a long path only a few columns to type a sentence in.
            let where_ = anchor.notification("");
            let mut lines = vec![Line::from(Span::styled(
                ellipsize_text(where_.trim_end_matches([' ', ':']), inner_width as usize),
                Style::default().fg(th.muted),
            ))];
            lines.extend(wrapped_field(input, inner_width, th));
            (
                "comment to the agent",
                lines,
                "enter send   esc cancel",
                false,
            )
        }
        Prompt::ConfirmRemove { target, label } => {
            let (title, detail) = target.wording();
            (
                title,
                vec![Line::from(vec![
                    Span::styled(
                        label.clone(),
                        Style::default().fg(th.text).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(detail, Style::default().fg(th.muted)),
                ])],
                "y/enter remove   n/esc cancel",
                true,
            )
        }
    };

    // Borders, the body, and the hint under it.
    let height = (body.len() as u16 + 3).min(area.height);
    let popup = centered_rect(width, height, area);

    f.render_widget(Clear, popup);
    let accent = if danger { th.err } else { th.accent };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent))
        .padding(Padding::horizontal(1))
        .title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(th.on_accent)
                .bg(accent)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    let mut lines = body;
    lines.push(Line::from(Span::styled(hint, Style::default().fg(th.dim))));
    f.render_widget(Paragraph::new(lines), inner);
}

/// A text field with a visible caret. Empty fields show nothing but the
/// caret, so there is always something to look at.
fn field(input: &str, th: Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(input.to_string(), Style::default().fg(th.text)),
        Span::styled(CARET, Style::default().fg(th.accent)),
    ])
}

/// The same field wrapped to `width`, showing its last [`PROMPT_MAX_ROWS`]
/// rows. The tail is what survives because the caret is there: a prompt
/// that scrolls away from what you are typing is the bug this exists to
/// avoid.
fn wrapped_field(input: &str, width: u16, th: Theme) -> Vec<Line<'static>> {
    let mut rows = wrap(input, width);
    // The caret needs a cell of its own, and a row filled to the edge has
    // none left.
    if rows
        .last()
        .is_none_or(|r| Span::raw(r.clone()).width() >= width.max(1) as usize)
    {
        rows.push(String::new());
    }
    let last = rows.len() - 1;
    rows.into_iter()
        .enumerate()
        .skip(last.saturating_sub(PROMPT_MAX_ROWS - 1))
        .map(|(i, row)| {
            let mut spans = vec![Span::styled(row, Style::default().fg(th.text))];
            if i == last {
                spans.push(Span::styled(CARET, Style::default().fg(th.accent)));
            }
            Line::from(spans)
        })
        .collect()
}

/// Greedy word wrap. A word moves down whole when it will not fit; one
/// wider than the line itself is cut, because the alternative is a row
/// that overruns the box. Always returns at least one row, so an empty
/// field still has somewhere to put its caret.
fn wrap(text: &str, width: u16) -> Vec<String> {
    let width = width.max(1) as usize;
    let mut rows: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut cur_w = 0usize;
    for ch in text.chars() {
        let w = Span::raw(ch.to_string()).width().max(1);
        if cur_w + w > width {
            let carry = match cur.rfind(' ') {
                Some(i) if i + 1 < cur.len() => cur.split_off(i + 1),
                _ => String::new(),
            };
            rows.push(std::mem::take(&mut cur));
            cur_w = Span::raw(carry.clone()).width();
            cur = carry;
        }
        cur.push(ch);
        cur_w += w;
    }
    rows.push(cur);
    rows
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}

/// A checkout's row: what is running in it, and what git says about it.
fn checkout_item(c: &argus_protocol::CheckoutInfo, th: Theme) -> Item<'static> {
    // A checkout is usually sitting on the branch it's named after;
    // repeating it ("master master") says nothing. Show the branch only
    // when it actually differs.
    let mut detail = git_spans_unless_branch_is(c.git.as_ref(), &c.name, th);
    if detail.is_empty() {
        detail.push(Span::styled(
            if c.primary { "primary" } else { "worktree" },
            Style::default().fg(th.dim),
        ));
    }
    // Two agents in one directory is allowed, but it is never something to
    // find out from the diff later. The glyph carries it where the column
    // is too narrow for the words, which is most columns.
    let agents = c
        .listed_panes()
        .filter(|p| p.kind == argus_protocol::PaneKind::Agent)
        .count();
    let shared = agents > 1;
    if shared {
        detail.push(Span::styled("  ", Style::default()));
        detail.push(Span::styled(
            format!("shared by {agents}"),
            Style::default().fg(th.warn),
        ));
    }
    Item::new(
        vec![
            status_dot(worst_pane_status(c), th),
            Span::styled(
                format!("{} ", if c.primary { "⌂" } else { "⧉" }),
                Style::default().fg(if c.primary { th.muted } else { th.dim }),
            ),
            Span::styled(
                c.name.clone(),
                Style::default().fg(th.text).add_modifier(Modifier::BOLD),
            ),
            Span::styled(if shared { " ⚠" } else { "" }, Style::default().fg(th.warn)),
        ],
        detail,
    )
    .badged(if c.listed_panes().next().is_none() {
        Vec::new()
    } else {
        vec![Span::styled(
            format!("{} ▣", c.listed_panes().count()),
            Style::default().fg(th.dim),
        )]
    })
}

/// A branch with no directory of its own — an offer of one, and something
/// you can switch to or delete from where it stands.
fn branch_item(name: &str, th: Theme) -> Item<'static> {
    Item::new(
        vec![
            status_dot(None, th),
            Span::styled("⌥ ", Style::default().fg(th.dim)),
            Span::styled(name.to_string(), Style::default().fg(th.muted)),
        ],
        vec![Span::styled("no checkout", Style::default().fg(th.dim))],
    )
}

/// A branch that only exists on a remote. Named as the remote names it,
/// because the point of the row is that it isn't here yet.
fn remote_item(name: &str, th: Theme) -> Item<'static> {
    Item::new(
        vec![
            status_dot(None, th),
            Span::styled("⇣ ", Style::default().fg(th.dim)),
            Span::styled(name.to_string(), Style::default().fg(th.muted)),
        ],
        vec![Span::styled(
            "on the remote only",
            Style::default().fg(th.dim),
        )],
    )
}

/// The compact git summary on a checkout row: branch name (or `detached`),
/// commits ahead/behind upstream, and a dirty marker with its file count.
/// Each part carries its own color, so `clean` and `*3` read differently at
/// a glance instead of being one undifferentiated string.
/// [`git_spans`] with the branch name elided when it merely repeats the
/// row's own label. The ahead/behind/dirty markers always survive — those
/// are never implied by the name.
fn git_spans_unless_branch_is(
    git: Option<&GitStatus>,
    name: &str,
    th: Theme,
) -> Vec<Span<'static>> {
    let redundant = git.and_then(|g| g.branch.as_deref()) == Some(name);
    git_spans(git, th)
        .into_iter()
        .filter(|s| !(redundant && s.content.trim() == name))
        .collect()
}

fn git_spans(git: Option<&GitStatus>, th: Theme) -> Vec<Span<'static>> {
    let Some(g) = git else { return Vec::new() };
    let mut spans = vec![match &g.branch {
        Some(branch) => Span::styled(branch.clone(), Style::default().fg(th.muted)),
        None => Span::styled("detached".to_string(), Style::default().fg(th.dim)),
    }];
    if g.ahead > 0 {
        spans.push(Span::styled(
            format!("  ↑{}", g.ahead),
            Style::default().fg(th.ok),
        ));
    }
    if g.behind > 0 {
        spans.push(Span::styled(
            format!("  ↓{}", g.behind),
            Style::default().fg(th.warn),
        ));
    }
    // Spelled out rather than a `*n` sigil: the detail line has room, and
    // the count is the thing you act on.
    if g.dirty {
        spans.push(Span::styled(
            format!("  {}", plural(g.changed_files, "change")),
            Style::default().fg(th.warn),
        ));
    } else if g.branch.is_some() {
        spans.push(Span::styled("  clean", Style::default().fg(th.dim)));
    }
    spans
}

/// The exit code appended to a pane row, and only for a failure — a clean
/// exit is already said by the outlined box, and repeating it as text would
/// make every finished pane shout.
/// The detail line's word for a pane's state — the glyph's meaning spelled
/// out as well.
fn plural(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("{n} {noun}")
    } else if let Some(stem) = noun.strip_suffix('y') {
        format!("{n} {stem}ies")
    } else {
        format!("{n} {noun}s")
    }
}

fn status_word(status: PaneStatus) -> &'static str {
    match status {
        PaneStatus::Idle => "idle",
        PaneStatus::Working => "working",
        PaneStatus::Waiting => "needs you",
        PaneStatus::NeedsReview => "needs review",
        PaneStatus::Done => "done",
        PaneStatus::Failed => "failed",
        PaneStatus::Exited { .. } => "exited",
    }
}

/// The row's second line: which agent this is, then what it is saying.
///
/// The agent's own note when it has left one, because "needs you" without
/// saying what for still costs you a trip into the pane — which is the
/// whole thing this column exists to save.
///
/// The template leads because the name above it is the agent's own, and a
/// row renamed to "fixing the pty deadlock" otherwise stops saying which
/// CLI is in it. Dimmer than the note: it is how you tell two rows apart,
/// not what you came to read.
fn pane_detail(p: &argus_protocol::PaneInfo, th: Theme) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    // Only once the agent has taken the name over — before that the row
    // already reads "opencode" and saying it twice is noise.
    if let Some(template) = p.template.as_deref().filter(|t| *t != p.title) {
        spans.push(Span::styled(
            format!("{template}  "),
            Style::default().fg(th.dim),
        ));
    }
    match p.note.as_deref().filter(|n| !n.is_empty()) {
        Some(note) => spans.push(Span::styled(
            note.to_string(),
            Style::default().fg(if p.status.needs_you() {
                th.err
            } else {
                th.muted
            }),
        )),
        None => spans.push(Span::styled(
            status_word(p.status),
            Style::default().fg(th.dim),
        )),
    }
    spans
}

/// One agent running underneath a pane, as a row of its own beneath its
/// parent (DESIGN.md §8b). Indented and unbolded so the column still reads
/// as a list of panes: a child is something happening in a pane, not
/// somewhere else to go — selecting its row selects the pane it runs in.
fn child_item(c: &ChildAgentInfo, th: Theme) -> Item<'static> {
    Item::new(
        vec![
            Span::styled("  ⤷ ", Style::default().fg(th.dim)),
            status_dot(Some(c.status), th),
            Span::styled(c.label.clone(), Style::default().fg(th.muted)),
        ],
        vec![Span::styled(
            match c.note.as_deref().filter(|n| !n.is_empty()) {
                Some(note) => format!("    {note}"),
                None => format!("    {}", status_word(c.status)),
            },
            Style::default().fg(if c.status.needs_you() { th.err } else { th.dim }),
        )],
    )
}

/// Which pane each row of the panes column belongs to: a pane's own row,
/// then one row per child listed under it. Shared with `app` so a click
/// lands on the same pane the renderer drew there.
pub fn pane_row_owners(c: &argus_protocol::CheckoutInfo) -> Vec<usize> {
    c.listed_panes()
        .enumerate()
        .flat_map(|(i, p)| std::iter::repeat_n(i, 1 + p.children.len()))
        .collect()
}

fn exit_note(status: PaneStatus) -> String {
    match status {
        PaneStatus::Exited { code: Some(0) } => String::new(),
        PaneStatus::Exited { code: Some(c) } => format!("  exit {c}"),
        PaneStatus::Exited { code: None } => "  killed".to_string(),
        _ => String::new(),
    }
}

fn worst_pane_status(c: &argus_protocol::CheckoutInfo) -> Option<PaneStatus> {
    c.listed_panes()
        .flat_map(|p| std::iter::once(p.status).chain(p.children.iter().map(|child| child.status)))
        .max_by_key(rank)
}

/// Parents show the most urgent child (DESIGN.md §8b). Active work outranks
/// completed work, while review, failure, and waiting remain actionable.
fn rank(status: &PaneStatus) -> u8 {
    match status {
        PaneStatus::Exited { code: Some(0) } => 0,
        PaneStatus::Idle => 1,
        PaneStatus::Done => 2,
        PaneStatus::Working => 3,
        PaneStatus::Exited { .. } => 4,
        PaneStatus::NeedsReview => 5,
        PaneStatus::Failed => 6,
        PaneStatus::Waiting => 7,
    }
}

/// Shape carries the state signal (§8b); color reinforces it but is never
/// the only distinction. Outlined shapes mark idle or cleanly exited work.
fn status_dot(status: Option<PaneStatus>, th: Theme) -> Span<'static> {
    let (glyph, color) = match status {
        None => ("· ", th.dim),
        Some(PaneStatus::Idle) => ("○ ", th.ok),
        Some(PaneStatus::Working) => ("● ", th.warn),
        Some(PaneStatus::Waiting) => ("▲ ", th.err),
        Some(PaneStatus::NeedsReview) => ("◆ ", th.err),
        Some(PaneStatus::Done) => ("✓ ", th.ok),
        // Still running, unlike an exit, so it is a block rather than a cross.
        Some(PaneStatus::Failed) => ("■ ", th.err),
        Some(PaneStatus::Exited { code: Some(0) }) => ("□ ", th.dim),
        Some(PaneStatus::Exited { .. }) => ("✗ ", th.err),
    };
    Span::styled(glyph, Style::default().fg(color))
}

struct TermView<'a> {
    grid: Option<&'a Grid>,
}

/// Draws the pane and reports where the hardware cursor would go if this
/// pane owned it. Reporting rather than placing: `render` makes one cursor
/// decision for the whole frame (see [`render`]), so a pane drawn
/// underneath something else cannot strand its cursor on top of it.
/// Where the hardware cursor goes this frame, and what the child asked it
/// to look like. The two travel together because they come from the same
/// grid: the pane that owns the cursor owns its shape too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorPlacement {
    pub position: Position,
    pub shape: CursorShape,
}

fn render_term(
    f: &mut Frame,
    grid: Option<&Grid>,
    area: Rect,
    focused: bool,
) -> Option<CursorPlacement> {
    f.render_widget(TermView { grid }, area);
    term_cursor(grid, area, focused)
}

/// The child cursor mapped into `area`, or `None` when it must not be drawn.
///
/// Bounded by the grid as well as by the area. The two disagree for a frame
/// whenever a pane's on-screen size changes: the client draws the grid it
/// has, and only asks for a matching pty size afterwards, so a grid still
/// at the pty's 24x80 default can be drawn into a much larger box. Placing
/// the cursor at a coordinate the drawn rows don't reach puts it in empty
/// space — better to skip a frame than to point at nothing.
fn term_cursor(grid: Option<&Grid>, area: Rect, focused: bool) -> Option<CursorPlacement> {
    // A parked view is history: the child's cursor belongs to the live
    // screen, which is not the one being drawn.
    let grid = grid.filter(|grid| focused && grid.cursor.visible && !grid.is_scrolled())?;
    let rows = grid.view().len();
    let cols = grid.view().first().map_or(0, Vec::len);
    let row = usize::from(grid.cursor.row);
    let col = usize::from(grid.cursor.col);
    if row >= rows.min(usize::from(area.height)) || col >= cols.min(usize::from(area.width)) {
        return None;
    }
    Some(CursorPlacement {
        position: Position::new(area.x + grid.cursor.col, area.y + grid.cursor.row),
        shape: grid.cursor.shape,
    })
}

impl Widget for TermView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let Some(grid) = self.grid else { return };
        for (r, row) in grid.view().iter().enumerate() {
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
    use argus_protocol::{
        CheckoutId, CheckoutInfo, PaneId, PaneInfo, PaneKind, ProjectId, ProjectInfo, RepositoryId,
        RepositoryInfo,
    };
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn git(
        branch: Option<&str>,
        dirty: bool,
        changed: usize,
        ahead: usize,
        behind: usize,
    ) -> GitStatus {
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
                    note: None,
                    template: None,
                    children: Vec::new(),
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
    fn a_clean_branch_says_so_rather_than_leaving_it_to_be_inferred() {
        let s = git_spans(Some(&git(Some("master"), false, 0, 0, 0)), Theme::default());
        assert_eq!(text_of(&s).trim(), "master  clean");
    }

    #[test]
    fn one_change_is_not_reported_as_one_changes() {
        let s = git_spans(Some(&git(Some("m"), true, 1, 0, 0)), Theme::default());
        assert_eq!(text_of(&s).trim(), "m  1 change");
    }

    #[test]
    fn a_detached_head_says_so() {
        let s = git_spans(Some(&git(None, false, 0, 0, 0)), Theme::default());
        assert_eq!(text_of(&s).trim(), "detached");
    }

    #[test]
    fn ahead_behind_and_dirty_render_in_a_fixed_order() {
        let s = git_spans(Some(&git(Some("wt"), true, 5, 1, 2)), Theme::default());
        assert_eq!(text_of(&s).trim(), "wt  ↑1  ↓2  5 changes");
    }

    #[test]
    fn zero_counts_are_omitted_rather_than_shown_as_zero() {
        let s = text_of(&git_spans(
            Some(&git(Some("m"), false, 0, 0, 0)),
            Theme::default(),
        ));
        assert!(
            !s.contains('↑') && !s.contains('↓') && !s.contains('0'),
            "{s:?}"
        );
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
        assert_eq!(color_of("change"), Some(th.warn));
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
        assert_eq!(
            worst_pane_status(&c),
            Some(PaneStatus::Exited { code: Some(1) })
        );
    }

    #[test]
    fn a_clean_exit_ranks_below_a_live_pane() {
        let c = checkout_with(&[PaneStatus::Exited { code: Some(0) }, PaneStatus::Idle]);
        assert_eq!(worst_pane_status(&c), Some(PaneStatus::Idle));
    }

    #[test]
    fn descendant_status_rolls_up_through_checkout_repository_and_project() {
        let mut c = checkout_with(&[PaneStatus::Idle]);
        c.panes[0].children.push(ChildAgentInfo {
            label: "blocked child".to_string(),
            status: PaneStatus::Waiting,
            note: None,
        });
        let repository = RepositoryInfo {
            id: RepositoryId(2),
            name: "repo".to_string(),
            branches: Vec::new(),
            default_branch: None,
            remote_branches: Vec::new(),
            checkouts: vec![c],
        };
        let project = ProjectInfo {
            id: ProjectId(3),
            name: "project".to_string(),
            repositories: vec![repository],
        };

        let checkout_status = worst_pane_status(&project.repositories[0].checkouts[0]);
        let repository_status = project.repositories[0]
            .checkouts
            .iter()
            .filter_map(worst_pane_status)
            .max_by_key(rank);
        let project_status = project
            .repositories
            .iter()
            .flat_map(|repository| repository.checkouts.iter())
            .filter_map(worst_pane_status)
            .max_by_key(rank);

        assert_eq!(checkout_status, Some(PaneStatus::Waiting));
        assert_eq!(repository_status, Some(PaneStatus::Waiting));
        assert_eq!(project_status, Some(PaneStatus::Waiting));
    }

    #[test]
    fn a_kill_with_no_exit_code_counts_as_a_failure() {
        assert_eq!(
            rank(&PaneStatus::Exited { code: None }),
            rank(&PaneStatus::Exited { code: Some(1) })
        );
    }

    // --- the status glyph ---------------------------------------------------

    #[test]
    fn every_status_has_a_shape_distinct_glyph() {
        let th = Theme::default();
        let statuses = [
            PaneStatus::Idle,
            PaneStatus::Working,
            PaneStatus::Waiting,
            PaneStatus::NeedsReview,
            PaneStatus::Done,
            PaneStatus::Failed,
            PaneStatus::Exited { code: Some(0) },
            PaneStatus::Exited { code: Some(1) },
        ];
        let glyphs = statuses.map(|status| status_dot(Some(status), th).content.trim().to_string());

        for (i, glyph) in glyphs.iter().enumerate() {
            assert!(
                !glyphs[..i].contains(glyph),
                "{status:?} reuses glyph {glyph:?}",
                status = statuses[i]
            );
        }
        assert_ne!(
            status_dot(Some(PaneStatus::Done), th).content,
            status_dot(Some(PaneStatus::Exited { code: Some(0) }), th).content,
            "reviewed completion and process exit remain different states"
        );
    }

    #[test]
    fn each_live_state_gets_its_own_color() {
        let th = Theme::default();
        assert_eq!(status_dot(Some(PaneStatus::Idle), th).style.fg, Some(th.ok));
        assert_eq!(
            status_dot(Some(PaneStatus::Working), th).style.fg,
            Some(th.warn)
        );
        assert_eq!(
            status_dot(Some(PaneStatus::Waiting), th).style.fg,
            Some(th.err)
        );
        assert_eq!(
            status_dot(Some(PaneStatus::NeedsReview), th).style.fg,
            Some(th.err)
        );
        assert_eq!(status_dot(Some(PaneStatus::Done), th).style.fg, Some(th.ok));
    }

    #[test]
    fn exits_are_a_box_or_a_cross_not_a_live_state_glyph() {
        let th = Theme::default();
        let clean = status_dot(Some(PaneStatus::Exited { code: Some(0) }), th);
        let failed = status_dot(Some(PaneStatus::Exited { code: Some(1) }), th);
        assert_eq!(clean.content.trim(), "□");
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
        // Two lines per item, and a half-drawn item is worse than none.
        let inner = Rect::new(1, 1, 10, 5);
        assert_eq!(row_rect(inner, 0).unwrap().y, 1);
        assert_eq!(row_rect(inner, 0).unwrap().height, ROW_HEIGHT);
        assert_eq!(row_rect(inner, 1).unwrap().y, 3);
        assert!(row_rect(inner, 2).is_none(), "no room for both its lines");
    }

    // --- what a pane row says -----------------------------------------------

    fn pane(status: PaneStatus, note: Option<&str>) -> argus_protocol::PaneInfo {
        argus_protocol::PaneInfo {
            id: PaneId(1),
            kind: PaneKind::Agent,
            title: "claude".to_string(),
            status,
            note: note.map(str::to_string),
            template: None,
            children: Vec::new(),
        }
    }

    #[test]
    fn a_pane_with_nothing_to_say_falls_back_to_its_state() {
        let th = Theme::default();
        assert_eq!(
            text_of(&pane_detail(&pane(PaneStatus::Working, None), th)),
            "working"
        );
        assert_eq!(
            text_of(&pane_detail(&pane(PaneStatus::Waiting, None), th)),
            "needs you"
        );
        assert_eq!(
            text_of(&pane_detail(&pane(PaneStatus::Failed, None), th)),
            "failed"
        );
        assert_eq!(
            text_of(&pane_detail(&pane(PaneStatus::NeedsReview, None), th)),
            "needs review"
        );
        assert_eq!(
            text_of(&pane_detail(&pane(PaneStatus::Done, None), th)),
            "done"
        );
    }

    #[test]
    fn a_renamed_row_still_says_which_agent_is_in_it() {
        // The agent takes the name over as soon as it knows its task, so
        // without this a column of renamed rows stops telling you which
        // CLI to expect behind any of them.
        let th = Theme::default();
        let mut p = pane(PaneStatus::Working, None);
        p.title = "fixing the pty deadlock".to_string();
        p.template = Some("opencode".to_string());
        assert_eq!(text_of(&pane_detail(&p, th)), "opencode  working");

        p.note = Some("needs the db password".to_string());
        assert_eq!(
            text_of(&pane_detail(&p, th)),
            "opencode  needs the db password"
        );
    }

    #[test]
    fn a_working_child_gets_its_own_row_under_its_pane() {
        // Agents spawned inside a pane cannot touch its row, so without a
        // row of their own there is nothing on screen saying they are
        // there at all.
        let mut app = app_with_tree();
        if let Some(p) = app.tree[0].repositories[0].checkouts[0].panes.get_mut(0) {
            p.children = vec![
                ChildAgentInfo {
                    label: "searching the hook table".to_string(),
                    status: PaneStatus::Working,
                    note: None,
                },
                ChildAgentInfo {
                    label: "running the tests".to_string(),
                    status: PaneStatus::Waiting,
                    note: Some("needs a password".to_string()),
                },
            ];
        }
        let out = lines(&draw_at(&mut app, 160, 24)).join(
            "
",
        );
        assert!(
            out.contains("⤷"),
            "children are marked as such:
{out}"
        );
        assert!(out.contains("searching the"), "{out}");
        assert!(out.contains("running the tests"), "{out}");
        // A child stalled on a human says so where the parent's note goes,
        // which is the whole reason to list it.
        assert!(out.contains("needs a password"), "{out}");
    }

    #[test]
    fn the_selection_stays_on_the_pane_when_children_push_rows_down() {
        let mut app = app_with_tree();
        if let Some(p) = app.tree[0].repositories[0].checkouts[0].panes.get_mut(0) {
            p.children = vec![ChildAgentInfo {
                label: "searching the hook table".to_string(),
                status: PaneStatus::Working,
                note: None,
            }];
        }
        app.focus = Focus::Panes;
        app.sel_pane = 1;
        let buf = draw_at(&mut app, 160, 24);
        let inner = app.layout.panes.inner;
        // Row 0 is the pane, row 1 its child, so the second pane is row 2.
        let marker = buf.cell((inner.x, inner.y + ROW_HEIGHT * 2)).unwrap();
        assert_eq!(marker.symbol(), MARKER, "the marker follows the pane's row");
    }

    #[test]
    fn a_row_still_called_after_its_agent_does_not_say_so_twice() {
        let th = Theme::default();
        let mut p = pane(PaneStatus::Working, None);
        p.title = "opencode".to_string();
        p.template = Some("opencode".to_string());
        assert_eq!(text_of(&pane_detail(&p, th)), "working");
    }

    #[test]
    fn a_stalled_pane_says_what_it_wants_instead_of_that_it_wants_something() {
        // The whole point of the note: "needs you" still costs you a trip
        // into the pane to find out what for.
        let th = Theme::default();
        let spans = pane_detail(
            &pane(PaneStatus::Waiting, Some("needs the db password")),
            th,
        );
        assert_eq!(text_of(&spans), "needs the db password");
        assert_eq!(
            spans[0].style.fg,
            Some(th.err),
            "a blocked row should read as one"
        );
    }

    #[test]
    fn a_note_on_a_calm_pane_is_not_dressed_as_an_alarm() {
        let th = Theme::default();
        let spans = pane_detail(&pane(PaneStatus::Working, Some("rewriting the parser")), th);
        assert_eq!(spans[0].style.fg, Some(th.muted));
    }

    #[test]
    fn an_empty_note_is_not_an_empty_line() {
        let th = Theme::default();
        assert_eq!(
            text_of(&pane_detail(&pane(PaneStatus::Idle, Some("")), th)),
            "idle"
        );
    }

    #[test]
    fn a_failed_pane_outranks_the_calm_ones_but_not_a_waiting_one() {
        // Parents show the worst child; both want you, and the one you can
        // still answer wants you most.
        assert!(rank(&PaneStatus::Failed) > rank(&PaneStatus::Working));
        assert!(rank(&PaneStatus::Failed) > rank(&PaneStatus::Exited { code: Some(1) }));
        assert!(rank(&PaneStatus::NeedsReview) > rank(&PaneStatus::Exited { code: Some(1) }));
        assert!(rank(&PaneStatus::Waiting) > rank(&PaneStatus::Failed));
    }

    #[test]
    fn a_failed_pane_is_still_running_so_it_is_not_an_exit_cross() {
        // A cross would read as "this is over"; it isn't.
        let th = Theme::default();
        let failed = status_dot(Some(PaneStatus::Failed), th);
        assert_eq!(failed.content.trim(), "■");
        assert_eq!(failed.style.fg, Some(th.err));
    }

    // --- rendering the whole frame -----------------------------------------

    fn tree() -> Vec<ProjectInfo> {
        vec![ProjectInfo {
            id: ProjectId(1),
            name: "argus".to_string(),
            repositories: vec![RepositoryInfo {
                id: RepositoryId(2),
                name: "orion".to_string(),
                branches: Vec::new(),
                default_branch: None,
                remote_branches: Vec::new(),
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
                                note: None,
                                template: None,
                                children: Vec::new(),
                            },
                            PaneInfo {
                                id: PaneId(101),
                                kind: PaneKind::Shell,
                                title: "shell".to_string(),
                                status: PaneStatus::Idle,
                                note: None,
                                template: None,
                                children: Vec::new(),
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
            }],
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

    /// The status bar's row: the one carrying the nav keymap.
    fn bar_row(buf: &ratatui::buffer::Buffer) -> u16 {
        lines(buf)
            .iter()
            .rposition(|r| !r.trim().is_empty())
            .expect("the status bar") as u16
    }

    fn bar(buf: &ratatui::buffer::Buffer) -> String {
        lines(buf)[bar_row(buf) as usize].clone()
    }

    fn app_with_tree() -> App {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        // Keep the receiver alive so sends don't fail during render setup.
        std::mem::forget(rx);
        let mut app = App::new(tx);
        app.on_server_msg(argus_protocol::ServerMsg::Tree(tree()));
        app
    }

    fn cell(ch: char) -> argus_protocol::Cell {
        argus_protocol::Cell {
            ch: ch.to_string().into(),
            ..Default::default()
        }
    }

    // --- a pane parked in its scrollback -------------------------------------

    /// An app inside a pane whose view is parked `offset` lines back, with
    /// `mark` filling the rows the daemon answered with.
    fn app_scrolled_back(offset: u32, depth: u32, mark: char) -> App {
        let mut app = app_with_tree();
        app.focus = Focus::PaneContent;
        let pane = app.column_pane().expect("the fixture pane");
        let live = |c: char| vec![vec![cell(c); 30]; 5];
        let mut grid = crate::grid::Grid::new(live('L'));
        grid.scrollback = Some(crate::grid::Scrollback {
            offset,
            depth,
            cells: live(mark),
        });
        app.grids.insert(pane, grid);
        app
    }

    #[test]
    fn a_parked_pane_draws_its_history_rather_than_the_live_screen() {
        let mut app = app_scrolled_back(120, 4000, 'H');
        let text = lines(&draw(&mut app)).join("\n");
        assert!(text.contains("HHH"), "the rows read out of the scrollback");
        assert!(
            !text.contains("LLL"),
            "and not the live screen underneath them"
        );
    }

    #[test]
    fn a_parked_pane_shows_how_far_back_it_is_in_its_title() {
        // A pane parked in history looks exactly like a quiet one. Without
        // this the operator has no way to tell that what is on screen is
        // not what the child is doing now.
        let mut app = app_scrolled_back(120, 4000, 'H');
        let text = lines(&draw(&mut app)).join("\n");
        assert!(text.contains("\u{2191} 120/4000"), "got:\n{text}");
    }

    #[test]
    fn a_parked_pane_says_how_to_get_back_to_the_live_screen() {
        let mut app = app_scrolled_back(120, 4000, 'H');
        assert!(bar(&draw(&mut app)).contains("scrolled back"));

        let mut live = app_with_tree();
        live.focus = Focus::PaneContent;
        assert!(bar(&draw(&mut live)).contains("shift-pgup scroll"));
    }

    #[test]
    fn a_parked_pane_does_not_take_the_hardware_cursor() {
        // The child's cursor belongs to the live screen, which is not the
        // one being drawn. Placing it here points at unrelated history.
        let mut app = app_scrolled_back(120, 4000, 'H');
        let pane = app.column_pane().unwrap();
        app.grids.get_mut(&pane).unwrap().cursor = argus_protocol::Cursor {
            row: 1,
            col: 1,
            visible: true,
            ..Default::default()
        };
        draw(&mut app);
        assert_eq!(app.layout.cursor, None);
    }

    // --- a column taller than its card ---------------------------------------

    /// Eight checkouts in one repository, so a short column has to scroll.
    fn app_with_a_long_checkout_column() -> App {
        let mut app = app_with_tree();
        let r = &mut app.tree[0].repositories[0];
        r.checkouts = (0..8)
            .map(|i| argus_protocol::CheckoutInfo {
                id: CheckoutId(20 + i),
                name: format!("wt-{i}"),
                path: format!("/repo/wt-{i}"),
                primary: i == 0,
                git: None,
                panes: Vec::new(),
            })
            .collect();
        app.focus = Focus::Checkouts;
        app
    }

    /// The row of the checkouts column that a given screen row sits on.
    fn click_checkout(app: &mut App, drawn_row: u16) {
        let inner = app.layout.checkouts.inner;
        app.on_mouse(crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: inner.x + 1,
            row: inner.y + drawn_row * ROW_HEIGHT,
            modifiers: KeyModifiers::NONE,
        });
    }

    #[test]
    fn a_click_in_a_scrolled_column_selects_the_row_it_landed_on() {
        let mut app = app_with_a_long_checkout_column();
        app.sel_checkout = 7;
        // Tall enough for a few rows, far short of eight.
        let buf = draw_at(&mut app, 100, 12);
        let top = app.layout.checkouts.inner.y;
        let first_drawn = lines(&buf)[top as usize].clone();

        click_checkout(&mut app, 0);

        let name = app
            .current_checkout()
            .map(|c| c.name.clone())
            .unwrap_or_default();
        assert!(
            first_drawn.contains(&name),
            "clicking the top row must select the checkout drawn there: {first_drawn:?} selected {name:?}"
        );
    }

    #[test]
    fn a_scrolled_column_does_not_slide_when_the_selection_moves_back_up() {
        let mut app = app_with_a_long_checkout_column();
        app.sel_checkout = 7;
        let buf = draw_at(&mut app, 100, 12);
        let top = app.layout.checkouts.inner.y as usize;
        assert!(lines(&buf)[top].contains("wt-6"), "the column is scrolled");

        app.on_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        let buf = draw_at(&mut app, 100, 12);

        assert!(
            lines(&buf)[top].contains("wt-6"),
            "the selection is still on screen, so the list must not move under it: {:?}",
            lines(&buf)[top]
        );
    }

    /// The policy itself. Deriving the offset from the selection alone
    /// pins the selected row to the bottom of the card, which is what made
    /// the columns lurch whenever a row appeared above the cursor.
    #[test]
    fn a_column_scrolls_the_least_it_can_to_keep_the_selection_visible() {
        // Already on screen: nothing moves.
        assert_eq!(scrolled_to_show(3, Some(4), 5, 20), 3);
        // Above the window, and below it.
        assert_eq!(scrolled_to_show(3, Some(1), 5, 20), 1);
        assert_eq!(scrolled_to_show(3, Some(8), 5, 20), 4);
        // A list that fits needs no offset at all, and one that shrank
        // must not leave blank rows under it.
        assert_eq!(scrolled_to_show(3, Some(1), 5, 4), 0);
        assert_eq!(scrolled_to_show(9, Some(6), 5, 8), 3);
    }

    #[test]
    fn a_checkout_two_agents_are_working_in_says_it_is_shared() {
        let mut app = app_with_tree();
        let panes = &mut app.tree[0].repositories[0].checkouts[0].panes;
        for p in panes.iter_mut() {
            p.kind = argus_protocol::PaneKind::Agent;
        }

        let buf = draw_at(&mut app, 200, 20);
        let rendered = lines(&buf);
        let at = rendered
            .iter()
            .position(|l| l.contains("⌂ master"))
            .expect("the primary checkout has a row");

        assert!(
            rendered[at].contains('⚠'),
            "the glyph is what survives a narrow column: {:?}",
            rendered[at]
        );
        assert!(
            rendered[at + 1].contains("shared by 2"),
            "sharing a checkout is allowed, but not something to find out later: {:?}",
            rendered[at + 1]
        );
    }

    #[test]
    fn one_agent_and_a_shell_is_not_sharing() {
        let mut app = app_with_tree();

        let buf = draw_at(&mut app, 120, 20);
        let rendered = lines(&buf);

        assert!(
            !rendered.iter().any(|l| l.contains("shared")),
            "the fixture has one agent and one shell in that checkout"
        );
    }

    #[test]
    fn the_main_branch_gets_a_row_of_its_own_and_the_rest_do_not() {
        let mut app = app_with_tree();
        let r = &mut app.tree[0].repositories[0];
        r.branches = vec!["hotfix".to_string(), "trunk".to_string()];
        r.default_branch = Some("trunk".to_string());

        let buf = draw_at(&mut app, 120, 20);
        let rendered = lines(&buf);
        assert!(
            !rendered.iter().any(|l| l.contains("hotfix")),
            "an ordinary branch stays out of the column: {rendered:?}"
        );
        let at = rendered
            .iter()
            .position(|l| l.contains("trunk"))
            .expect("the main branch keeps its row whether or not it has a directory");

        // A row is two lines: the name, then what it is.
        assert!(
            rendered[at + 1].contains("no checkout"),
            "a branch row has to say what it is: {:?}",
            rendered[at + 1]
        );
    }

    #[test]
    fn a_remote_only_branch_says_that_is_where_it_is() {
        let mut app = app_with_tree();
        app.tree[0].repositories[0].remote_branches = vec!["origin/spike".to_string()];
        app.show_branches = true;

        // Wide, so the column has room for the row's own words.
        let rendered = lines(&draw_at(&mut app, 200, 20));
        let at = rendered
            .iter()
            .position(|l| l.contains("origin/spike"))
            .expect("a branch the remote has should be reachable");
        assert!(
            rendered[at + 1].contains("on the remote only"),
            "{:?}",
            rendered[at + 1]
        );
    }

    #[test]
    fn the_other_branches_appear_once_the_column_is_expanded() {
        let mut app = app_with_tree();
        app.tree[0].repositories[0].branches = vec!["hotfix".to_string()];
        app.show_branches = true;

        let rendered = lines(&draw_at(&mut app, 120, 20));
        let at = rendered
            .iter()
            .position(|l| l.contains("hotfix"))
            .expect("expanded, the branch should have a row of its own");
        assert!(
            rendered[at + 1].contains("no checkout"),
            "{:?}",
            rendered[at + 1]
        );
    }

    #[test]
    fn all_five_columns_are_drawn_in_the_normal_pane_view() {
        let mut app = app_with_tree();
        app.focus = Focus::PaneContent;
        let text = lines(&draw(&mut app)).join("\n");
        for title in ["projects", "repositories", "checkouts", "panes"] {
            assert!(
                text.contains(title),
                "{title} column missing while inside a pane"
            );
        }
    }

    #[test]
    fn fullscreen_gives_the_main_area_to_the_selected_pane() {
        let mut app = app_with_tree();
        app.focus = Focus::PaneContent;
        draw(&mut app);
        let column_width = app.layout.content.outer.width;

        app.pane_fullscreen = true;
        let text = lines(&draw(&mut app)).join("\n");

        assert!(app.layout.content.outer.width > column_width);
        for panel in [
            app.layout.projects,
            app.layout.repositories,
            app.layout.checkouts,
            app.layout.panes,
        ] {
            assert_eq!(
                panel.outer,
                Rect::default(),
                "hidden columns must not remain clickable"
            );
        }
        for title in ["projects", "repositories", "checkouts", "panes"] {
            assert!(
                !text.contains(title),
                "{title} column remained visible in fullscreen"
            );
        }
        assert!(text.contains("argus › orion › master › claude"));
        assert!(text.contains("f restore"));
    }

    #[test]
    fn a_focused_terminal_places_the_hardware_cursor_at_the_child_cursor() {
        let mut app = app_with_tree();
        app.focus = Focus::PaneContent;
        app.grids.insert(
            PaneId(100),
            Grid::with_cursor(
                vec![vec![Default::default(); 4]; 3],
                argus_protocol::Cursor {
                    row: 1,
                    col: 2,
                    visible: true,
                    ..Default::default()
                },
                Default::default(),
            ),
        );
        let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();

        terminal.draw(|f| render(f, &mut app)).unwrap();

        terminal.backend_mut().assert_cursor_position((
            app.layout.content.inner.x + 2,
            app.layout.content.inner.y + 1,
        ));
    }

    #[test]
    fn an_overlay_whose_cursor_is_hidden_does_not_leave_the_column_cursor_on_top_of_it() {
        // Regression: the content column and the overlay both drew a
        // terminal, and ratatui has one cursor slot per frame. The overlay
        // took the early return when its child had hidden its cursor,
        // which left the column's position in the slot — so the hardware
        // cursor sat on top of the overlay, pointing at a coordinate that
        // belonged to the pane behind it.
        let mut app = app_with_tree();
        app.focus = Focus::PaneContent;
        app.grids.insert(
            PaneId(100),
            Grid::with_cursor(
                vec![vec![Default::default(); 40]; 10],
                argus_protocol::Cursor {
                    row: 1,
                    col: 2,
                    visible: true,
                    ..Default::default()
                },
                Default::default(),
            ),
        );
        app.grids.insert(
            PaneId(101),
            Grid::with_cursor(
                vec![vec![Default::default(); 40]; 10],
                argus_protocol::Cursor {
                    row: 1,
                    col: 2,
                    visible: false,
                    ..Default::default()
                },
                Default::default(),
            ),
        );
        app.overlay = Some(Overlay::Pane {
            pane: PaneId(101),
            title: "editor".to_string(),
            ephemeral: true,
        });

        draw(&mut app);

        assert_eq!(
            app.layout.cursor, None,
            "the overlay owns the cursor and its child has hidden it"
        );
    }

    #[test]
    fn a_prompt_takes_the_cursor_away_from_the_pane_behind_it() {
        // The prompt draws its own caret; a second cursor blinking in the
        // column underneath is just noise.
        let mut app = app_with_tree();
        app.focus = Focus::PaneContent;
        app.grids.insert(
            PaneId(100),
            Grid::with_cursor(
                vec![vec![Default::default(); 40]; 10],
                argus_protocol::Cursor {
                    row: 1,
                    col: 2,
                    visible: true,
                    ..Default::default()
                },
                Default::default(),
            ),
        );
        draw(&mut app);
        assert!(app.layout.cursor.is_some());

        app.prompt = Some(Prompt::EditorCommand {
            input: String::new(),
        });
        draw(&mut app);

        assert_eq!(app.layout.cursor, None);
    }

    #[test]
    fn a_cursor_past_the_end_of_the_grid_is_not_drawn() {
        // A pane's on-screen box is resized a frame before its pty is, so
        // the grid can be smaller than the area it is drawn into. Placing
        // the cursor by the area alone points it at rows that were never
        // drawn.
        let area = Rect::new(0, 0, 80, 40);
        let grid = Grid::with_cursor(
            vec![vec![Default::default(); 80]; 24],
            argus_protocol::Cursor {
                row: 30,
                col: 0,
                visible: true,
                ..Default::default()
            },
            Default::default(),
        );

        assert_eq!(term_cursor(Some(&grid), area, true), None);
    }

    #[test]
    fn a_cursor_inside_both_the_grid_and_the_area_is_drawn() {
        let area = Rect::new(3, 5, 80, 40);
        let grid = Grid::with_cursor(
            vec![vec![Default::default(); 80]; 24],
            argus_protocol::Cursor {
                row: 7,
                col: 9,
                visible: true,
                ..Default::default()
            },
            Default::default(),
        );

        assert_eq!(
            term_cursor(Some(&grid), area, true).map(|c| c.position),
            Some(Position::new(12, 12))
        );
    }

    #[test]
    fn an_unfocused_or_hidden_cursor_is_not_drawn() {
        let area = Rect::new(0, 0, 80, 40);
        let visible = Grid::with_cursor(
            vec![vec![Default::default(); 80]; 24],
            argus_protocol::Cursor {
                row: 1,
                col: 1,
                visible: true,
                ..Default::default()
            },
            Default::default(),
        );
        let hidden = Grid::with_cursor(
            vec![vec![Default::default(); 80]; 24],
            argus_protocol::Cursor {
                row: 1,
                col: 1,
                visible: false,
                ..Default::default()
            },
            Default::default(),
        );

        assert_eq!(term_cursor(Some(&visible), area, false), None);
        assert_eq!(term_cursor(Some(&hidden), area, true), None);
        assert_eq!(term_cursor(None, area, true), None);
    }

    #[test]
    fn preferred_column_widths_are_used_and_keep_a_minimum() {
        let mut app = app_with_tree();
        app.column_widths = Some(vec![2, 18, 20, 20, 40]);
        draw(&mut app);

        assert_eq!(app.layout.projects.outer.width, MIN_COLUMN_WIDTH);
        assert_eq!(app.layout.repositories.outer.width, 18);
        assert_eq!(app.layout.checkouts.outer.width, 20);
        assert_eq!(app.layout.panes.outer.width, 20);
        assert_eq!(app.layout.content.outer.width, 28);
    }

    #[test]
    fn narrow_row_text_ends_in_an_ellipsis() {
        let mut app = app_with_tree();
        app.column_widths = Some(vec![8, 18, 18, 18, 34]);
        let text = lines(&draw(&mut app)).join("\n");

        assert!(
            text.contains("● …"),
            "narrow project name should end in an ellipsis:\n{text}"
        );
    }

    #[test]
    fn the_tree_contents_actually_reach_the_screen() {
        let mut app = app_with_tree();
        let text = lines(&draw(&mut app)).join("\n");
        assert!(text.contains("argus"), "project name");
        assert!(text.contains("master"), "checkout name");
        assert!(text.contains("claude"), "pane title");
    }

    #[test]
    fn repository_rows_roll_up_checkout_counts_panes_and_status() {
        let mut app = app_with_tree();
        app.tree[0].repositories.push(RepositoryInfo {
            id: RepositoryId(3),
            name: "satellite".to_string(),
            branches: Vec::new(),
            default_branch: None,
            remote_branches: Vec::new(),
            checkouts: vec![CheckoutInfo {
                id: CheckoutId(12),
                name: "main".to_string(),
                path: "/satellite".to_string(),
                primary: true,
                git: None,
                panes: vec![PaneInfo {
                    id: PaneId(102),
                    kind: PaneKind::Agent,
                    title: "waiting".to_string(),
                    status: PaneStatus::Waiting,
                    note: None,
                    template: None,
                    children: Vec::new(),
                }],
            }],
        });

        let buf = draw_at(&mut app, 140, 20);
        let text = lines(&buf).join("\n");
        assert!(
            text.contains("satellite"),
            "repository row missing:\n{text}"
        );
        assert!(
            text.contains("2 repositories"),
            "project rollup missing:\n{text}"
        );
        assert!(
            text.contains("1 ▣"),
            "repository pane rollup missing:\n{text}"
        );

        let status = buf
            .cell((
                app.layout.repositories.inner.x + 1,
                app.layout.repositories.inner.y + ROW_HEIGHT,
            ))
            .unwrap();
        assert_eq!(status.symbol(), "▲");
        assert_eq!(status.fg, app.theme.err);
    }

    #[test]
    fn the_focused_column_alone_gets_the_accent_border() {
        let th = Theme::default();
        let mut app = app_with_tree();
        app.focus = Focus::Checkouts;
        let buf = draw(&mut app);

        let corner = |p: Panel| buf.cell((p.outer.x, p.outer.y)).unwrap().fg;
        assert_eq!(corner(app.layout.checkouts), th.accent, "focused column");
        assert_eq!(corner(app.layout.projects), th.edge, "unfocused column");
    }

    #[test]
    fn the_three_elevations_show_up_on_screen() {
        // Page behind unfocused panel behind focused panel. This is what
        // makes the panels read as cards rather than boxes.
        let th = Theme::default();
        let mut app = app_with_tree();
        app.focus = Focus::Projects;
        let buf = draw(&mut app);

        // A blank cell inside each panel, below the last row.
        let blank = |p: Panel| {
            buf.cell((p.inner.x, p.inner.y + p.inner.height - 1))
                .unwrap()
                .bg
        };
        assert_eq!(
            blank(app.layout.projects),
            th.surface_focus,
            "focused panel"
        );
        assert_eq!(blank(app.layout.checkouts), th.surface, "unfocused panel");
        assert_eq!(buf.cell((0, 0)).unwrap().bg, th.bg, "the page behind them");
    }

    #[test]
    fn the_selected_row_is_marked_and_raised_never_reversed() {
        let th = Theme::default();
        let mut app = app_with_tree();
        app.focus = Focus::Checkouts;
        app.sel_checkout = 1;
        let buf = draw(&mut app);

        let inner = app.layout.checkouts.inner;
        let marker = buf.cell((inner.x, inner.y + ROW_HEIGHT)).unwrap();
        assert_eq!(
            marker.symbol(),
            MARKER,
            "selection marker on the selected row"
        );
        assert_eq!(marker.fg, th.accent);
        assert_eq!(marker.bg, th.sel_bg);

        let unselected = buf.cell((inner.x, inner.y)).unwrap();
        assert_eq!(
            unselected.symbol(),
            GUTTER,
            "other rows keep an aligned gutter"
        );
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

        let inner = app.layout.checkouts.inner;
        let cell = buf.cell((inner.x + 1, inner.y)).unwrap();
        assert_eq!(
            cell.bg, th.sel_bg_dim,
            "you should still see where you were"
        );
    }

    #[test]
    fn an_empty_column_explains_itself_instead_of_going_blank() {
        let mut app = app_with_tree();
        app.sel_checkout = 1; // the worktree with no panes
        app.focus = Focus::Panes;
        let text = lines(&draw(&mut app)).join("\n");
        assert!(text.contains("nothing running"), "{text}");
        assert!(
            text.contains("shell"),
            "an empty panes column says what to press:
{text}"
        );
    }

    #[test]
    fn the_live_view_titles_itself_with_the_path_through_the_tree() {
        let app = app_with_tree();
        assert_eq!(content_title(&app), "argus › orion › master › claude");
    }

    #[test]
    fn the_status_bar_shows_the_keymap_and_swaps_it_inside_a_pane() {
        let mut app = app_with_tree();
        let nav = lines(&draw(&mut app)).join("\n");
        assert!(nav.contains("q detach"), "nav keymap");

        app.focus = Focus::PaneContent;
        let typing = lines(&draw(&mut app)).join("\n");
        assert!(
            typing.contains("typing"),
            "a pane's keys are the child's, and it should say so"
        );
        assert!(
            !typing.contains("q detach"),
            "the nav keymap would be a lie here"
        );
    }

    #[test]
    fn a_pending_leader_chord_is_announced() {
        let mut app = app_with_tree();
        app.focus = Focus::PaneContent;
        app.leader_pending = true;
        let text = lines(&draw(&mut app)).join("\n");
        assert!(
            text.contains("leader"),
            "a half-entered chord must be visible"
        );
    }

    #[test]
    fn an_error_takes_over_the_status_bar_from_the_breadcrumb() {
        let mut app = app_with_tree();
        app.on_server_msg(argus_protocol::ServerMsg::Error {
            message: "git worktree add failed".to_string(),
        });
        let text = lines(&draw(&mut app)).join("\n");
        assert!(
            text.contains("git worktree add failed"),
            "errors must be read, not buried"
        );
    }

    #[test]
    fn an_error_hands_the_bar_back_once_a_key_acknowledges_it() {
        let mut app = app_with_tree();
        app.on_server_msg(argus_protocol::ServerMsg::Error {
            message: "git worktree add failed".to_string(),
        });
        app.on_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('j'),
            crossterm::event::KeyModifiers::NONE,
        ));
        let bar = bar(&draw(&mut app));
        assert!(
            !bar.contains("git worktree add failed"),
            "a read error must not outlive the key that acknowledges it:
{bar}"
        );
        assert!(
            bar.trim_start().starts_with("projects"),
            "the breadcrumb gets its seat back:
{bar}"
        );
    }

    #[test]
    fn a_fresh_bar_shows_the_breadcrumb_not_a_message() {
        let mut app = app_with_tree();
        let bar = bar(&draw(&mut app));
        assert!(
            bar.trim_start().starts_with("projects"),
            "nothing has happened yet, so the seat is the breadcrumb's:
{bar}"
        );
    }

    #[test]
    fn an_ordinary_report_takes_the_breadcrumbs_seat_too() {
        // Feedback the user asked for by pressing something — it is no use
        // to anyone if only errors are allowed on screen.
        let mut app = app_with_tree();
        app.report("4 changed vs staged");
        let bar = bar(&draw(&mut app));
        assert!(bar.contains("4 changed vs staged"), "{bar}");
    }

    #[test]
    fn only_an_alarm_is_colored_like_one() {
        let th = Theme::default();
        let mut app = app_with_tree();

        app.report("4 changed vs staged");
        let buf = draw(&mut app);
        // Status bar is always the second-to-last row (height 20, status bar at row 18).
        let y = buf.area.height - 2;
        assert!(
            (0..buf.area.width).all(|x| buf.cell((x, y)).unwrap().fg != th.err),
            "an ordinary report is news, not an alarm"
        );

        app.alert("error: git worktree add failed");
        let buf = draw(&mut app);
        let y = buf.area.height - 2;
        assert!(
            (0..buf.area.width).any(|x| buf.cell((x, y)).unwrap().fg == th.err),
            "and an error still has to look like one"
        );
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
            (0..buf.area.height)
                .any(|y| (0..buf.area.width).any(|x| buf.cell((x, y)).unwrap().fg == th.err)),
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
        assert!(
            text.contains("enter add"),
            "a prompt should say how to commit it"
        );
    }

    fn comment_prompt(input: &str) -> App {
        let mut app = app_with_tree();
        app.prompt = Some(Prompt::Comment {
            anchor: argus_protocol::ReviewAnchor {
                base: argus_protocol::ReviewBase::Unstaged,
                commit: None,
                path: "crates/argus-client/src/ui.rs".to_string(),
                old_path: None,
                old_start: Some(1013),
                old_end: Some(1013),
                new_start: Some(1013),
                new_end: Some(1013),
                text: vec!["        Prompt::Comment { anchor, input } => (".to_string()],
            },
            input: input.to_string(),
        });
        app
    }

    /// What is inside the prompt box, borders and the columns it floats
    /// over excluded. The box is found rather than assumed: it is centered
    /// and sized to its own content.
    fn box_rows(buf: &ratatui::buffer::Buffer) -> Vec<String> {
        let sym = |x: u16, y: u16| {
            buf.cell((x, y))
                .map(|c| c.symbol().to_string())
                .unwrap_or_default()
        };
        // The panels are drawn first and start higher up, so the last
        // top-left corner on the screen belongs to the modal over them.
        let (x0, y0) = (0..buf.area.height)
            .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
            .rfind(|(x, y)| sym(*x, *y) == "\u{256d}")
            .expect("a prompt box should be drawn");
        let x1 = (x0 + 1..buf.area.width)
            .find(|x| sym(*x, y0) == "\u{256e}")
            .expect("closed on the right");

        (y0 + 1..buf.area.height)
            .take_while(|y| sym(x0, *y) != "\u{2570}")
            .map(|y| {
                (x0 + 1..x1)
                    .map(|x| sym(x, y))
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn the_box_finder_reads_the_modal_and_not_the_columns_behind_it() {
        // Guards the four tests below: they would all pass on a broken
        // finder that returned the whole screen.
        let mut app = comment_prompt("just this");
        let rows = box_rows(&draw(&mut app));
        assert!(
            rows.iter().all(|r| !r.contains("projects")),
            "the column titles are outside the box: {rows:?}"
        );
        assert!(rows.iter().any(|r| r.contains("just this")), "{rows:?}");
    }

    #[test]
    fn a_long_comment_wraps_inside_the_box_instead_of_running_off_it() {
        // The bug: one line, no wrap — past the edge of the box you were
        // typing text you could not read back.
        let sentence = "this loop rebuilds the whole row set on every keystroke, \
                        which is fine at ten files and visibly slow at a thousand";
        let mut app = comment_prompt(sentence);
        let buf = draw(&mut app);

        let rows = box_rows(&buf);
        assert!(rows.len() > 3, "the box grew to fit: {rows:?}");
        let typed: String = rows.join(" ");
        for word in ["rebuilds", "keystroke", "thousand"] {
            assert!(
                typed.contains(word),
                "{word:?} should be readable: {rows:?}"
            );
        }
    }

    #[test]
    fn the_box_never_overruns_the_screen_it_floats_over() {
        let mut app = comment_prompt(&"word ".repeat(200));
        let buf = draw(&mut app);
        // `lines` trims the right edge, so an overrun shows up as a row
        // wider than the terminal or a panic in `draw`.
        assert!(lines(&buf).iter().all(|l| l.chars().count() <= 100));
        assert!(
            lines(&buf).len() <= 20,
            "and it stays a modal rather than becoming the screen"
        );
    }

    #[test]
    fn what_you_are_typing_stays_on_screen_once_the_box_stops_growing() {
        // The tail is kept, not the head: the caret is at the end.
        let mut app = comment_prompt(&format!("{} tailword", "filler ".repeat(300)));
        let buf = draw(&mut app);
        let rows = box_rows(&buf).join(" ");
        assert!(rows.contains("tailword"), "{rows}");
        assert!(rows.contains(CARET), "and the caret with it: {rows}");
    }

    #[test]
    fn the_anchor_gets_its_own_line_rather_than_the_room_to_type_in() {
        let mut app = comment_prompt("make this lazy");
        let buf = draw(&mut app);
        let rows = box_rows(&buf);

        let anchored = rows
            .iter()
            .position(|r| r.contains("ui.rs:1013"))
            .expect("the anchor is shown");
        let typed = rows
            .iter()
            .position(|r| r.contains("make this lazy"))
            .expect("and so is the comment");
        assert!(
            anchored < typed,
            "on separate lines, anchor first: {rows:?}"
        );
    }

    #[test]
    fn a_narrow_terminal_still_gets_a_box_that_fits() {
        let mut app = comment_prompt("nope");
        let buf = draw_at(&mut app, 30, 12);
        assert!(lines(&buf).iter().all(|l| l.chars().count() <= 30));
        assert!(lines(&buf).join("\n").contains("nope"));
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
    /// `cargo test -p argus dump_frame -- --ignored --nocapture`. Beats
    /// launching the client into a real terminal just to see the layout.
    #[test]
    #[ignore = "prints a frame for eyeballing; asserts nothing"]
    fn dump_frame() {
        let mut app = app_with_tree();
        app.focus = Focus::Checkouts;
        app.templates = vec!["claude".to_string()];
        let w = std::env::var("DUMP_W")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100);
        let h = std::env::var("DUMP_H")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(20);
        for line in lines(&draw_at(&mut app, w, h)) {
            println!("|{line}");
        }
    }

    /// Same idea as `dump_frame`, for the two views it doesn't reach.
    #[test]
    #[ignore]
    fn dump_review() {
        let mut app = app_with_review();
        for line in lines(&draw_at(&mut app, 100, 20)) {
            println!("|{line}");
        }
    }

    #[test]
    #[ignore]
    fn dump_split_review() {
        let mut app = app_with_diff(
            true,
            vec![
                diff_line(
                    argus_protocol::LineKind::Context,
                    Some(9),
                    Some(9),
                    "fn f() {",
                ),
                diff_line(
                    argus_protocol::LineKind::Removed,
                    Some(10),
                    None,
                    "    let x = old();",
                ),
                diff_line(
                    argus_protocol::LineKind::Removed,
                    Some(11),
                    None,
                    "    drop(x);",
                ),
                diff_line(
                    argus_protocol::LineKind::Added,
                    None,
                    Some(10),
                    "    let x = new();",
                ),
                diff_line(argus_protocol::LineKind::Context, Some(12), Some(11), "}"),
            ],
        );
        for line in lines(&draw_at(&mut app, 120, 20)) {
            println!("|{line}");
        }
    }

    // --- the directory browser ----------------------------------------------

    fn app_browsing() -> App {
        let mut app = app_with_tree();
        let mut picker = crate::dirpicker::DirPicker::new(crate::dirpicker::DirTarget::Project, 1);
        picker.show(argus_protocol::DirListing {
            request_id: 1,
            path: "/home/u/Source/github.com".to_string(),
            parent: Some("/home/u/Source".to_string()),
            entries: [("argus", true), ("notes", false), ("orion", true)]
                .iter()
                .map(|(name, is_repo)| argus_protocol::DirEntry {
                    name: name.to_string(),
                    is_repo: *is_repo,
                })
                .collect(),
            error: None,
        });
        app.dir_picker = Some(picker);
        app
    }

    #[test]
    fn the_browser_shows_where_you_are_and_what_is_under_it() {
        let mut app = app_browsing();
        let rendered = lines(&draw(&mut app)).join("\n");
        assert!(rendered.contains("add project"), "{rendered}");
        assert!(rendered.contains("github.com"), "the breadcrumb");
        assert!(rendered.contains("add this directory"), "{rendered}");
        assert!(rendered.contains("orion"), "{rendered}");
        assert!(rendered.contains("tab open"), "the keys are on screen");
    }

    #[test]
    fn browsing_for_somewhere_to_put_a_repository_says_so() {
        // The same rows, asked a different question: confirming here puts
        // the new repository in this directory rather than adding the
        // directory itself.
        let mut app = app_browsing();
        app.dir_picker.as_mut().unwrap().target =
            crate::dirpicker::DirTarget::NewRepository(ProjectId(1));
        let rendered = lines(&draw(&mut app)).join("\n");
        assert!(rendered.contains("new repository"), "{rendered}");
        assert!(rendered.contains("make it in this directory"), "{rendered}");
        assert!(!rendered.contains("add this directory"), "{rendered}");
        assert!(rendered.contains("enter choose"), "{rendered}");
    }

    #[test]
    fn naming_a_new_repository_shows_where_it_will_land() {
        let mut app = app_with_tree();
        app.prompt = Some(Prompt::NewRepository {
            project: ProjectId(1),
            parent: "/home/u/Source".to_string(),
            input: "thing".to_string(),
        });
        let rendered = lines(&draw(&mut app)).join("\n");
        assert!(rendered.contains("new repository"), "{rendered}");
        assert!(rendered.contains("/home/u/Source/thing"), "{rendered}");
        assert!(
            rendered.contains("empty to use the directory"),
            "{rendered}"
        );
    }

    #[test]
    fn a_new_repository_with_no_name_yet_points_at_the_directory_itself() {
        let mut app = app_with_tree();
        app.prompt = Some(Prompt::NewRepository {
            project: ProjectId(1),
            parent: "/home/u/Source".to_string(),
            input: String::new(),
        });
        let rendered = lines(&draw(&mut app)).join("\n");
        assert!(rendered.contains("/home/u/Source"), "{rendered}");
    }

    #[test]
    fn a_repository_among_the_directories_is_marked() {
        // Which children are already repos is the question the browser
        // exists to answer, and it is invisible from the name.
        let mut app = app_browsing();
        let rendered = lines(&draw(&mut app));
        // Rightmost match: the repositories column behind the modal also
        // has an "orion" on it.
        let row = rendered.iter().rev().find(|r| r.contains("orion")).unwrap();
        assert!(row.contains("git"), "{row}");
        let plain = rendered.iter().find(|r| r.contains("notes")).unwrap();
        assert!(!plain.contains("git"), "{plain}");
    }

    #[test]
    fn typing_narrows_the_browser_to_what_matches() {
        let mut app = app_browsing();
        app.on_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        let rendered = lines(&draw(&mut app)).join("\n");
        assert!(rendered.contains("notes"), "{rendered}");
        assert!(!rendered.contains("argus\n"), "{rendered}");
        assert!(
            !rendered.contains("add this directory"),
            "the row that answers no query steps aside"
        );
    }

    #[test]
    fn an_unreadable_directory_says_so_instead_of_looking_empty() {
        let mut app = app_with_tree();
        let mut picker = crate::dirpicker::DirPicker::new(crate::dirpicker::DirTarget::Project, 1);
        picker.show(argus_protocol::DirListing {
            request_id: 1,
            path: "/root".to_string(),
            parent: Some("/".to_string()),
            entries: Vec::new(),
            error: Some("permission denied".to_string()),
        });
        app.dir_picker = Some(picker);
        let rendered = lines(&draw(&mut app)).join("\n");
        assert!(rendered.contains("permission denied"), "{rendered}");
    }

    #[test]
    fn a_breadcrumb_too_long_for_the_box_keeps_its_end() {
        // The segments nearest the cursor are the ones that say where you
        // are; the drive letter is not.
        let long = "/very/deep".repeat(20);
        assert_eq!(elide_head(&long, 12).chars().next(), Some('\u{2026}'));
        assert!(elide_head(&long, 12).ends_with("very/deep"));
        assert_eq!(elide_head("/short", 12), "/short");
    }

    #[test]
    #[ignore]
    fn dump_dir_picker() {
        let mut app = app_browsing();
        app.theme = Theme::default();
        for line in lines(&draw_at(&mut app, 100, 20)) {
            println!("|{line}");
        }
    }

    #[test]
    #[ignore]
    fn dump_picker() {
        let mut app = app_with_tree();
        app.theme = Theme::default();
        let mut p = crate::app::Picker::new(
            PickerKind::Branch {
                checkout: CheckoutId(10),
            },
            "switch branch",
            ["feature/login", "feature/logout", "hotfix", "release/2.1"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            0,
        );
        p.type_query("log");
        app.picker = Some(p);

        for line in lines(&draw_at(&mut app, 100, 20)) {
            println!("|{line}");
        }
    }

    #[test]
    #[ignore]
    fn dump_settings() {
        let mut app = app_with_tree();
        app.open_settings();
        for line in lines(&draw_at(&mut app, 100, 20)) {
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
        assert!(
            text.contains("no projects"),
            "a first run should say the tree is empty"
        );
        assert!(text.contains("add"), "and how to start:\n{text}");
    }
    // --- review viewer ------------------------------------------------------

    fn app_with_review() -> App {
        app_with_review_split(false)
    }

    fn app_with_review_split(split: bool) -> App {
        app_with_diff(
            split,
            vec![
                diff_line(
                    argus_protocol::LineKind::Context,
                    Some(10),
                    Some(10),
                    "unchanged",
                ),
                diff_line(argus_protocol::LineKind::Removed, Some(11), None, "gone"),
                diff_line(argus_protocol::LineKind::Added, None, Some(11), "arrived"),
            ],
        )
    }

    fn diff_line(
        kind: argus_protocol::LineKind,
        old_lineno: Option<u32>,
        new_lineno: Option<u32>,
        text: &str,
    ) -> argus_protocol::DiffLine {
        argus_protocol::DiffLine {
            kind,
            old_lineno,
            new_lineno,
            text: text.to_string(),
            spans: Vec::new(),
        }
    }

    fn app_with_diff(split: bool, lines: Vec<argus_protocol::DiffLine>) -> App {
        let mut app = app_with_tree();
        app.review_split = split;
        app.review = Some(crate::review::ReviewView::new(
            argus_protocol::Review {
                request_id: 1,
                checkout: CheckoutId(1),
                base: argus_protocol::ReviewBase::Unstaged,
                files: vec![argus_protocol::FileDiff {
                    path: "src/thing.rs".to_string(),
                    old_path: None,
                    kind: argus_protocol::ChangeKind::Modified,
                    hunks: vec![argus_protocol::Hunk {
                        header: "@@ -10,3 +10,3 @@ fn f()".to_string(),
                        lines,
                    }],
                    note: None,
                }],
                commit: None,
            },
            split,
        ));
        app.overlay = Some(Overlay::Review);
        app.focus = Focus::Review;
        app
    }

    /// As the overlay first opens: headers, and no commit summarized yet.
    fn app_with_history() -> App {
        let mut app = app_with_tree();
        app.history = Some(crate::history::HistoryView::new(
            CheckoutId(1),
            vec![argus_protocol::CommitInfo {
                oid: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                short: "aaaaaaa".into(),
                summary: "Wake a pane's pump on the byte".into(),
                author: "hunt".into(),
                time: 0,
            }],
        ));
        app.overlay = Some(Overlay::History);
        app.focus = Focus::Review;
        app
    }

    /// The same overlay after the cursor has drilled into that commit and
    /// the daemon has answered with what it touched.
    fn app_with_drilled_history() -> App {
        let mut app = app_with_history();
        let view = app.history.as_mut().unwrap();
        view.drill();
        view.receive_files(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            vec![
                argus_protocol::CommitFile {
                    path: "crates/argusd/src/pty.rs".into(),
                    old_path: None,
                    kind: argus_protocol::ChangeKind::Modified,
                    added: 12,
                    removed: 3,
                },
                argus_protocol::CommitFile {
                    path: "DESIGN.md".into(),
                    old_path: None,
                    kind: argus_protocol::ChangeKind::Added,
                    added: 40,
                    removed: 0,
                },
            ],
        );
        app
    }

    #[test]
    fn the_review_never_hides_the_nav_columns() {
        // The standing rule for every view Argus has: one thing opening
        // must not take the others off screen.
        let mut app = app_with_review();
        let out = lines(&draw(&mut app)).join("\n");
        assert!(out.contains("argus"), "the project is still listed:\n{out}");
        assert!(
            out.contains("src/thing.rs"),
            "and the diff is up too:\n{out}"
        );
    }

    #[test]
    fn the_history_never_hides_the_nav_columns() {
        // Same standing rule as the review: opening the log must not take
        // the tree off screen.
        let mut app = app_with_history();
        let out = lines(&draw(&mut app)).join(
            "
",
        );
        assert!(
            out.contains("argus"),
            "the project is still listed:
{out}"
        );
    }

    #[test]
    fn a_commit_row_shows_its_short_hash_summary_and_author() {
        let mut app = app_with_history();
        let out = lines(&draw(&mut app)).join(
            "
",
        );
        assert!(
            out.contains("aaaaaaa"),
            "the short hash:
{out}"
        );
        assert!(
            out.contains("Wake a pane's pump"),
            "the subject:
{out}"
        );
        assert!(
            out.contains("hunt"),
            "the author:
{out}"
        );
    }

    #[test]
    fn the_files_a_commit_touched_are_listed_once_it_is_drilled_into() {
        let mut app = app_with_drilled_history();
        let out = lines(&draw(&mut app)).join(
            "
",
        );
        assert!(out.contains("crates/argusd/src/pty.rs"), "{out}");
        assert!(out.contains("DESIGN.md"), "{out}");
    }

    /// The reason the overlay opens fast at all: a hundred commits cost a
    /// hundred diffs to summarize, so a fresh list shows none of them.
    #[test]
    fn a_fresh_history_lists_no_files_and_marks_the_commit_as_foldable() {
        let mut app = app_with_history();
        let out = lines(&draw(&mut app)).join(
            "
",
        );
        assert!(!out.contains("crates/argusd/src/pty.rs"), "{out}");
        assert!(
            out.contains("▸"),
            "the header says there is something to open:
{out}"
        );
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
        assert!(
            out.contains("@@ -10,3 +10,3 @@"),
            "the hunk header too:\n{out}"
        );
    }

    #[test]
    fn added_and_removed_lines_are_told_apart_by_color() {
        // The markers alone are one character wide; color is what makes a
        // diff scannable. The marker keeps its own colour because a
        // terminal that drops backgrounds leaves nothing else.
        let mut app = app_with_review();
        let buf = draw(&mut app);
        let th = app.theme;
        assert_eq!(fg_of(&buf, "+arrived"), Some(th.ok));
        assert_eq!(fg_of(&buf, "-gone"), Some(th.err));
    }

    /// The foreground now belongs to syntax, so which side a line is on is
    /// carried by a background wash that does not depend on the selection.
    #[test]
    fn added_and_removed_lines_are_washed_whether_or_not_they_are_selected() {
        let mut app = app_with_review();
        let th = app.theme;
        let buf = draw(&mut app);
        assert_eq!(bg_of(&buf, "+arrived"), Some(th.add_bg));
        assert_eq!(bg_of(&buf, "-gone"), Some(th.del_bg));
        assert_ne!(
            bg_of(&buf, "unchanged"),
            Some(th.add_bg),
            "context is not washed"
        );
    }

    #[test]
    fn the_selected_line_is_washed_so_a_range_reads_as_one_block() {
        let mut app = app_with_review();
        let th = app.theme;
        app.review.as_mut().unwrap().toggle_mark();
        app.review.as_mut().unwrap().move_by(1);
        let buf = draw(&mut app);
        assert_eq!(bg_of(&buf, "unchanged"), Some(th.sel_bg));
        // Selecting a removed line brightens its wash rather than replacing
        // it: a selected range must still show which side each line was on.
        assert_eq!(bg_of(&buf, "-gone"), Some(th.del_bg_sel), "the whole range");
        assert_eq!(bg_of(&buf, "+arrived"), Some(th.add_bg), "but no further");
    }

    /// End to end from the wire: a span the daemon sent reaches the screen as
    /// its theme colour, and the text around it stays plain.
    #[test]
    fn a_highlight_span_colours_only_the_run_it_covers() {
        let mut app = app_with_review();
        {
            let view = app.review.as_mut().unwrap();
            let line = &mut view.review.files[0].hunks[0].lines[0];
            line.text = "let x = 1".to_string();
            line.spans = vec![argus_protocol::HighlightSpan {
                start: 0,
                end: 3,
                kind: argus_protocol::HighlightKind::Keyword,
            }];
        }
        let th = app.theme;
        let buf = draw(&mut app);
        assert_eq!(fg_of(&buf, "let"), Some(th.syntax.keyword));
        assert_eq!(
            fg_of(&buf, "x = 1"),
            Some(th.text),
            "uncovered text stays plain"
        );
    }

    /// Offsets cross a process boundary, so the renderer treats them as
    /// untrusted. A span past the end of the line must not panic or colour
    /// anything, and the line must still draw.
    #[test]
    fn a_span_that_overruns_its_line_is_dropped_rather_than_sliced() {
        let mut app = app_with_review();
        {
            let view = app.review.as_mut().unwrap();
            let line = &mut view.review.files[0].hunks[0].lines[0];
            line.text = "short".to_string();
            line.spans = vec![argus_protocol::HighlightSpan {
                start: 2,
                end: 99,
                kind: argus_protocol::HighlightKind::Keyword,
            }];
        }
        let th = app.theme;
        let buf = draw(&mut app);
        assert_eq!(fg_of(&buf, "short"), Some(th.text));
    }

    #[test]
    fn a_diff_taller_than_the_column_scrolls_to_keep_the_cursor_visible() {
        let mut app = app_with_review();
        app.review.as_mut().unwrap().bottom_of_diff();
        let out = lines(&draw_at(&mut app, 100, 10)).join("\n");
        assert!(
            out.contains("+arrived"),
            "the cursor's line is on screen:\n{out}"
        );
    }

    fn fg_of(buf: &ratatui::buffer::Buffer, needle: &str) -> Option<Color> {
        cell_at(buf, needle).map(|c| c.fg)
    }

    fn bg_of(buf: &ratatui::buffer::Buffer, needle: &str) -> Option<Color> {
        cell_at(buf, needle).map(|c| c.bg)
    }

    fn cell_at<'a>(
        buf: &'a ratatui::buffer::Buffer,
        needle: &str,
    ) -> Option<&'a ratatui::buffer::Cell> {
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

    // --- the split view -----------------------------------------------------

    #[test]
    fn a_split_row_draws_both_sides_of_one_change_on_the_same_line() {
        let mut app = app_with_review_split(true);
        let out = lines(&draw(&mut app));
        assert!(
            out.iter()
                .any(|l| l.contains("-gone") && l.contains("+arrived")),
            "the removal and its replacement share a row:
{}",
            out.join(
                "
"
            )
        );
    }

    #[test]
    fn the_unified_view_still_stacks_them() {
        // The whole point of the toggle is that this is the other shape.
        let mut app = app_with_review();
        let out = lines(&draw(&mut app));
        assert!(
            !out.iter()
                .any(|l| l.contains("-gone") && l.contains("+arrived")),
            "{}",
            out.join(
                "
"
            )
        );
    }

    #[test]
    fn each_side_of_a_split_row_keeps_its_own_wash() {
        // One row, two sides: a single line-wide background would say the
        // whole row was added, or removed, and it is both.
        let mut app = app_with_review_split(true);
        let th = app.theme;
        let buf = draw(&mut app);
        assert_eq!(bg_of(&buf, "-gone"), Some(th.del_bg));
        assert_eq!(bg_of(&buf, "+arrived"), Some(th.add_bg));
    }

    #[test]
    fn a_split_row_numbers_each_side_from_its_own_file() {
        // The reason a split view can show both numbers at all: the left is
        // read from the old file, the right from the new one.
        let mut app = app_with_diff(
            true,
            vec![
                diff_line(argus_protocol::LineKind::Removed, Some(11), None, "gone"),
                diff_line(argus_protocol::LineKind::Added, None, Some(207), "arrived"),
            ],
        );
        let buf = draw(&mut app);
        let y = row_of(&buf, "-gone").expect("the removed line is drawn");
        // Inside the overlay only: the panel borders down the screen are
        // the same glyph as the divider.
        let row = row_text(&buf, y, app.layout.overlay.inner);
        let (left, right) = row.split_once('│').expect("a divider between the sides");
        assert!(left.contains("11"), "the old number on the left: {left:?}");
        assert!(
            right.contains("207"),
            "the new number on the right: {right:?}"
        );
    }

    #[test]
    fn a_side_with_nothing_opposite_it_is_recessed_rather_than_blank() {
        // An added line with no removal against it leaves half the row
        // empty; dropping it a step in elevation says "nothing here"
        // instead of "an empty line of code".
        let mut app = app_with_diff(
            true,
            vec![diff_line(
                argus_protocol::LineKind::Added,
                None,
                Some(1),
                "solo",
            )],
        );
        let th = app.theme;
        let buf = draw(&mut app);
        let y = row_of(&buf, "+solo").expect("the added line is drawn");
        assert_eq!(
            buf.cell((app.layout.overlay.inner.x, y)).map(|c| c.bg),
            Some(th.surface),
            "the empty left side"
        );
    }

    #[test]
    fn a_long_line_is_cut_at_its_own_half_rather_than_over_the_divider() {
        // One side overrunning would push the other off the row, and the
        // columns would stop lining up down the screen.
        let mut app = app_with_diff(
            true,
            vec![
                diff_line(
                    argus_protocol::LineKind::Removed,
                    Some(1),
                    None,
                    &"x".repeat(400),
                ),
                diff_line(argus_protocol::LineKind::Added, None, Some(1), "short"),
            ],
        );
        let out = lines(&draw(&mut app));
        assert!(
            out.iter().any(|l| l.contains("+short")),
            "the far side survives the long one:
{}",
            out.join(
                "
"
            )
        );
    }

    fn row_text(buf: &ratatui::buffer::Buffer, y: u16, area: Rect) -> String {
        (area.x..area.x + area.width)
            .filter_map(|x| buf.cell((x, y)).map(|c| c.symbol()))
            .collect()
    }

    fn row_of(buf: &ratatui::buffer::Buffer, needle: &str) -> Option<u16> {
        let needle: Vec<&str> = needle.split("").filter(|s| !s.is_empty()).collect();
        (0..buf.area.height).find(|&y| {
            let row: Vec<&str> = (0..buf.area.width)
                .map(|x| buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "))
                .collect();
            row.windows(needle.len()).any(|w| w == needle)
        })
    }

    #[test]
    fn the_status_bar_offers_the_reviews_own_keys_while_it_is_up() {
        let mut app = app_with_review();
        let out = lines(&draw(&mut app)).join("\n");
        assert!(out.contains("c comment"), "{out}");
        assert!(out.contains("b staged/unstaged"), "{out}");
        assert!(!out.contains("s shell"), "not the tree keymap:\n{out}");
    }

    #[test]
    fn the_tree_keymap_advertises_review() {
        let mut app = app_with_tree();
        app.focus = Focus::Checkouts;
        let out = lines(&draw(&mut app)).join("\n");
        assert!(out.contains("R review"), "{out}");
    }

    // --- the fuzzy picker ---------------------------------------------------

    fn app_with_branch_picker(query: &str) -> App {
        let mut app = app_with_tree();
        let mut p = crate::app::Picker::new(
            PickerKind::Branch {
                checkout: CheckoutId(10),
            },
            "switch branch",
            ["feature/login", "feature/logout", "hotfix"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            0,
        );
        p.type_query(query);
        app.picker = Some(p);
        app
    }

    #[test]
    fn a_fuzzy_picker_shows_what_has_been_typed() {
        let mut app = app_with_branch_picker("log");
        let out = lines(&draw(&mut app)).join("\n");
        assert!(out.contains("› log"), "the query line:\n{out}");
    }

    #[test]
    fn an_empty_query_says_what_the_line_is_for() {
        let mut app = app_with_branch_picker("");
        let out = lines(&draw(&mut app)).join("\n");
        assert!(out.contains("type to filter"), "{out}");
    }

    #[test]
    fn only_the_matching_rows_are_drawn() {
        let mut app = app_with_branch_picker("log");
        let out = lines(&draw(&mut app)).join("\n");
        assert!(out.contains("feature/login"), "{out}");
        assert!(
            !out.contains("hotfix"),
            "a non-match should be gone:\n{out}"
        );
    }

    #[test]
    fn the_create_row_is_visible_rather_than_implied() {
        // Creating a branch by pressing Enter on nothing would be a
        // surprise; it gets a row you can see and aim at.
        let mut app = app_with_branch_picker("brand-new");
        let out = lines(&draw(&mut app)).join("\n");
        assert!(out.contains("create brand-new"), "{out}");
    }

    #[test]
    fn a_long_list_scrolls_instead_of_filling_the_screen() {
        let mut app = app_with_tree();
        let items: Vec<String> = (0..200).map(|i| format!("branch-{i:03}")).collect();
        let mut p = crate::app::Picker::new(
            PickerKind::Branch {
                checkout: CheckoutId(10),
            },
            "switch branch",
            items,
            0,
        );
        p.type_query("");
        app.picker = Some(p);

        let buf = draw_at(&mut app, 100, 24);
        let out = lines(&buf).join("\n");
        assert!(out.contains("branch-000"), "{out}");
        assert!(
            !out.contains("branch-199"),
            "the box must stay a modal:\n{out}"
        );
    }

    #[test]
    fn the_short_pickers_keep_their_plain_list() {
        // A query line over four theme names would be clutter.
        let mut app = app_with_tree();
        app.picker = Some(crate::app::Picker::new(
            PickerKind::Theme,
            "theme",
            crate::theme::THEMES.iter().map(|t| t.to_string()).collect(),
            1,
        ));
        let out = lines(&draw(&mut app)).join("\n");
        assert!(!out.contains("type to filter"), "{out}");
        assert!(out.contains("mocha"), "{out}");
    }

    #[test]
    fn only_a_cyclable_setting_shows_the_arrows() {
        // Arrows on free text would promise a carousel that isn't there.
        let mut app = app_with_tree();
        app.open_settings();
        let out = lines(&draw(&mut app)).join("\n");
        assert!(out.contains("‹ floating window ›"), "{out}");
        assert!(
            !out.contains("‹ (from"),
            "the command row is typed, not cycled:\n{out}"
        );
    }

    #[test]
    fn the_settings_panel_says_where_it_saves() {
        let mut app = app_with_tree();
        app.open_settings();
        let out = lines(&draw(&mut app)).join("\n");
        assert!(out.contains("client.toml"), "{out}");
    }

    #[test]
    fn an_editor_never_appears_in_the_panes_column() {
        let mut app = app_with_tree();
        if let Some(c) = app.tree[0].repositories[0].checkouts.get_mut(0) {
            c.panes.push(PaneInfo {
                id: PaneId(700),
                kind: PaneKind::Editor,
                title: "zzz-editor.rs".to_string(),
                status: PaneStatus::Idle,
                note: None,
                template: None,
                children: Vec::new(),
            });
        }
        let out = lines(&draw(&mut app)).join("\n");
        assert!(!out.contains("zzz-editor"), "editors are not panes:\n{out}");
        assert!(out.contains("claude"), "the agent still is:\n{out}");
    }

    #[test]
    fn collapsed_projects_column_renders_as_a_tab() {
        let mut app = app_with_tree();
        app.projects_collapsed = true;
        let buf = draw(&mut app);

        // The tab occupies the left page gutter: one cell wide, the column
        // band tall (so the whole edge is clickable), sitting on x = 0.
        assert_eq!(app.layout.projects.outer.width, 1);
        assert_eq!(app.layout.projects.outer.x, 0);
        assert_eq!(app.layout.repositories.outer.x, 1);

        // The other columns absorb the freed space; the live view is widest.
        assert!(app.layout.repositories.outer.width > 10);
        assert!(app.layout.checkouts.outer.width > 10);
        assert!(app.layout.panes.outer.width > 10);
        assert!(app.layout.content.outer.width > 20);

        let all = lines(&buf);
        let tab_y = app.layout.projects.outer.y as usize;
        let first = |line: &str| line.chars().next().unwrap_or(' ');
        assert_eq!(first(&all[tab_y]), '▸', "disclosure on the tab row");

        // Below the mark the gutter is empty page, not a bordered rail.
        let gutter = app.layout.projects.outer;
        for y in gutter.y..gutter.y.saturating_add(gutter.height) {
            let ch = first(&all[y as usize]);
            if y as usize == tab_y {
                continue;
            }
            assert_ne!(ch, '│', "full-height rail at row {y}");
            assert_ne!(ch, '╭', "card border in the gutter at row {y}");
            assert_ne!(ch, '╰', "card border in the gutter at row {y}");
            assert_eq!(ch, ' ', "gutter below the tab should be empty at row {y}");
        }

        let text = all.join("\n");
        assert!(
            text.contains("argus"),
            "project still named in the breadcrumb"
        );
        assert!(
            !all.iter().any(|line| line.starts_with("argus")),
            "project name must not occupy the gutter"
        );
    }

    #[test]
    fn collapsed_constraints_cede_the_projects_width() {
        // With captured widths, the four survivors keep them and the slack
        // lands in the content column, same as dragging a gutter.
        let total = 100;
        let preferred = Some(vec![12u16, 18, 20, 20, 40]);
        let c = collapsed_projects_constraints(total, preferred.as_deref());
        match c.as_slice() {
            [Constraint::Length(18), Constraint::Length(20), Constraint::Length(20), Constraint::Length(39)] =>
                {}
            other => panic!("unexpected collapsed constraints: {other:?}"),
        }
    }

    #[test]
    fn collapsed_constraints_default_redeals_over_four_columns() {
        let total = 100;
        let c = collapsed_projects_constraints(total, None);
        assert_eq!(c.len(), 4);
        let sum: u16 = c
            .iter()
            .map(|c| match c {
                Constraint::Length(w) => *w,
                _ => panic!("all lengths"),
            })
            .sum();
        assert_eq!(sum + 3, total, "4 columns + 3 gutters = total");
    }
}
