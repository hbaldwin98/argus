//! The two diagrams of an open feature as the view sets them (BOARD-FLOWS-12, BOARD-SEQ-9, -10): what each
//! is told to show, scaled to its box or fitted to the canvas, and the scroll that brings the lit arrow
//! into view when the cursor has moved.

use argus_desktop_core::board::lenses::picture::{inline_scale, layout, Picture};
use argus_desktop_core::connection::page::{FeatureView, Lens};
use gpui_kit::{point, Context, Pixels, Window};

use super::{Expanded, OpenFeature};
use crate::board::lenses::flows::picture;
use crate::theme::scale::css;
use crate::theme::Tokens;

impl OpenFeature {

    /// Scrolls the diagram to the lit arrow if it is out of its box: sideways in the inline holder,
    /// both ways on the expanded canvas (BOARD-FLOWS-12, BOARD-SEQ-10).
    pub(super) fn bring_diagram(&mut self, view: &FeatureView, window: &Window, tokens: &Tokens) {
        use argus_desktop_core::board::lenses::picture::{scroll_into, INLINE_PADDING};
        let Some(flow) = view.flows.as_ref().and_then(|flows| flows.flow.as_ref()).filter(|_| view.lens == Lens::Flows) else { return };
        let picture = layout(&flow.diagram);
        let Some(arrow) = picture.arrows.iter().find(|arrow| arrow.on) else { return };
        let rem_size = window.rem_size();
        let to_real = |css_px: f32| crate::theme::scale::pixels(css_px, rem_size);
        let (from, to) = picture::reach(arrow);
        let holder = css(self.holder_area.get().size.width, rem_size);
        if holder > 0.0 {
            let scale = inline_scale(holder, picture.width);
            let offset = -css(self.scroll.holder.offset().x, rem_size);
            let at = picture::lit_offset(offset, holder - INLINE_PADDING, &picture, scale, INLINE_PADDING);
            if at != offset {
                let y = self.scroll.holder.offset().y;
                self.scroll.holder.set_offset(point(-to_real(at), y));
            }
        }
        if let Some(held) = self.expanded {
            let canvas = self.canvas_area.get().size;
            if canvas.width > Pixels::ZERO {
                let pad = tokens.sp_16;
                let current = self.scroll.canvas.offset();
                let (view_w, view_h) = (css(canvas.width, rem_size), css(canvas.height, rem_size));
                let x = scroll_into(-css(current.x, rem_size), view_w, pad + from * held.zoom, pad + to * held.zoom, 0.0);
                let y = scroll_into(-css(current.y, rem_size), view_h, pad + (arrow.y - 12.0) * held.zoom, pad + (arrow.y + 12.0) * held.zoom, 0.0);
                self.scroll.canvas.set_offset(point(-to_real(x), -to_real(y)));
            }
        }
    }

    /// Tells the two diagrams what to show: the inline one scaled to its box, the expanded one at its
    /// zoom, which is the fit of the canvas while it is fitted.
    pub(super) fn show_diagrams(&mut self, view: &FeatureView, window: &Window, cx: &mut Context<Self>) {
        let flow = view.flows.as_ref().and_then(|flows| flows.flow.as_ref()).filter(|_| view.lens == Lens::Flows);
        let Some(flow) = flow else {
            self.inline.update(cx, |seq, cx| seq.show(None, cx));
            self.big.update(cx, |seq, cx| seq.show(None, cx));
            return;
        };
        let rem_size = window.rem_size();
        let picture: Picture = layout(&flow.diagram);
        let holder = css(self.holder_area.get().size.width, rem_size);
        let scale = if holder > 0.0 { inline_scale(holder, picture.width) } else { 1.0 };
        self.inline.update(cx, |seq, cx| seq.show(Some(picture::shown(flow, scale)), cx));
        match self.expanded {
            Some(held) => {
                let canvas = self.canvas_area.get().size;
                let size = (css(canvas.width, rem_size), css(canvas.height, rem_size));
                let zoom = held.zoom_in(size, (picture.width, picture.height));
                if held.fit && size != self.canvas_seen.get() {
                    // Measured once it is on screen: the next frame draws the fit.
                    self.canvas_seen.set(size);
                    cx.notify();
                }
                self.expanded = Some(Expanded { zoom, ..held });
                self.big.update(cx, |seq, cx| seq.show(Some(picture::shown(flow, zoom)), cx));
            }
            None => self.big.update(cx, |seq, cx| seq.show(None, cx)),
        }
    }
}
