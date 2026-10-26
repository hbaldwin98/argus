//! A list of fixed-height rows that draws only the rows on screen, for a list whose length the
//! window does not choose: a large checkout's changed files or its whole tree. Drawn whole, every
//! row is laid out on every frame; at tens of thousands of rows a frame takes long enough that
//! Windows dispatches into the window mid-draw, and GPUI panics ("must call prepaint before paint").

use std::rc::Rc;

use gpui_kit::component::{v_virtual_list, VirtualList, VirtualListScrollHandle};
use gpui_kit::{px, size, AnyElement, Context, ElementId, Pixels, Render, Size, Window};

use crate::theme::scale::pixels;

/// Rows `heights` tall (each in the page's pixels, as a token is), of which `draw` is asked only
/// for those in view. `draw` gets the row's index; its element is laid out in exactly its height.
pub fn long_list<V: Render>(
    view: gpui_kit::Entity<V>,
    id: impl Into<ElementId>,
    heights: &[f32],
    rem_size: Pixels,
    scroll: &VirtualListScrollHandle,
    draw: impl Fn(&mut V, usize, &mut Window, &mut Context<V>) -> AnyElement + 'static,
) -> VirtualList {
    let sizes: Rc<Vec<Size<Pixels>>> = Rc::new(heights.iter().map(|h| size(px(0.0), pixels(*h, rem_size))).collect());
    v_virtual_list(view, id, sizes, move |this, range, window, cx| range.map(|ix| draw(this, ix, window, cx)).collect::<Vec<_>>())
        .track_scroll(scroll)
}
