//! The screens the window has: one per side the session draws a pane on, and the one selection
//! across them (SHELL-SCREEN-1, -4).
//!
//! The Workspace's and the console's two sides are always here. A split cell or an extension's
//! console view has a screen only while a view of that side is alive (`bind`, `unbind`), as the
//! page makes one per screen it builds; a frame for a side with no screen is dropped, and the
//! session sends the screen whole when the side is bound again. A side is counted, not flagged: the
//! session gives a freed side to the next view, and GPUI releases the old view after the new one
//! has bound it, so the screen goes only with the last view of it.
//!
//! There is at most one selection, since a press anywhere clears every other (SHELL-INPUT-5), and
//! it goes when its side shows another pane's frame.

use std::sync::Arc;

use argus_desktop_core::connection::page::{Frame, Side};

use crate::panes::screen::Screen;
use crate::panes::selection::Selection;

/// The side a selection is on, and its two ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Marked {
    pub side: Side,
    pub selection: Selection,
}

pub struct Screens {
    /// Each side's screen and how many views hold it; the three always there are never counted down.
    held: Vec<(Side, usize, Arc<Screen>)>,
    /// What a side with no screen reads as.
    none: Arc<Screen>,
    marked: Option<Marked>,
}

impl Screens {
    pub fn new() -> Self {
        let held = [Side::Workspace, Side::First, Side::Second].map(|side| (side, 0, Arc::new(Screen::new()))).to_vec();
        Self { held, none: Arc::new(Screen::new()), marked: None }
    }

    /// Gives `side` a screen if it has none. One that has is left as it is, with one more view of it.
    pub fn bind(&mut self, side: Side) {
        match self.held.iter_mut().find(|(held, ..)| *held == side) {
            Some((_, views, _)) => *views += 1,
            None => self.held.push((side, 1, Arc::new(Screen::new()))),
        }
    }

    /// One view of `side` went: with the last, its screen and the selection on it go too. The three
    /// the window always has stay. Whether a selection went with it.
    pub fn unbind(&mut self, side: Side) -> bool {
        if matches!(side, Side::Workspace | Side::First | Side::Second) {
            return false;
        }
        let Some(at) = self.held.iter().position(|(held, ..)| *held == side) else { return false };
        self.held[at].1 -= 1;
        if self.held[at].1 > 0 {
            return false;
        }
        self.held.remove(at);
        self.mark_off(side)
    }

    /// The screen of `side`; an empty one for a side with none.
    pub fn of(&self, side: Side) -> &Arc<Screen> {
        self.held.iter().find(|(held, ..)| *held == side).map_or(&self.none, |(.., screen)| screen)
    }

    /// Applies `frame` to its side's screen; the side, when it has one.
    pub fn apply(&mut self, frame: Frame) -> Option<Side> {
        let side = frame.side;
        let at = self.held.iter().position(|(held, ..)| *held == side)?;
        // A different pane's rows are not the old pane's: what was selected is gone (SHELL-SCREEN-4).
        if self.held[at].2.pane() != Some(frame.pane) {
            self.mark_off(side);
        }
        Arc::make_mut(&mut self.held[at].2).apply(frame);
        Some(side)
    }

    pub fn marked(&self) -> Option<Marked> {
        self.marked
    }

    /// The selection on `side`.
    pub fn selection_of(&self, side: Side) -> Option<Selection> {
        self.marked.filter(|marked| marked.side == side).map(|marked| marked.selection)
    }

    /// Puts the selection where `marked` says, or nowhere. The sides whose picture changed, the
    /// one it left and the one it came to, each once.
    pub fn mark(&mut self, marked: Option<Marked>) -> Vec<Side> {
        if self.marked == marked {
            return Vec::new();
        }
        let mut sides: Vec<Side> = self.marked.iter().chain(marked.iter()).map(|marked| marked.side).collect();
        sides.dedup();
        self.marked = marked;
        sides
    }

    fn mark_off(&mut self, side: Side) -> bool {
        let on = self.marked.is_some_and(|marked| marked.side == side);
        if on {
            self.marked = None;
        }
        on
    }
}

impl Default for Screens {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
