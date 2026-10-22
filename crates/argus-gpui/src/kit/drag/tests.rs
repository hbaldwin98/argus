use super::*;
use gpui_kit::{point, px};

/// A rem at 100%: the page's pixels are the window's.
fn rem_px() -> Pixels {
    px(13.0)
}

fn at(x: f32, y: f32) -> Point<Pixels> {
    point(px(x), px(y))
}

#[test]
fn a_press_that_stays_under_the_threshold_is_a_click_on_what_was_pressed() {
    let mut drag = Drag::new();
    drag.press(at(100.0, 100.0), "tab");
    assert_eq!(drag.moved(at(103.0, 103.0), rem_px()), None, "4.2 px is under the threshold");
    assert!(!drag.is_dragging());
    assert_eq!(drag.released(at(103.0, 103.0)), Some(Step::Clicked("tab")));
    assert!(!drag.pressed());
}

#[test]
fn a_press_that_goes_the_threshold_is_a_drag_that_drops() {
    let mut drag = Drag::new();
    drag.press(at(100.0, 100.0), "tab");
    assert_eq!(drag.moved(at(105.0, 100.0), rem_px()), Some(Step::Began("tab")), "5 px is the threshold");
    assert!(drag.is_dragging());
    assert_eq!(drag.moved(at(130.0, 90.0), rem_px()), Some(Step::Moved("tab")));
    assert_eq!(drag.at(), Some(at(130.0, 90.0)));
    assert_eq!(drag.released(at(130.0, 90.0)), Some(Step::Dropped("tab", at(130.0, 90.0))));
}

#[test]
fn a_drag_does_not_turn_back_into_a_click_by_returning_to_where_it_began() {
    let mut drag = Drag::new();
    drag.press(at(0.0, 0.0), 1);
    drag.moved(at(40.0, 0.0), rem_px());
    drag.moved(at(0.0, 0.0), rem_px());
    assert_eq!(drag.released(at(0.0, 0.0)), Some(Step::Dropped(1, at(0.0, 0.0))));
}

#[test]
fn escape_cancels_a_drag_with_nothing_dropped() {
    let mut drag = Drag::new();
    drag.press(at(0.0, 0.0), "tab");
    drag.moved(at(50.0, 0.0), rem_px());
    assert_eq!(drag.cancel(), Some(Step::Cancelled("tab")));
    assert!(!drag.pressed());
    assert_eq!(drag.released(at(50.0, 0.0)), None, "the release after a cancel is nothing");
}

#[test]
fn cancelling_a_press_that_never_dragged_clicks_nothing() {
    let mut drag = Drag::new();
    drag.press(at(0.0, 0.0), "tab");
    assert_eq!(drag.cancel(), None);
    assert_eq!(drag.released(at(0.0, 0.0)), None);
}

#[test]
fn nothing_held_means_nothing_to_tell() {
    let mut drag: Drag<u8> = Drag::new();
    assert_eq!(drag.moved(at(500.0, 500.0), rem_px()), None);
    assert_eq!(drag.released(at(1.0, 1.0)), None);
    assert_eq!(drag.cancel(), None);
}

mod properties {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Source: `gpui/parity/cross-cutting.md` Decided 6, one 5 px
        /// threshold. A press is a drag exactly when the pointer was ever
        /// 5 px or more from where it was pressed, and then its release
        /// drops, never clicks.
        #[test]
        fn a_press_is_a_drag_exactly_when_the_pointer_went_the_threshold(
            moves in proptest::collection::vec((-20.0f32..20.0, -20.0f32..20.0), 0..12),
        ) {
            let mut drag = Drag::new();
            drag.press(at(0.0, 0.0), ());
            let mut went = false;
            let mut last = at(0.0, 0.0);
            for (dx, dy) in moves {
                last = at(dx, dy);
                went |= dx.hypot(dy) >= THRESHOLD;
                drag.moved(last, rem_px());
            }
            prop_assert_eq!(drag.is_dragging(), went);
            let ended = drag.released(last);
            prop_assert_eq!(matches!(ended, Some(Step::Dropped(..))), went);
            prop_assert_eq!(matches!(ended, Some(Step::Clicked(_))), !went);
        }
    }
}

/// SHELL-LOOK-8: the 5 px threshold is the page's, so it doubles at 200% zoom.
#[test]
fn the_threshold_follows_the_zoom() {
    let mut drag = Drag::new();
    drag.press(at(0.0, 0.0), "tab");
    assert_eq!(drag.moved(at(9.0, 0.0), px(26.0)), None, "9 real px is 4.5 of the page's");
    assert_eq!(drag.moved(at(10.0, 0.0), px(26.0)), Some(Step::Began("tab")));
}
