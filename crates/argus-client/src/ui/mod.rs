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
    NoteCounts, PaneStatus, TodoState,
};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph, Widget, Wrap};
use ratatui::Frame;

use crate::app::{
    App, CheckoutRow, Focus, Overlay, PaneLocation, Panel, PickerKind, Prompt, Setting,
};
use crate::dirpicker::DirRow;
use crate::grid::Grid;
use crate::history::{HistoryRow, HistoryView};
use crate::notes::NoteMode;
use crate::review::{ReviewView, Row};
use crate::theme::Theme;
use argus_protocol::CursorShape;

mod columns;
mod history;
mod modals;
mod overlay;
mod review;
mod rows;
mod status;
mod term;
mod text;

use columns::*;
use history::*;
use modals::*;
use overlay::*;
use review::*;
use rows::*;
use status::*;
use term::*;
use text::*;

pub use rows::pane_row_owners;
pub use term::CursorPlacement;

/// The text caret, drawn rather than using the terminal cursor: the
/// cursor belongs to whichever pane is focused.
const CARET: &str = "▏";

/// The selection marker, and the blank gutter every other row gets so text
/// stays aligned whether or not it's selected.
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

#[cfg(test)]
mod tests;
