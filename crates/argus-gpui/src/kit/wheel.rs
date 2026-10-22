//! Which scroll box takes the wheel: one that can still move the way it turns, and no other.
//!
//! GPUI hands a wheel event to every scroll box under the pointer, and each moves, so a log inside a
//! scrolling page scrolls both. The page's boxes chain instead (`overscroll-behavior: auto`): the
//! innermost takes it while it can move, and at its end the next box out does. A box asks for this
//! here by putting [`Wheel::probe`] among its children and [`Wheel::keep`] on its wheel, and
//! hands the box's `ScrollHandle` to [`Wheel::new`].
//!
//! The probe is painted after its parent's own scroll listener is registered, so it hears the event
//! first and sees where the box was; `keep` hears it after the box has moved, compares, and stops
//! the event only when the box really moved.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::{canvas, prelude::*, App, DispatchPhase, IntoElement, Pixels, Point, ScrollHandle, ScrollWheelEvent, Window};

pub struct Wheel {
    handle: ScrollHandle,
    before: Rc<Cell<Point<Pixels>>>,
}

/// Whether a box scrolled from `before` to `after` (offsets, which are never positive) moved at all
/// once held to its `max` range; a box already at its end is pushed past it and does not.
pub fn moved(before: Point<Pixels>, after: Point<Pixels>, max: Point<Pixels>) -> bool {
    let held = |at: Pixels, max: Pixels| at.clamp(-max.max(Pixels::ZERO), Pixels::ZERO);
    held(before.x, max.x) != held(after.x, max.x) || held(before.y, max.y) != held(after.y, max.y)
}

impl Wheel {
    pub fn new(handle: &ScrollHandle) -> Self {
        Self { handle: handle.clone(), before: Rc::default() }
    }

    /// Remembers where the box was when a wheel event arrived. It fills its parent and takes no room.
    pub fn probe(&self) -> impl IntoElement {
        let (handle, before) = (self.handle.clone(), self.before.clone());
        canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                window.on_mouse_event(move |_: &ScrollWheelEvent, phase, _, _| {
                    if phase == DispatchPhase::Bubble {
                        before.set(handle.offset());
                    }
                });
            },
        )
        .absolute()
        .size_full()
    }

    /// The box's wheel listener: keeps the event for this box when it moved, and lets it on to the
    /// box around it when it could not.
    pub fn keep(&self) -> impl Fn(&ScrollWheelEvent, &mut Window, &mut App) + 'static {
        let (handle, before) = (self.handle.clone(), self.before.clone());
        move |_, _, cx| {
            if moved(before.get(), handle.offset(), handle.max_offset()) {
                cx.stop_propagation();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{point, px};

    #[test]
    fn a_box_moves_while_it_has_room_the_way_it_turns() {
        let max = point(px(0.0), px(100.0));
        assert!(moved(point(px(0.0), px(-10.0)), point(px(0.0), px(-40.0)), max));
        assert!(moved(point(px(0.0), px(-10.0)), point(px(0.0), px(30.0)), max), "back up to the top is a move");
    }

    #[test]
    fn a_box_at_its_end_does_not_move_however_far_it_is_pushed() {
        let max = point(px(0.0), px(100.0));
        assert!(!moved(point(px(0.0), px(-100.0)), point(px(0.0), px(-160.0)), max), "at the bottom");
        assert!(!moved(point(px(0.0), px(0.0)), point(px(0.0), px(50.0)), max), "at the top");
        assert!(!moved(point(px(0.0), px(0.0)), point(px(0.0), px(-50.0)), point(px(0.0), px(0.0))), "nothing to scroll");
    }

    #[test]
    fn each_axis_is_held_to_its_own_range() {
        let max = point(px(200.0), px(0.0));
        assert!(moved(point(px(0.0), px(0.0)), point(px(-30.0), px(-30.0)), max), "sideways has room");
        assert!(!moved(point(px(-200.0), px(0.0)), point(px(-230.0), px(-30.0)), max), "sideways is at its end and up and down has none");
    }
}
