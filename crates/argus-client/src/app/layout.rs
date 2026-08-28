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
    pub projects: Panel,
    pub repositories: Panel,
    pub checkouts: Panel,
    pub panes: Panel,
    pub content: Panel,
    /// Zero-sized when no overlay is up.
    pub overlay: Panel,
    /// Where the last frame put the hardware cursor, `None` when it hid it.
    /// Recorded as well as applied so the decision — which is one decision
    /// for the whole frame, made across several layers — can be asserted on.
    pub cursor: Option<crate::ui::CursorPlacement>,
}

/// Which list row a point falls on. Rows are [`crate::ui::ROW_HEIGHT`]
/// lines tall, and either of an item's lines counts as that item.
pub(super) fn row_in(area: Rect, x: u16, y: u16) -> Option<usize> {
    if !in_rect(area, x, y) {
        return None;
    }
    Some(((y - area.y) / crate::ui::ROW_HEIGHT) as usize)
}

pub(super) fn in_rect(area: Rect, x: u16, y: u16) -> bool {
    x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height
}
