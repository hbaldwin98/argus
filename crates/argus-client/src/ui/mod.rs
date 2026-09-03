//! The renderer. Its normal spine has five columns: projects, repositories,
//! checkouts, open panes, and the selected pane's live view. A pane can
//! temporarily take the main content area while its terminal has focus.
//!
//! Every color goes through [`crate::theme::Theme`] rather than being named
//! here, and the visual language is deliberately narrow:
//!
//! - **Elevation** carries structure: the page sits at `bg`, an unfocused
//!   panel at `surface`, the focused one at `surface_focus`. The spine's
//!   cards have no box — the fill is what says where one begins, with the
//!   page showing through the gutter as a seam and a rule across the top
//!   carrying the name. A floating window keeps its border, because it has
//!   to cut itself out of what is behind it.
//! - **Focus** is that elevation plus an accent rule and title.
//! - **Selection** is a raised bar with an accent `▌` marker, never reverse
//!   video — reverse fights with the per-row status colors.
//! - **State** is a shape-distinct glyph in the row's status color (§8b),
//!   rolled up to parents by the worst descendant.
//! - **Weight** recedes with focus: names in the column being used are
//!   `text` and bold, and elsewhere they drop to `muted` — except the
//!   selected row, which is the path you are on and stays legible.
//! - **Overflow** is a thumb in the card's right padding cell, so a column
//!   never scrolls without admitting there is more of it.
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
    App, CheckoutRow, Focus, Fold, Overlay, PaneLocation, Panel, PickerKind, Prompt, Setting,
};
use crate::dirpicker::DirRow;
use crate::grid::Grid;
use crate::history::{HistoryRow, HistoryView};
use crate::notes::NoteMode;
use crate::review::{ReviewView, Row};
use crate::theme::Theme;
use argus_protocol::CursorShape;

mod columns;
mod help;
mod history;
mod modals;
mod overlay;
mod review;
mod rows;
mod status;
mod term;
mod text;

use columns::*;
use help::*;
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

/// A column's scroll thumb, drawn in the padding cell beside the border so
/// it reads as part of the card's edge rather than as a row of its own.
const SCROLL_THUMB: &str = "\u{2590}";

/// Every list item is a name line plus a detail line. `app` hit-tests
/// clicks against this, so it is shared rather than local.
pub const ROW_HEIGHT: u16 = 2;

/// The same item on one line, name only. Two lines is what stops a wide
/// column reading as cramped, but on a short terminal it is what makes it
/// cramped: a card with room for five items has to spend that room on
/// which items exist, not on what is true about each.
pub const COMPACT_ROW_HEIGHT: u16 = 1;

/// The card height below which detail lines cost more than they pay for —
/// six two-line items. Measured on the padded inside of a card, so the
/// borders and the top gutter are already out of it.
const COMFORTABLE_MIN_HEIGHT: u16 = 12;

/// How tall a row in the nav columns is, for a card of this inner height.
/// The renderer records the answer in [`crate::app::Layout`] so hit-testing
/// resolves a click against the rows that were actually drawn.
pub fn row_height(inner_height: u16) -> u16 {
    if inner_height < COMFORTABLE_MIN_HEIGHT {
        COMPACT_ROW_HEIGHT
    } else {
        ROW_HEIGHT
    }
}

/// Blank columns between panels, and between the panels and the screen
/// edge. Without it the cards touch and stop reading as separate surfaces.
///
/// Two rather than one since the cards lost their borders: the gutter is
/// the page showing through, and one cell of it is a hairline on a palette
/// whose page and surface are a few points apart.
pub const GUTTER_COLS: u16 = 2;

/// The narrowest a spine of `columns` cards can be drawn without any of
/// them going under its floor. What the fold breakpoints are derived from,
/// so the two cannot drift: the layout folds exactly when the widths it
/// would otherwise have to hand out stop being honest.
pub fn spine_min_width(columns: usize) -> u16 {
    let columns = columns as u16;
    columns.saturating_sub(1) * MIN_COLUMN_WIDTH
        + MIN_CONTENT_WIDTH
        + GUTTER_COLS * columns.saturating_sub(1)
}

/// A dragged column cannot be collapsed beyond this outer width. Below it
/// a card has no room to say anything: two cells go to the border and two
/// to the inner gutter, so eight cells of column were four cells of text —
/// a status glyph, a letter, and an ellipsis. The renderer scales the floor
/// down only when the terminal itself is too narrow to fit the spine.
pub const MIN_COLUMN_WIDTH: u16 = 14;

/// The live view's own floor, which is much larger because what it holds is
/// not a list but somebody's terminal. Width is reclaimed from the nav
/// columns before this is touched: a squeezed column is still readable, and
/// a forty-column pty is already the point at which most programs give up.
pub const MIN_CONTENT_WIDTH: u16 = 40;

/// Folded-away projects: a disclosure mark in the left page gutter, not a
/// full-height rail. The rest of that gutter is the click target.
const COLLAPSED_TAB: &str = "▸";

/// How much of a row has to be left for the name before a badge is worth
/// keeping. Below it the badge is winning space from the only part of the
/// row that says which thing this is.
const NAME_FLOOR: usize = 8;

/// The status glyph and its trailing space, which every row's name begins
/// with and which its detail line therefore hangs under.
const STATUS_WIDTH: usize = 2;

/// One list item: what it is, a dimmer line of what's true about it, and
/// an optional count pinned to the right of the name line. The badge is
/// there because these columns are narrow — a count appended to the detail
/// line is the first thing to get truncated away.
pub struct Item<'a> {
    pub name: Vec<Span<'a>>,
    pub detail: Vec<Span<'a>>,
    pub badge: Vec<Span<'a>>,
    /// How far the detail line is indented, so it starts under the name
    /// rather than under the glyphs in front of it. The width of that glyph
    /// run varies by row — a checkout carries a kind mark the others don't —
    /// and a detail hanging two cells left of its own name is the kind of
    /// raggedness that makes a column look unconsidered.
    pub indent: usize,
}

impl<'a> Item<'a> {
    fn new(name: Vec<Span<'a>>, detail: Vec<Span<'a>>) -> Self {
        Item {
            name,
            detail,
            badge: Vec::new(),
            indent: STATUS_WIDTH,
        }
    }

    fn badged(mut self, badge: Vec<Span<'a>>) -> Self {
        self.badge = badge;
        self
    }

    /// For a row whose name carries more in front of it than the status
    /// glyph — a kind mark, a child's elbow.
    fn indented(mut self, indent: usize) -> Self {
        self.indent = indent;
        self
    }
}

pub fn render(f: &mut Frame, app: &mut App) {
    let th = app.theme;
    // The page owns its background; leaving it `Reset` would inherit
    // whatever the host terminal happens to be, and the elevation between
    // page and panel is what makes the panels read as cards.
    f.render_widget(Block::default().style(Style::default().bg(th.bg)), f.area());

    let page = inset(f.area(), GUTTER_COLS, 1);
    // A resize is noticed here rather than plumbed in as an event, because
    // here is where the answer is used. Folding only ever tightens: a
    // terminal that has grown wide enough for five columns is not a reason
    // to undo a layout the user chose, and `p` is how they change it back.
    if app.layout.width != f.area().width {
        app.layout.width = f.area().width;
        app.fold = app.fold.max(Fold::required(f.area().width));
        if app.fold.hides(app.focus) {
            app.focus = app.fold.first_focus();
        }
    }

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
        app.layout.row_height = ROW_HEIGHT;
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

    // Last, over everything: the keymap is asked for on top of whatever
    // raised the question, and it hands the screen straight back.
    if app.help.is_some() {
        render_help(f, app, root[0], th);
        cursor = None;
    } else {
        app.layout.help = Panel::default();
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
    let block = card_block(&title, focused, th, area.width);
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
