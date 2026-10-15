//! A size the person dragged one of the window's edges to — the rail's and
//! the views panel's widths, an open feature's outline width and its brief's
//! height — held to what leaves it usable and the rest room, and whether it
//! changed since it was saved.
//!
//! Undragged it is `None`, and the page's own width stands, which may
//! narrow in a small window; once dragged, the person's width is kept as
//! theirs.

pub const RAIL: Bounds = Bounds { min: 200, max: 560 };
pub const VIEWS: Bounds = Bounds { min: 360, max: 1200 };
pub const OUTLINE: Bounds = Bounds { min: 220, max: 1200 };
pub const BRIEF: Bounds = Bounds { min: 40, max: 900 };

#[derive(Debug, Clone, Copy)]
pub struct Bounds {
    pub min: u32,
    pub max: u32,
}

#[derive(Debug)]
pub struct DraggedWidth {
    bounds: Bounds,
    width: Option<u32>,
    unsaved: bool,
}

impl DraggedWidth {
    pub fn new(bounds: Bounds, saved: Option<u32>) -> DraggedWidth {
        DraggedWidth { bounds, width: saved.map(|w| w.clamp(bounds.min, bounds.max)), unsaved: false }
    }

    /// The width a drag let go at, or `None` to go back to the page's own.
    pub fn set(&mut self, width: Option<u32>) {
        let width = width.map(|w| w.clamp(self.bounds.min, self.bounds.max));
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
    fn a_hand_edited_width_is_held_to_its_bounds_too() {
        assert_eq!(DraggedWidth::new(VIEWS, Some(5)).get(), Some(VIEWS.min));
    }
}
