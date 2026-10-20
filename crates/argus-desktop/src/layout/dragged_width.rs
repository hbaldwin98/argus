//! A size the person dragged one of the window's edges to — the rail's
//! width, an open feature's outline width and its brief's height, the Code
//! stage's file tree and outline — held to what leaves it usable and the rest room, and whether it
//! changed since it was saved.
//!
//! Undragged it is `None`, and the page's own width stands, which may
//! narrow in a small window; once dragged, the person's width is kept as
//! theirs.

pub const RAIL: Bounds = Bounds { min: 200, max: 560 };
/// The rail folded to its icons, the page's `--rail-folded`: a drag let go
/// nearer this than `RAIL.min` folds it there rather than leaving a rail
/// too narrow for its words and too wide for its icons.
pub const RAIL_FOLDED: u32 = 56;
pub const OUTLINE: Bounds = Bounds { min: 220, max: 1200 };
pub const BRIEF: Bounds = Bounds { min: 40, max: 900 };
pub const CODE_TREE: Bounds = Bounds { min: 180, max: 800 };
pub const CODE_OUTLINE: Bounds = Bounds { min: 160, max: 800 };

#[derive(Debug, Clone, Copy)]
pub struct Bounds {
    pub min: u32,
    pub max: u32,
}

#[derive(Debug)]
pub struct DraggedWidth {
    bounds: Bounds,
    /// A width below `bounds.min` it snaps to instead, the rail's folded one.
    folded: Option<u32>,
    width: Option<u32>,
    unsaved: bool,
}

impl DraggedWidth {
    pub fn new(bounds: Bounds, saved: Option<u32>) -> DraggedWidth {
        DraggedWidth::folding(bounds, None, saved)
    }

    pub fn folding(bounds: Bounds, folded: Option<u32>, saved: Option<u32>) -> DraggedWidth {
        let mut width = DraggedWidth { bounds, folded, width: None, unsaved: false };
        width.width = saved.map(|w| width.held(w));
        width
    }

    fn held(&self, width: u32) -> u32 {
        match self.folded {
            Some(folded) if width < (folded + self.bounds.min) / 2 => folded,
            _ => width.clamp(self.bounds.min, self.bounds.max),
        }
    }

    /// The width a drag let go at, or `None` to go back to the page's own.
    pub fn set(&mut self, width: Option<u32>) {
        let width = width.map(|w| self.held(w));
        if self.width != width {
            self.width = width;
            self.unsaved = true;
        }
    }

    pub fn get(&self) -> Option<u32> {
        self.width
    }

    /// Whether it changed since last asked, so it is saved.
    pub fn take_unsaved(&mut self) -> bool {
        std::mem::take(&mut self.unsaved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drag_is_held_to_its_bounds_and_saved_once() {
        let mut rail = DraggedWidth::new(RAIL, None);
        rail.set(Some(40));
        assert_eq!(rail.get(), Some(RAIL.min));
        assert!(rail.take_unsaved());
        assert!(!rail.take_unsaved());
        rail.set(Some(RAIL.min));
        assert!(!rail.take_unsaved(), "the same width is not saved again");
        rail.set(Some(10_000));
        assert_eq!(rail.get(), Some(RAIL.max));
        rail.set(None);
        assert_eq!(rail.get(), None, "back to the page's own");
    }

    #[test]
    fn a_rail_dragged_nearer_folded_than_its_minimum_folds() {
        let mut rail = DraggedWidth::folding(RAIL, Some(RAIL_FOLDED), None);
        rail.set(Some(100));
        assert_eq!(rail.get(), Some(RAIL_FOLDED));
        rail.set(Some(150));
        assert_eq!(rail.get(), Some(RAIL.min), "nearer the minimum, it opens to it");
        assert_eq!(DraggedWidth::folding(RAIL, Some(RAIL_FOLDED), Some(RAIL_FOLDED)).get(), Some(RAIL_FOLDED));
    }

    #[test]
    fn a_hand_edited_width_is_held_to_its_bounds_too() {
        assert_eq!(DraggedWidth::new(OUTLINE, Some(5)).get(), Some(OUTLINE.min));
    }
}
