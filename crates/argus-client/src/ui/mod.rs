//! The renderer: the command center — the tab strip naming the four
//! stages, the contextual rail, the open stage, and the command band (see
//! [`command_center`]). A pane's terminal can take the whole content area
//! while it has focus, and overlays and modals float over everything.
//!
//! Every color goes through [`crate::theme::Theme`] rather than being named
//! here, and the visual language is deliberately narrow:
//!
//! - **Elevation** carries structure: the page sits at `bg`, an unfocused
//!   card at `surface`, the focused one at `surface_focus`.
//! - **Focus** is that elevation plus an accent border and title.
//! - **Selection** is a raised bar with an accent `▌` marker, never reverse
//!   video — reverse fights with the per-row status colors.
//! - **State** is a shape-distinct glyph in the row's status color (§8b),
//!   rolled up to parents by the most urgent state beneath.
//! - **Overflow** is a thumb beside the rows, so a list never scrolls
//!   without admitting there is more of it.

use argus_protocol::{
    Color as PColor, FileDiff, HighlightKind, HighlightSpan, LineKind, PaneStatus,
};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph, Widget, Wrap};
use ratatui::Frame;

use crate::app::{
    App, CheckoutRow, Focus, Mode, Overlay, OverlayMode, PaneLocation, Panel, PickerKind, Prompt,
    RailRow, Setting, View,
};
use crate::brief::BriefMode;
use crate::dirpicker::DirRow;
use crate::grid::Grid;
use crate::history::{HistoryRow, HistoryView};
use crate::review::{ReviewView, Row};
use crate::theme::Theme;
use argus_protocol::CursorShape;

mod card;
mod command_center;
mod help;
mod history;
mod modals;
mod overlay;
pub(super) mod prose;
mod review;
mod status;
mod term;
mod text;
mod views;

use card::*;
use command_center::*;
use help::*;
use history::*;
use modals::*;
use overlay::*;

use review::*;
use status::*;
use term::*;
use text::*;
use views::*;

pub(crate) use command_center::{
    agent_at as command_center_agent_at, checkout_at as command_center_checkout_at,
    feature_row_at as command_center_feature_row_at,
    pane_at as command_center_pane_at, project_header_at as command_center_project_header_at,
    rail_target_at as command_center_rail_target_at,
    sidebar_contains as command_center_sidebar_contains,
};
pub use term::CursorPlacement;
pub use views::tab_at;

/// The text caret, drawn rather than using the terminal cursor: the
/// cursor belongs to whichever pane is focused.
const CARET: &str = "▏";

/// The selection marker, and the blank gutter every other row gets so text
/// stays aligned whether or not it's selected.
const MARKER: &str = "▌";
const GUTTER: &str = " ";

/// Root decisions have no tree guide; this fills the column tasks use for state.
pub(crate) const DECISION_ROOT_MARK: &str = "◇";

pub(crate) fn decision_title_pad(depth: usize) -> &'static str {
    if depth == 0 {
        "◇ "
    } else {
        "  "
    }
}

/// A column's scroll thumb, drawn in the padding cell beside the border so
/// it reads as part of the card's edge rather than as a row of its own.
const SCROLL_THUMB: &str = "\u{2590}";

/// Every list item is a name line plus a detail line. `app` hit-tests
/// clicks against this, so it is shared rather than local.
pub const ROW_HEIGHT: u16 = 2;

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
}

pub fn render(f: &mut Frame, app: &mut App) {
    let th = app.theme.drawn();
    // The page owns its background; leaving it `Reset` would inherit
    // whatever the host terminal happens to be, and the elevation between
    // page and panel is what makes the panels read as cards.
    f.render_widget(Block::default().style(Style::default().bg(th.bg)), f.area());

    // A row of air above the tabs and below the status band, so the shell
    // does not sit flush against the host terminal's edges.
    let page = inset(f.area(), 0, 1);
    let status_height = 2;
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(status_height)])
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
        // Nothing of the shell is drawn under a fullscreen terminal, tabs
        // included, and a stale rect is a click that lands on something
        // nobody can see.
        app.layout.views = Panel::default();
        forget_rail_and_stages(app);
        forget_feature_view(app);
        render_terminal(f, app, root[0], th)
    } else {
        render_command_center(f, app, root[0], th)
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

/// The feature stage's panels, for the frames that do not draw it: a stale
/// rect there would swallow a click meant for whatever is on screen.
fn forget_feature_view(app: &mut App) {
    app.layout.features = Panel::default();
    app.layout.feature_brief = Panel::default();
    app.layout.feature_tasks = Panel::default();
    app.layout.feature_decisions = Panel::default();
}

/// Zeroes the rail's and the stages' recorded regions, for the frames that
/// draw none of them: a stale rect is a click that selects a row nobody
/// can see.
fn forget_rail_and_stages(app: &mut App) {
    app.layout.rail = Panel::default();
    app.layout.agents = Panel::default();
    app.layout.checkouts = Panel::default();
    app.layout.panes = Panel::default();
}

/// The selected pane's terminal given the whole content area, while it is
/// fullscreen.
fn render_terminal(f: &mut Frame, app: &mut App, area: Rect, th: Theme) -> Option<CursorPlacement> {
    // Typing focus is what the accent border promises here, so only
    // PaneContent lights it up — merely selecting a pane does not.
    let lit = app.focus_lit(Focus::PaneContent);
    let focused = lit.is_lit();
    // A parked pane looks exactly like a quiet one, so the title has to say
    // that the rows on screen are history rather than the current output.
    let title = match app.scroll_indicator() {
        Some(where_) => format!("{where_} · {}", content_title(app)),
        None => content_title(app),
    };
    let block = panel_block(&title, lit, th, area.width);
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
        let pane = app.column_pane();
        let grid = pane.and_then(|id| app.grids.get(&id));
        render_term(f, grid, inner, focused, pane, app.selection.as_ref())
    };
    app.layout.terminal = Panel {
        outer: area,
        inner,
        first: 0,
    };
    cursor
}

/// `project / checkout / pane` for the fullscreen terminal's title, which
/// doubles as the breadcrumb telling you where in the tree it came from.
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
