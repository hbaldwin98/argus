//! What a pointer over a pane's screen is, with no window in it: the cell under it, a press that
//! is the child's or the start of a selection, the move that is not worth a report, the release
//! that copies, and a wheel counted in notches.
//!
//! This is `panes/input.ts`'s mouse half and `panes/screen.ts`'s `cellAt` as arithmetic, so a
//! window that is not the page asks the same questions and gets the same answers (SHELL-INPUT-5
//! to -10). What the gestures mean to the child, a selection's text and a notch's effect are the
//! session's; this only says which call a pointer event is.

use crate::connection::page::PageMouse;

/// A cell as the calls name it: `(row, col)`.
pub type Cell = (u16, u16);

/// A notch of a physical wheel in pixels (a touchpad's small deltas add up to one).
pub const PIXELS_PER_NOTCH: f32 = 100.0;
/// A notch of a wheel that reports lines.
pub const LINES_PER_NOTCH: f32 = 3.0;

/// The cell under a point `x`, `y` pixels from the grid's top-left corner, held inside a grid of
/// `rows` by `cols` so a drag past an edge selects to the edge. A grid with no cells has one at
/// `(0, 0)`.
pub fn cell_at(x: f32, y: f32, cell_w: f32, cell_h: f32, rows: u16, cols: u16) -> Cell {
    let clamp = |at: f32, cell: f32, most: u16| {
        let n = if cell > 0.0 { (at / cell).floor() } else { 0.0 };
        n.clamp(0.0, f32::from(most.saturating_sub(1))) as u16
    };
    (clamp(y, cell_h, rows), clamp(x, cell_w, cols))
}

/// What a pointer event asks of the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gesture {
    /// The child asked for the mouse: `Call::Mouse`.
    Mouse { gesture: PageMouse, button: u8, at: Cell },
    /// A drag across the pane ended: `Call::Copy`.
    Copy { from: Cell, to: Cell },
}

/// The pointer's state on one screen: a button the child holds, or a selection's anchor.
#[derive(Debug, Default)]
pub struct Pointer {
    anchor: Option<Cell>,
    /// The pointer left the anchor's cell: before that a press and release is a click, not a drag.
    moved: bool,
    /// The button the child holds, if the press was its.
    held: Option<u8>,
    /// The cell last reported, so a move within one is not reported again.
    last: Option<Cell>,
}

impl Pointer {
    fn report(&mut self, gesture: PageMouse, button: u8, at: Cell) -> Gesture {
        self.last = Some(at);
        Gesture::Mouse { gesture, button, at }
    }

    fn moving(&self, at: Cell) -> bool {
        self.last != Some(at)
    }

    /// A button went down on the screen at `at`. `takes_mouse` is the screen's word that the
    /// child asked for the mouse and is not parked in its history. With it, and without Shift,
    /// the press is the child's; otherwise a left press anchors a selection and another button
    /// does nothing (the window still hands the pane the keys, which is not decided here).
    pub fn press(&mut self, button: u8, shift: bool, takes_mouse: bool, at: Cell) -> Option<Gesture> {
        if takes_mouse && !shift {
            self.held = Some(button);
            return Some(self.report(PageMouse::Down, button, at));
        }
        if button != 0 {
            return None;
        }
        self.anchor = Some(at);
        self.moved = false;
        None
    }

    /// The pointer moved with a button down, wherever it is in the window: a drag the child hears
    /// when the cell changed, or the head of a selection.
    pub fn drag(&mut self, at: Cell) -> Option<Gesture> {
        if let Some(button) = self.held {
            return self.moving(at).then(|| self.report(PageMouse::Drag, button, at));
        }
        if let Some(anchor) = self.anchor {
            self.moved |= at != anchor;
        }
        None
    }

    /// The pointer moved over the screen with no button down. Only the screen with the keys
    /// reports it, and only to a child that asked for the mouse, and only when the cell changed.
    pub fn hover(&mut self, at: Cell, takes_mouse: bool, keyed: bool) -> Option<Gesture> {
        if self.anchor.is_some() || self.held.is_some() || !takes_mouse || !keyed {
            return None;
        }
        self.moving(at).then(|| self.report(PageMouse::Move, 0, at))
    }

    /// A button came up at `at`. The child's button ends its press; a selection that really moved
    /// copies; a click copies nothing.
    pub fn release(&mut self, button: u8, at: Cell) -> Option<Gesture> {
        if let Some(held) = self.held {
            if button != held {
                return None;
            }
            self.held = None;
            return Some(self.report(PageMouse::Up, held, at));
        }
        let anchor = self.anchor.take()?;
        std::mem::take(&mut self.moved).then_some(Gesture::Copy { from: anchor, to: at })
    }

    /// Whether a press is being followed, so the window keeps hearing the pointer.
    pub fn active(&self) -> bool {
        self.held.is_some() || self.anchor.is_some()
    }

    /// The selection a drag to `head` shows: nothing until the pointer has left the anchor's cell.
    pub fn selection(&self, head: Cell) -> Option<(Cell, Cell)> {
        self.anchor.filter(|_| self.moved).map(|anchor| (anchor, head))
    }

    /// A press whose release the window never saw: the child's is let go where the pointer is, a
    /// selection ends with no copy.
    pub fn lost(&mut self, at: Cell) -> Option<Gesture> {
        let up = self.held.take().map(|button| self.report(PageMouse::Up, button, at));
        self.cancel();
        up
    }

    /// Lets a press go without a report or a copy, for a release that went elsewhere.
    pub fn cancel(&mut self) {
        self.anchor = None;
        self.moved = false;
        self.held = None;
    }
}

/// What a wheel's delta is measured in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Pixels,
    Lines,
}

/// A wheel counted in notches: small deltas add up, the whole notches go out, and the fraction is
/// carried to the next (SHELL-INPUT-10).
#[derive(Debug, Default)]
pub struct Wheel(f32);

impl Wheel {
    /// Adds `delta` (positive scrolls down, as the page's `deltaY`) and returns the whole notches
    /// it came to, negative up.
    pub fn add(&mut self, delta: f32, unit: Unit) -> i32 {
        self.0 += match unit {
            Unit::Pixels => delta / PIXELS_PER_NOTCH,
            Unit::Lines => delta / LINES_PER_NOTCH,
        };
        let whole = self.0.trunc();
        self.0 -= whole;
        whole as i32
    }
}

#[cfg(test)]
mod tests;
