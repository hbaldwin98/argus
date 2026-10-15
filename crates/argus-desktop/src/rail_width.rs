//! How wide the person dragged the window's rail: held to what leaves the
//! rail usable and the stage room, and whether it changed since it was
//! saved.
//!
//! Undragged it is `None`, and the page's own width stands, which narrows
//! in a small window; once dragged, the person's width is kept as theirs.

pub const MIN_WIDTH: u32 = 200;
pub const MAX_WIDTH: u32 = 560;

#[derive(Debug, Default)]
pub struct RailWidth {
    width: Option<u32>,
    unsaved: bool,
}

impl RailWidth {
    pub fn new(saved: Option<u32>) -> RailWidth {
        RailWidth { width: saved.map(|w| w.clamp(MIN_WIDTH, MAX_WIDTH)), unsaved: false }
    }

    /// The width a drag let go at, or `None` to go back to the page's own.
    pub fn set(&mut self, width: Option<u32>) {
        let width = width.map(|w| w.clamp(MIN_WIDTH, MAX_WIDTH));
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
        let mut rail = RailWidth::new(None);
        rail.set(Some(40));
        assert_eq!(rail.get(), Some(MIN_WIDTH));
        assert!(rail.take_unsaved());
        assert!(!rail.take_unsaved());
        rail.set(Some(MIN_WIDTH));
        assert!(!rail.take_unsaved(), "the same width is not saved again");
        rail.set(Some(10_000));
        assert_eq!(rail.get(), Some(MAX_WIDTH));
        rail.set(None);
        assert_eq!(rail.get(), None, "back to the page's own");
    }

    #[test]
    fn a_hand_edited_width_is_held_to_its_bounds_too() {
        assert_eq!(RailWidth::new(Some(5)).get(), Some(MIN_WIDTH));
    }
}
