//! A float over the window that is not a dialog: held to the foot of a place another view drew (a
//! cell of the grid, the stage), drawn over everything, taking no keys and dimming nothing.
//!
//! The dialogs' host (`host`) is modal: a scrim, the keys, Escape. A float is none of that, so it is
//! an element of its own rather than a kind of dialog. Where it goes is only known once the place it
//! is held to has been laid out this frame (`kit/spots`), so it measures and places its child in its
//! own prepaint, which a [`deferred`] float runs after every other element's.

use gpui_kit::{deferred, div, prelude::*, px, size, AnyElement, App, AvailableSpace, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, LayoutId, Pixels, Style, Window};

use crate::kit::spots::{Spot, Spots};
use crate::theme::scale::pixels;

/// Where a float is held, the first of `anchors` that was drawn, `inset` in from its sides and `lift`
/// up from its foot, at most `max_w` wide.
pub struct Float {
    anchors: Vec<Spot>,
    inset: f32,
    lift: f32,
    max_w: f32,
    child: Option<AnyElement>,
}

/// `child` floated at the foot of the first of `anchors` that is on screen, else of the window.
pub fn float(anchors: Vec<Spot>, inset: f32, lift: f32, max_w: f32, child: impl IntoElement) -> impl IntoElement {
    deferred(Float { anchors, inset, lift, max_w, child: Some(child.into_any_element()) }).priority(1)
}

/// The box the float takes inside `anchor`, given its height: centred, `inset` from either side up
/// to `max_w`, its foot `lift` above the anchor's.
pub fn placed(anchor: Bounds<Pixels>, inset: Pixels, lift: Pixels, max_w: Pixels, height: Pixels) -> Bounds<Pixels> {
    let width = (anchor.size.width - inset * 2.0).min(max_w).max(px(0.0));
    let left = anchor.origin.x + (anchor.size.width - width) / 2.0;
    let top = (anchor.origin.y + anchor.size.height - lift - height).max(anchor.origin.y);
    Bounds::new(gpui_kit::point(left, top), size(width, height))
}

impl IntoElement for Float {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Float {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    /// Nothing in the flow: the float takes no room where it is put.
    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        let style = Style { position: gpui_kit::Position::Absolute, ..Style::default() };
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) {
        let Some(child) = self.child.take() else { return };
        let spots = Spots::peek(cx);
        let anchor = self.anchors.iter().find_map(|spot| spots.get(*spot)).unwrap_or_else(|| Bounds::new(gpui_kit::point(px(0.0), px(0.0)), window.viewport_size()));
        let rem = window.rem_size();
        let (inset, lift, max_w) = (pixels(self.inset, rem), pixels(self.lift, rem), pixels(self.max_w, rem));
        let width = placed(anchor, inset, lift, max_w, px(0.0)).size.width;
        let mut held = div().w(width).child(child).into_any_element();
        let measured = held.layout_as_root(size(AvailableSpace::Definite(width), AvailableSpace::MinContent), window, cx);
        let at = placed(anchor, inset, lift, max_w, measured.height);
        held.prepaint_at(at.origin, window, cx);
        self.child = Some(held);
    }

    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), _: &mut (), window: &mut Window, cx: &mut App) {
        if let Some(child) = self.child.as_mut() {
            child.paint(window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::point;

    #[test]
    fn a_float_sits_above_its_anchors_foot_inset_from_its_sides_and_never_above_its_top() {
        let cell = Bounds::new(point(px(100.0), px(50.0)), size(px(400.0), px(300.0)));
        let at = placed(cell, px(12.0), px(58.0), px(640.0), px(80.0));
        assert_eq!(at, Bounds::new(point(px(112.0), px(212.0)), size(px(376.0), px(80.0))));
        let wide = Bounds::new(point(px(0.0), px(0.0)), size(px(1000.0), px(300.0)));
        assert_eq!(placed(wide, px(12.0), px(58.0), px(640.0), px(80.0)).origin.x, px(180.0), "centred at its widest");
        let short = placed(cell, px(12.0), px(58.0), px(640.0), px(400.0));
        assert_eq!(short.origin.y, px(50.0), "a float taller than its place starts at the top");
    }
}
