//! Bringing the pane on screen into view, once per pane (SHELL-RAIL-18).
//!
//! A long tree puts the pane on screen below the fold. It is scrolled to the nearest edge once when
//! it becomes the pane shown, not on every redraw, so a person reading the rail is not fought: the
//! list is left where they put it until another pane comes on screen. The row says where it was
//! drawn as it paints, and the scroll is made on the frame after.

use gpui_kit::{px, Context, Window};

use super::RailView;

/// How far the content moves to bring a row spanning `row` into a view spanning `view` with the least
/// movement, both as `(top, bottom)` in the same coordinates: none while it is inside, the gap to the
/// nearer edge otherwise. Positive moves the content down.
pub fn nearest(view: (f32, f32), row: (f32, f32)) -> f32 {
    if row.0 < view.0 {
        view.0 - row.0
    } else if row.1 > view.1 {
        // A row taller than the view is lined up by its top, which is what is read first.
        (view.1 - row.1).max(view.0 - row.0)
    } else {
        0.0
    }
}

impl RailView {
    /// The pane on screen changed: it is scrolled to once, on the next frame, whenever it is not the
    /// one already scrolled to.
    pub(super) fn scroll_to(&mut self, shown: Option<u64>) {
        if shown.is_some() && shown != self.scrolled_to {
            self.pending_scroll = true;
        }
        self.scrolled_to = shown;
    }

    /// Asks for the scroll to be made once the row has said where it is.
    pub(super) fn schedule_scroll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.pending_scroll) {
            return;
        }
        let rail = cx.entity();
        // After this frame is drawn, so the row has said where it was.
        window.defer(cx, move |_, cx| {
            rail.update(cx, |rail, cx| rail.make_scroll(cx));
        });
    }

    fn make_scroll(&mut self, cx: &mut Context<Self>) {
        let (view, row) = (self.scroll.bounds(), self.shown_row.get());
        if row.size.height <= px(0.0) {
            return;
        }
        let delta = nearest((view.top().as_f32(), view.bottom().as_f32()), (row.top().as_f32(), row.bottom().as_f32()));
        if delta == 0.0 {
            return;
        }
        let (offset, most) = (self.scroll.offset(), self.scroll.max_offset());
        let y = (offset.y.as_f32() + delta).clamp(-most.y.as_f32(), 0.0);
        self.scroll.set_offset(gpui_kit::point(offset.x, px(y)));
        cx.notify();
    }
}

/// What a test reads of where the list is scrolled to, and moves it as a person's wheel would.
#[cfg(test)]
impl RailView {
    pub(crate) fn scrolled(&self) -> gpui_kit::Pixels {
        self.scroll.offset().y
    }

    pub(crate) fn scroll_by(&self, dy: f32) {
        let offset = self.scroll.offset();
        self.scroll.set_offset(gpui_kit::point(offset.x, offset.y + px(dy)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_inside_the_view_does_not_move_it() {
        assert_eq!(nearest((100.0, 500.0), (200.0, 232.0)), 0.0);
        assert_eq!(nearest((100.0, 500.0), (100.0, 500.0)), 0.0, "filling the view exactly is inside");
    }

    #[test]
    fn a_row_below_the_view_is_brought_up_to_its_bottom_edge_and_one_above_down_to_its_top() {
        assert_eq!(nearest((100.0, 500.0), (600.0, 632.0)), -132.0);
        assert_eq!(nearest((100.0, 500.0), (40.0, 72.0)), 60.0);
    }

    #[test]
    fn a_row_taller_than_the_view_is_lined_up_by_its_top() {
        assert_eq!(nearest((100.0, 200.0), (150.0, 400.0)), -50.0);
    }
}
