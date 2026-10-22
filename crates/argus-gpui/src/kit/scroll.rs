//! Bringing a child of a scrolling list into view by the least movement, for a list that keeps its
//! own scroll handle. The rule ("nearest": none while it is inside, the gap to the nearer edge
//! otherwise) is the rail's (`chrome/rail/scroll`); a row says where it was drawn as it paints, so
//! the scroll is made from the bounds the last frame left.

use gpui_kit::{point, px, ScrollHandle};

use crate::chrome::rail::scroll::nearest;

/// Scrolls `scroll` so its child `ix` is inside, by the least movement; whether it moved. A child the
/// last frame did not draw, or a view with no height yet, moves nothing.
pub fn reveal(scroll: &ScrollHandle, ix: usize) -> bool {
    let (view, Some(row)) = (scroll.bounds(), scroll.bounds_for_item(ix)) else { return false };
    if view.size.height <= px(0.0) || row.size.height <= px(0.0) {
        return false;
    }
    let delta = nearest((view.top().as_f32(), view.bottom().as_f32()), (row.top().as_f32(), row.bottom().as_f32()));
    if delta == 0.0 {
        return false;
    }
    let (offset, most) = (scroll.offset(), scroll.max_offset());
    let y = (offset.y.as_f32() + delta).clamp(-most.y.as_f32(), 0.0);
    scroll.set_offset(point(offset.x, px(y)));
    true
}
