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

/// How many of the leading nav columns are folded away to tabs in the left
/// page gutter, ceding their width to the columns that remain.
///
/// Folding rather than squeezing is what a narrow terminal needs: five
/// cards sharing sixty cells are five things none of which can be read,
/// where three cards sharing the same sixty are three that can. Nothing is
/// unreachable while folded — the live view's title is a full breadcrumb,
/// the flat pane view spells the path out on every row, and `p` brings a
/// column back at any width.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Fold {
    #[default]
    None,
    Projects,
    Repositories,
}

impl Fold {
    pub const ALL: [Fold; 3] = [Fold::None, Fold::Projects, Fold::Repositories];

    /// How many columns the spine draws at this fold, the live view
    /// included.
    pub fn columns(self) -> usize {
        5 - self.hidden()
    }

    /// How many leading nav columns are tabs rather than cards.
    pub fn hidden(self) -> usize {
        match self {
            Fold::None => 0,
            Fold::Projects => 1,
            Fold::Repositories => 2,
        }
    }

    pub fn hides(self, focus: Focus) -> bool {
        match focus {
            Focus::Projects => self >= Fold::Projects,
            Focus::Repositories => self >= Fold::Repositories,
            _ => false,
        }
    }

    /// The leftmost column still on screen, and so where focus goes when
    /// the one it was on folds away.
    pub fn first_focus(self) -> Focus {
        match self {
            Fold::None => Focus::Projects,
            Fold::Projects => Focus::Repositories,
            Fold::Repositories => Focus::Checkouts,
        }
    }

    pub fn cycle(self) -> Fold {
        match self {
            Fold::None => Fold::Projects,
            Fold::Projects => Fold::Repositories,
            Fold::Repositories => Fold::None,
        }
    }

    /// The least folding this width can carry without any column dropping
    /// under its floor. Applied on a resize only, and only ever to fold
    /// further — a width that suddenly fits five columns is not a reason to
    /// undo a layout the user chose.
    pub fn required(width: u16) -> Fold {
        // The page is inset by one on each side before the spine sees it.
        let width = width.saturating_sub(crate::ui::GUTTER_COLS * 2);
        *Fold::ALL
            .iter()
            .find(|fold| width >= crate::ui::spine_min_width(fold.columns()))
            .unwrap_or(&Fold::Repositories)
    }
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
    /// The frame width the last render saw. Kept so a resize can be noticed
    /// where the layout is decided, rather than plumbed in as an event.
    pub width: u16,
    /// How tall a nav row was drawn this frame. A short terminal gets
    /// one-line rows, and a click has to be resolved against the rows on
    /// screen rather than against the roomier ones the code prefers.
    pub row_height: u16,
    pub projects: Panel,
    pub repositories: Panel,
    pub checkouts: Panel,
    pub panes: Panel,
    pub content: Panel,
    /// Zero-sized when no overlay is up.
    pub overlay: Panel,
    /// The keymap window, zero-sized when it is not up.
    pub help: Panel,
    /// Where the last frame put the hardware cursor, `None` when it hid it.
    /// Recorded as well as applied so the decision — which is one decision
    /// for the whole frame, made across several layers — can be asserted on.
    pub cursor: Option<crate::ui::CursorPlacement>,
}

/// Which list row a point falls on. A row is `height` lines tall, and any
/// of its lines counts as that item.
pub(super) fn row_in(area: Rect, height: u16, x: u16, y: u16) -> Option<usize> {
    if !in_rect(area, x, y) {
        return None;
    }
    Some(((y - area.y) / height.max(1)) as usize)
}

pub(super) fn in_rect(area: Rect, x: u16, y: u16) -> bool {
    x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height
}
