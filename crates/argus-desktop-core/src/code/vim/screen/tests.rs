use proptest::prelude::*;

use super::*;

const VIEW: View = View { top: 10, bottom: 19 };

#[test]
fn with_no_window_told_the_view_is_the_whole_text() {
    let v = View::default().within(49);
    assert_eq!((v.top, v.bottom, v.rows()), (0, 49, 50));
}

#[test]
fn a_view_is_held_to_the_text() {
    let v = View { top: 40, bottom: 80 }.within(45);
    assert_eq!((v.top, v.bottom), (40, 45));
    let v = View { top: 90, bottom: 99 }.within(45);
    assert_eq!((v.top, v.bottom), (45, 45), "a view past the text is its last line");
}

#[test]
fn h_m_and_l_pick_the_top_middle_and_bottom() {
    assert_eq!(VIEW.nth_from_top(1), 10);
    assert_eq!(VIEW.nth_from_top(3), 12);
    assert_eq!(VIEW.middle(), 14);
    assert_eq!(VIEW.nth_from_bottom(1), 19);
    assert_eq!(VIEW.nth_from_bottom(4), 16);
}

#[test]
fn a_count_on_h_or_l_does_not_leave_the_view() {
    assert_eq!(VIEW.nth_from_top(99), 19);
    assert_eq!(VIEW.nth_from_bottom(99), 10);
    assert_eq!(VIEW.nth_from_top(0), 10, "no count is the first");
}

#[test]
fn a_page_is_the_rows_in_view_and_stops_at_the_text() {
    assert_eq!(VIEW.paged(12, 1, true, 100), 22);
    assert_eq!(VIEW.paged(12, 2, true, 100), 32);
    assert_eq!(VIEW.paged(95, 1, true, 100), 100);
    assert_eq!(VIEW.paged(12, 1, false, 100), 2);
    assert_eq!(VIEW.paged(5, 1, false, 100), 0);
}

#[test]
fn a_half_page_is_half_the_rows_or_the_count() {
    assert_eq!(VIEW.halved(12, None, true, 100), 17);
    assert_eq!(VIEW.halved(12, None, false, 100), 7);
    assert_eq!(VIEW.halved(12, Some(3), true, 100), 15);
    assert_eq!(VIEW.halved(1, Some(3), false, 100), 0);
    assert_eq!(View { top: 0, bottom: 0 }.halved(0, None, true, 10), 1, "at least a line");
}

proptest! {
    // However the window is told, a line picked is a line of the text, and in the view when one is.
    #[test]
    fn a_picked_line_is_in_the_text_and_the_view(
        top in 0usize..60, rows in 1usize..40, last in 0usize..60, count in 0usize..100, line in 0usize..60
    ) {
        let v = View { top, bottom: top + rows - 1 }.within(last);
        for picked in [v.nth_from_top(count), v.middle(), v.nth_from_bottom(count)] {
            prop_assert!(picked >= v.top && picked <= v.bottom);
            prop_assert!(picked <= last);
        }
        let line = line.min(last);
        for forward in [true, false] {
            prop_assert!(v.paged(line, count, forward, last) <= last);
            prop_assert!(v.halved(line, Some(count), forward, last) <= last);
        }
    }
}
