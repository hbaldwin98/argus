//! Where the last frame put things, and which of them has focus.
//!
//! The renderer records the regions it drew so a click can be mapped back
//! onto the row it landed on without either side redoing the layout
//! arithmetic.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Projects,
    Repositories,
    Checkouts,
    Panes,
    PaneContent,
    /// A mode of the rightmost column, not a fifth column of its own.
    Review,
    /// A floating window over everything else — see [`Overlay`].
    Overlay,
    /// The open view, when it is not the workspace. One variant for all of
    /// them: a view owns the whole stage, so there is never a second thing
    /// on it for focus to pick between.
    View,
}

/// One rendered panel: the whole card, and the padded area its rows live
/// in. Both are needed — a click on a row selects it, but a click anywhere
/// else on the card still moves focus there.
#[derive(Debug, Clone, Copy, Default)]
pub struct Panel {
    pub outer: Rect,
    pub inner: Rect,
    /// The list index drawn on this card's first row. A column taller than
    /// its card is scrolled, and then a row on screen is not the row's
    /// index: everything mapping a click back to a row has to add this,
    /// and the next frame scrolls from here rather than recomputing an
    /// offset from the selection alone.
    pub first: usize,
}

/// Screen regions from the most recent render, so mouse clicks can be
/// mapped back onto tree rows / pane cells without duplicating layout math.
#[derive(Debug, Clone, Copy, Default)]
pub struct Layout {
    /// The contextual rail down the left edge: the project, its
    /// repositories and their checkouts and panes.
    pub rail: Panel,
    /// The Checkouts stage's table.
    pub checkouts: Panel,
    /// The Panes stage's cards.
    pub panes: Panel,
    /// The selected pane's terminal: the Workspace stage, or the whole
    /// content area while the pane is fullscreen.
    pub terminal: Panel,
    /// The feature view's four panels: the list of features, and the
    /// selected feature's brief, tasks, and decision tree. All zero-sized
    /// while another stage is open, so a click cannot land on a panel that is
    /// not drawn.
    pub features: Panel,
    pub feature_brief: Panel,
    pub feature_tasks: Panel,
    pub feature_diagrams: Panel,
    pub feature_decisions: Panel,
    /// The row of view tabs above everything else. Zero-sized on a
    /// terminal too short to spend a row on it.
    pub views: Panel,
    /// Zero-sized when no overlay is up.
    pub overlay: Panel,
    /// The keymap window, zero-sized when it is not up.
    pub help: Panel,
    /// The command center's AGENTS list in the rail.
    pub agents: Panel,
    /// Where the last frame put the hardware cursor, `None` when it hid it.
    /// Recorded as well as applied so the decision — which is one decision
    /// for the whole frame, made across several layers — can be asserted on.
    pub cursor: Option<crate::ui::CursorPlacement>,
}

pub(super) fn in_rect(area: Rect, x: u16, y: u16) -> bool {
    x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height
}
