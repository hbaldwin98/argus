//! The picture beside the flows (BOARD-FLOWS-11, -12): the flow's title and the button that expands it
//! over the window, and under them the diagram in a box that scrolls sideways.
//!
//! The diagram is scaled to fit the box's width, never below 70% nor above its own size; when it still
//! overflows, the lit step is scrolled into view only if it is outside the box, 24px past the edge. The
//! scaling is the page's (`fitted` in `flows.ts`), in core as `picture::inline_scale`.

use std::rc::Rc;

use argus_desktop_core::board::lenses::picture::{layout, scroll_into, Picture};
use argus_desktop_core::connection::page::{FeatureView, FlowDetail};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, Entity, SharedString, TestSupportExt as _};

use crate::board::feature_view::act::Act;
use crate::board::feature_view::OpenFeature;
use crate::board::lenses::sequence::Shown;
use crate::kit::area;
use crate::kit::button::{button, Look};
use crate::kit::caption::{label, meta};
use crate::kit::icons::Glyph;
use crate::kit::keycap::keycap;
use crate::kit::tip::tip;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// What the diagram is showing for `flow`, at `scale`.
pub fn shown(flow: &FlowDetail, scale: f32) -> Shown {
    let picture: Rc<Picture> = Rc::new(layout(&flow.diagram));
    Shown { picture, diagram: flow.id, title: flow.title.clone(), names: flow.diagram.lifelines.iter().map(|l| l.name.clone()).collect(), scale }
}

/// The span an arrow takes in the diagram's units, its words included: the page brings its `.smsg` group,
/// text and all, into view (`fitted` in `flows.ts`), and a self-call's words run on past its loop.
pub fn reach(arrow: &argus_desktop_core::board::lenses::picture::Arrow) -> (f32, f32) {
    use argus_desktop_core::board::lenses::picture::{Line, LOOP_OUT};
    match arrow.line {
        Line::Across { from, to, .. } => (from.min(to), from.max(to)),
        Line::Loop { x, .. } => (x, x + LOOP_OUT + 24.0 + arrow.text.chars().count() as f32 * 7.2),
    }
}

/// How far the holder is scrolled sideways to show the lit step if it is outside it: the arrow's reach in
/// the diagram's units, `scale` times as big, in a box `view` wide with `offset` gone off its start.
pub fn lit_offset(offset: f32, view: f32, picture: &Picture, scale: f32, margin: f32) -> f32 {
    let Some(arrow) = picture.arrows.iter().find(|arrow| arrow.on) else { return offset };
    let (from, to) = reach(arrow);
    scroll_into(offset, view, from * scale, to * scale, margin)
}

/// The article: its head and the holder.
pub fn article(this: &OpenFeature, view: &FeatureView, feature: &Entity<OpenFeature>, tokens: &Tokens) -> AnyElement {
    let Some(flow) = view.flows.as_ref().and_then(|flows| flows.flow.as_ref()) else {
        // BOARD-FLOWS-11: with no flow the pane says what to do.
        return div().id("flow-picture").test_support().aria_label("Choose a flow to draw it here.").size_full().py(rem(tokens.sp_16)).child(meta(tokens).child("Choose a flow to draw it here.")).into_any_element();
    };
    let expanding = feature.clone();
    v_flex()
        .id("flow-picture")
        .test_support()
        .aria_label(SharedString::from(flow.title.clone()))
        .size_full()
        .py(rem(tokens.sp_16))
        .gap(rem(tokens.sp_16))
        .child(
            h_flex().items_center().gap(rem(tokens.sp_8)).child(label(flow.title.clone(), tokens)).child(div().flex_1()).child(
                button("flow-expand", "Expand", Look::Plain, false, tokens)
                    .tooltip(tip("Expand over the window"))
                    .child(Glyph::Expand.at(tokens.icon_sm))
                    .child(" Expand ")
                    .child(keycap("F", SharedString::from(tokens.font_mono), tokens))
                    .on_click(move |_, window, cx| expanding.update(cx, |feature, cx| feature.act(Act::Expand, window, cx))),
            ),
        )
        .child(
            div()
                .id("seq-holder")
                .flex()
                .test_support()
                .relative()
                .flex_1()
                .min_h_0()
                .overflow_x_scroll()
                .track_scroll(&this.scroll.holder)
                .bg(tokens.bg_sunken)
                .border_1()
                .border_color(tokens.line)
                .rounded(rem(tokens.r_md))
                .child(area::measure(this.holder_area.clone()))
                .child(div().flex_shrink_0().p(rem(tokens.sp_12)).child(this.inline.clone())),
        )
        .into_any_element()
}
