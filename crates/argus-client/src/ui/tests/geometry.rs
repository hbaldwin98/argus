//! Centering and stacking, without a frame around them.

use super::*;

// --- layout helpers -----------------------------------------------------

#[test]
fn a_modal_is_centered_in_its_area() {
    let r = centered_rect(10, 4, Rect::new(0, 0, 30, 20));
    assert_eq!((r.x, r.y, r.width, r.height), (10, 8, 10, 4));
}

#[test]
fn a_modal_larger_than_the_screen_is_pinned_not_wrapped_negative() {
    let r = centered_rect(50, 40, Rect::new(0, 0, 30, 20));
    assert_eq!((r.x, r.y), (0, 0), "saturating, never underflowing");
}

#[test]
fn rows_stack_down_the_panel_and_stop_at_its_bottom() {
    // Two lines per item, and a half-drawn item is worse than none.
    let inner = Rect::new(1, 1, 10, 5);
    assert_eq!(row_rect_of(inner, 0, ROW_HEIGHT).unwrap().y, 1);
    assert_eq!(row_rect_of(inner, 0, ROW_HEIGHT).unwrap().height, ROW_HEIGHT);
    assert_eq!(row_rect_of(inner, 1, ROW_HEIGHT).unwrap().y, 3);
    assert!(row_rect_of(inner, 2, ROW_HEIGHT).is_none(), "no room for both its lines");
}
