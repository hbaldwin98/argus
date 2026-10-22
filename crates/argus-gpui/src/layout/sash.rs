//! What size a dock has with its sash under the pointer.
//!
//! The page's edge drag (`layout/edge.ts`): the size is measured from the dock's
//! own edge to the pointer, plus the distance the sash was grabbed from that
//! edge, so the dock does not jump to put its border under the pointer. What a
//! drag draws is held to the bounds Rust keeps a dock to; the size reported on
//! release is the same number, and what the dock is afterwards is whatever
//! Rust sends back.

use argus_desktop_core::layout::docks::{Dock, BOTTOM, SIDE};
use gpui_kit::{Bounds, Pixels, Point};

/// The size with the pointer at `at`: the bottom's height up from the
/// stage's foot, a side's width in from the work area's own edge. A size is the page's pixels
/// and the pointer is the window's, so the distance is divided by `zoom` (1.0 at 100%).
pub fn size_at(dock: Dock, at: Point<Pixels>, work: Bounds<Pixels>, center: Bounds<Pixels>, zoom: f32) -> f32 {
    let real = match dock {
        Dock::Bottom => center.bottom().as_f32() - at.y.as_f32(),
        Dock::Left => at.x.as_f32() - work.left().as_f32(),
        Dock::Right => work.right().as_f32() - at.x.as_f32(),
    };
    real / zoom
}

/// How far the sash was grabbed from the dock's edge: the dock's size as
/// drawn, less what the pointer's place says.
pub fn grab(dock: Dock, drawn: f32, at: Point<Pixels>, work: Bounds<Pixels>, center: Bounds<Pixels>, zoom: f32) -> f32 {
    drawn - size_at(dock, at, work, center, zoom)
}

/// The size the drag holds with the pointer at `at`, in whole pixels and
/// within what Rust keeps a dock to.
pub fn held(dock: Dock, grab: f32, at: Point<Pixels>, work: Bounds<Pixels>, center: Bounds<Pixels>, zoom: f32) -> u32 {
    let bounds = match dock {
        Dock::Bottom => BOTTOM,
        Dock::Left | Dock::Right => SIDE,
    };
    let size = (size_at(dock, at, work, center, zoom) + grab).round();
    size.clamp(bounds.min as f32, bounds.max as f32) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{point, px, size};

    fn area() -> Bounds<Pixels> {
        Bounds::new(point(px(250.0), px(34.0)), size(px(1000.0), px(600.0)))
    }

    fn at(x: f32, y: f32) -> Point<Pixels> {
        point(px(x), px(y))
    }

    #[test]
    fn each_dock_is_measured_from_its_own_edge() {
        assert_eq!(size_at(Dock::Left, at(550.0, 300.0), area(), area(), 1.0), 300.0);
        assert_eq!(size_at(Dock::Right, at(950.0, 300.0), area(), area(), 1.0), 300.0);
        assert_eq!(size_at(Dock::Bottom, at(700.0, 434.0), area(), area(), 1.0), 200.0);
    }

    #[test]
    fn a_sash_grabbed_off_the_edge_keeps_its_offset_and_the_dock_does_not_jump() {
        // The dock is 300 wide and the sash is grabbed 3 px inside its edge.
        let grabbed = grab(Dock::Left, 300.0, at(547.0, 300.0), area(), area(), 1.0);
        assert_eq!(grabbed, 3.0);
        assert_eq!(held(Dock::Left, grabbed, at(547.0, 300.0), area(), area(), 1.0), 300, "no move, no change");
        assert_eq!(held(Dock::Left, grabbed, at(647.0, 300.0), area(), area(), 1.0), 400);
    }

    /// The page's pixels are the window's divided by the zoom: a pointer 300 real pixels in is 150 of
    /// the page's at 200%.
    #[test]
    fn a_zoomed_window_measures_in_the_pages_pixels() {
        assert_eq!(size_at(Dock::Left, at(550.0, 300.0), area(), area(), 2.0), 150.0);
        assert_eq!(held(Dock::Left, 0.0, at(1050.0, 300.0), area(), area(), 2.0), 400);
        assert_eq!(grab(Dock::Left, 150.0, at(547.0, 300.0), area(), area(), 2.0), 1.5);
    }

    #[test]
    fn a_held_size_stays_within_the_bounds_rust_keeps_a_dock_to() {
        assert_eq!(held(Dock::Left, 0.0, at(260.0, 300.0), area(), area(), 1.0), SIDE.min);
        assert_eq!(held(Dock::Right, 0.0, at(0.0, 300.0), area(), area(), 1.0), 1250);
        assert_eq!(held(Dock::Bottom, 0.0, at(700.0, 633.0), area(), area(), 1.0), BOTTOM.min);
        assert_eq!(held(Dock::Bottom, 0.0, at(700.0, -900.0), area(), area(), 1.0), BOTTOM.max);
    }
}
