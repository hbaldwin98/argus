//! The diagram expanded over the window (BOARD-SEQ-7, -9, -10): a toolbar, the diagram on a canvas that
//! scrolls both ways, and by default the steps beside it.
//!
//! It covers the whole window, as the page's `.expanded` does (`inset: 0` over the app), over the strip
//! and the rail, and is put away by Collapse, Escape or a crumb. Fit is the canvas's room over the
//! diagram's, measured on the frame after it is drawn; the zoom shown is the one drawn.

use argus_desktop_core::connection::page::FeatureView;
use gpui_kit::component::h_flex;
use gpui_kit::{anchored, deferred, div, point, prelude::*, px, AnyElement, Entity, TestSupportExt as _, Window};

use super::{panel::panel, toolbar::toolbar};
use crate::board::feature_view::OpenFeature;
use crate::board::lenses::forms::edit::Editing;
use crate::kit::area;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The overlay for the expanded diagram of the open feature's flow, or nothing when none is expanded.
pub fn overlay(this: &OpenFeature, view: &FeatureView, open: Option<Editing>, feature: &Entity<OpenFeature>, window: &Window, tokens: &Tokens, cx: &gpui_kit::App) -> Option<AnyElement> {
    let held = this.expanded?;
    let flow = view.flows.as_ref()?.flow.as_ref()?;
    let bind = {
        let feature = feature.clone();
        crate::kit::button::button("seq-bind", "Bind a decision\u{2026}", crate::kit::button::Look::Plain, false, tokens).child("Bind a decision\u{2026}").on_click(move |event, window, cx| {
            let at = event.position();
            feature.update(cx, |feature, cx| feature.open_bind(Some(at), true, window, cx))
        })
    };
    let canvas = div()
        .id("seq-canvas")
        .test_support()
        .relative()
        .flex_1()
        .min_w_0()
        .h_full()
        .overflow_scroll()
        .track_scroll(&this.scroll.canvas)
        .bg(tokens.bg_sunken)
        .p(rem(tokens.sp_16))
        .child(area::measure(this.canvas_area.clone()))
        .child(div().flex_shrink_0().child(this.big.clone()));
    let body = h_flex().flex_1().min_h_0().child(canvas).when(held.panel, |body| body.child(panel(this, flow, open, feature, bind.into_any_element(), tokens, cx)));
    let size = window.bounds().size;
    Some(
        deferred(
            anchored().position(point(px(0.0), px(0.0))).child(
                gpui_kit::component::v_flex()
                    .id("expanded")
                    .test_support()
                    .aria_label(gpui_kit::SharedString::from(flow.title.clone()))
                    .occlude()
                    .w(size.width)
                    .h(size.height)
                    .bg(tokens.bg_surface)
                    .text_color(tokens.fg)
                    .child(toolbar(view, flow, held, feature, tokens))
                    .child(body),
            ),
        )
        .with_priority(1)
        .into_any_element(),
    )
}
