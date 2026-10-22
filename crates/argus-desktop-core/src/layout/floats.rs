//! Where a dialog may sit and how small it may get while a pointer holds it: the rules a window
//! follows live, so that what it draws and what it reports on release are the same box.
//!
//! Rust keeps a dialog's bounds no smaller than the minimum (`docks`, `FloatBounds::held`); this is
//! the other half the page used to hold alone (`layout/floats.ts`, `place`), moved here so a second
//! window does not write it again. A moved or resized box is not stored from here: the window
//! reports it on release and the session decides.

use crate::layout::docks::{FloatBounds, FLOAT_MIN};

/// How much of a dialog stays inside the work area however it is moved, so its header can always
/// be reached again: this much of its width, and this much above the bottom edge.
pub const KEEP: (i32, i32) = (120, 40);

impl FloatBounds {
    /// The box moved by the pointer's travel.
    pub fn moved(self, dx: i32, dy: i32) -> FloatBounds {
        FloatBounds { x: self.x.saturating_add(dx), y: self.y.saturating_add(dy), ..self }
    }

    /// The box with its corner moved by the pointer's travel, no smaller than the minimum.
    pub fn resized(self, dx: i32, dy: i32) -> FloatBounds {
        let grown = |size: u32, by: i32, least: u32| (i64::from(size) + i64::from(by)).clamp(i64::from(least), i64::from(u32::MAX)) as u32;
        FloatBounds { width: grown(self.width, dx, FLOAT_MIN.0), height: grown(self.height, dy, FLOAT_MIN.1), ..self }
    }

    /// The box as it is drawn in a work area `width` by `height`: no bigger than the area, at least
    /// [`KEEP`] of its width inside horizontally, its top never above the area and [`KEEP`] of
    /// height of room left under it. An area smaller than that draws it at its corner.
    pub fn kept_in(self, width: u32, height: u32) -> FloatBounds {
        let (w, h) = (self.width.min(width), self.height.min(height));
        let (area_w, area_h) = (i64::from(width), i64::from(height));
        let left = (i64::from(KEEP.0) - i64::from(w)).max(i64::from(i32::MIN));
        let right = (area_w - i64::from(KEEP.0)).max(0);
        let bottom = (area_h - i64::from(KEEP.1)).max(0);
        FloatBounds {
            x: i64::from(self.x).clamp(left.min(right), right) as i32,
            y: i64::from(self.y).clamp(0, bottom) as i32,
            width: w,
            height: h,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: i32, y: i32, width: u32, height: u32) -> FloatBounds {
        FloatBounds { x, y, width, height }
    }

    #[test]
    fn a_box_moves_with_the_pointer_and_resizes_no_smaller_than_the_minimum() {
        assert_eq!(at(96, 64, 560, 520).moved(-10, 30), at(86, 94, 560, 520));
        assert_eq!(at(96, 64, 560, 520).resized(40, -20), at(96, 64, 600, 500));
        assert_eq!(at(96, 64, 560, 520).resized(-900, -900), at(96, 64, FLOAT_MIN.0, FLOAT_MIN.1));
    }

    #[test]
    fn a_box_dragged_far_out_keeps_its_header_in_reach() {
        let area = (1000, 700);
        let far_right = at(5000, 100, 560, 520).kept_in(area.0, area.1);
        assert_eq!(far_right.x, 1000 - KEEP.0, "{KEEP:?} of the width stays in");
        let far_left = at(-5000, 100, 560, 520).kept_in(area.0, area.1);
        assert_eq!(far_left.x, KEEP.0 - 560, "only the keep of its width is left in");
        let above = at(100, -50, 560, 520).kept_in(area.0, area.1);
        assert_eq!(above.y, 0, "the header never goes above the area");
        let below = at(100, 5000, 560, 520).kept_in(area.0, area.1);
        assert_eq!(below.y, 700 - KEEP.1, "and a header's height stays above the bottom");
    }

    #[test]
    fn a_box_is_no_bigger_than_the_area_and_one_that_fits_is_left_where_it_is() {
        assert_eq!(at(0, 0, 4000, 4000).kept_in(800, 600), at(0, 0, 800, 600));
        assert_eq!(at(96, 64, 560, 520).kept_in(1200, 800), at(96, 64, 560, 520));
    }

    #[test]
    fn an_area_not_yet_measured_draws_the_box_at_its_corner() {
        let kept = at(96, 64, 560, 520).kept_in(0, 0);
        assert_eq!((kept.x, kept.y, kept.width, kept.height), (0, 0, 0, 0));
    }

    mod properties {
        use proptest::prelude::*;

        use super::*;

        proptest! {
            // `layout/floats.ts` `place`: whatever the box, the header can be reached again.
            #[test]
            fn a_kept_box_is_never_out_of_reach(
                x in -100_000i32..100_000, y in -100_000i32..100_000,
                w in 280u32..4000, h in 160u32..4000,
                area_w in 200u32..4000, area_h in 100u32..4000,
            ) {
                let kept = at(x, y, w, h).kept_in(area_w, area_h);
                prop_assert!(kept.width <= area_w && kept.height <= area_h);
                prop_assert!(kept.y >= 0 && i64::from(kept.y) <= (i64::from(area_h) - i64::from(KEEP.1)).max(0));
                prop_assert!(i64::from(kept.x) + i64::from(kept.width) >= i64::from(KEEP.0).min(i64::from(kept.width)));
                prop_assert!(i64::from(kept.x) <= (i64::from(area_w) - i64::from(KEEP.0)).max(0));
                prop_assert_eq!(kept.kept_in(area_w, area_h), kept, "keeping is idempotent");
            }
        }
    }
}
