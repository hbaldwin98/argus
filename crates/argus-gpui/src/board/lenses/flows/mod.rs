//! The Flows lens (BOARD-FLOWS-1 to -5, -11, -12): a feature's flows and the selected one's steps on the
//! left with the step under the cursor read whole under them, and the sequence diagram on the right,
//! which expands over the window.
//!
//! Core reads the flows and the steps (`lenses::flows`) and lays the diagram out (`lenses::picture`); a
//! click on a flow or a step puts the cursor there, and a binding, a step's words or a flow's source is
//! sent as the flow rewritten, shown when the pushed flows say so. Flows arrive after the board: until
//! the daemon answers the list says it is reading them.

pub mod bind;
pub mod picture;
pub mod steps;

use argus_desktop_core::connection::page::{Call, FeatureView, FlowItem};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, Entity, SharedString, TestSupportExt as _};

use crate::board::feature_view::columns::{self, set};
use crate::board::feature_view::OpenFeature;
use crate::board::lenses::forms::edit::Editing;
use crate::board::lenses::forms::view as form;
use crate::kit::button::{button, Look};
use crate::kit::caption::{label, meta, note};
use crate::kit::selected::selected;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// What the list says before the flows have come (BOARD-FLOWS-2).
const READING: &str = "Reading its flows\u{2026}";

/// A flow's row: its title, strong while it is the one read, and what it holds.
fn flow_row(ix: usize, item: &FlowItem, feature: &Entity<OpenFeature>, tokens: &Tokens) -> AnyElement {
    let (selecting, id) = (feature.clone(), item.id);
    let line = h_flex()
        .id(("frow", ix))
        .test_support()
        .aria_label(SharedString::from(format!("{} {}", item.title, item.summary)))
        .aria_selected(item.on)
        .flex_shrink_0()
        .min_h(rem(tokens.control_h))
        .px(rem(tokens.sp_8))
        .gap(rem(tokens.sp_8))
        .items_center()
        .justify_between()
        .rounded(rem(tokens.r_sm))
        .cursor_pointer()
        .text_color(tokens.fg)
        .when(!item.on, |line| line.hover(|line| line.bg(tokens.fill)))
        .child(
            div()
                .min_w_0()
                .truncate()
                .when(item.on, |title| title.font_weight(gpui_kit::FontWeight(tokens.weight_semibold)))
                .when(!item.on, |title| title.text_color(tokens.fg_muted))
                .child(SharedString::from(item.title.clone())),
        )
        .child(meta(tokens).flex_shrink_0().child(SharedString::from(item.summary.clone())))
        .on_click(move |_, _, cx| selecting.update(cx, |feature, cx| feature.send(Call::SelectStep { diagram: id, step: None }, cx)));
    selected(line, item.on, tokens).into_any_element()
}

/// The child of the list the lit step's row is, to scroll to: after the label, the line that says what
/// the list holds when it holds none, each flow, and the label over the steps.
pub fn lit_child(view: &FeatureView) -> Option<usize> {
    let flows = view.flows.as_ref()?;
    let flow = flows.flow.as_ref()?;
    let at = flow.rows.iter().position(|row| row.on)?;
    let said = usize::from(!flows.loaded || flows.flows.is_empty());
    Some(1 + said + flows.flows.len() + 1 + at)
}

/// The lens.
pub fn lens(this: &OpenFeature, view: &FeatureView, open: Option<Editing>, feature: &Entity<OpenFeature>, tokens: &Tokens, cx: &gpui_kit::App) -> AnyElement {
    let held = this.outline.read(cx).dragging().then(|| this.outline.read(cx).width());
    let width = columns::width(this.heard.read(cx).rail.outline_width, held, tokens);
    let flows = view.flows.as_ref();
    let flow = flows.and_then(|flows| flows.flow.as_ref());
    let (adding, form_entity) = (feature.clone(), this.form.clone());
    let count = flows.map_or(0, |flows| flows.flows.len());
    let bind = {
        let feature = feature.clone();
        button("step-bind", "Bind a decision\u{2026}", Look::Plain, true, tokens).child("Bind a decision\u{2026}").on_click(move |event, window, cx| {
            let at = event.position();
            feature.update(cx, |feature, cx| feature.open_bind(Some(at), false, window, cx))
        })
    };
    let list = v_flex()
        .id("flows-list")
        .test_support()
        .aria_label("Flows")
        .size_full()
        .min_w_0()
        .py(rem(tokens.sp_16))
        .gap(rem(tokens.sp_2))
        .overflow_y_scroll()
        .track_scroll(&this.scroll.flows)
        .child(h_flex().px(rem(tokens.sp_8)).pb(rem(tokens.sp_6)).gap(rem(tokens.sp_8)).child(label("Flows", tokens)).child(div().flex_1()).child(label(count.to_string(), tokens)))
        .when(!flows.is_some_and(|flows| flows.loaded), |list| list.child(note("flows-note", READING, tokens).px(rem(tokens.sp_8))))
        .when(flows.is_some_and(|flows| flows.loaded && flows.flows.is_empty()), |list| {
            list.child(
                meta(tokens)
                    .px(rem(tokens.sp_8))
                    .flex()
                    .flex_wrap()
                    .child("No flows yet. Add one from a skeleton, or an agent records one with ")
                    .child(div().font_family(SharedString::from(tokens.font_mono)).child("argus-hook diagram"))
                    .child("."),
            )
        })
        .children(flows.into_iter().flat_map(|flows| flows.flows.iter().enumerate().map(|(ix, item)| flow_row(ix, item, feature, tokens))))
        .when(open == Some(Editing::AddFlow), |list| {
            list.child(
                v_flex()
                    .id("flow-add")
                    .test_support()
                    .aria_label("New flow")
                    .flex_shrink_0()
                    .p(rem(tokens.sp_8))
                    .gap(rem(tokens.sp_4))
                    .child(label("New flow", tokens))
                    .child(form::line_field(&form_entity, "What it shows", tokens, cx))
                    .child(form::buttons(&form_entity, Editing::AddFlow, tokens)),
            )
        })
        .when(flow.is_some(), |list| {
            // A section break like the others in the stage, so the steps do not read as the last row of the flows above them.
            list.child(div().id("steps-label").flex_shrink_0().mt(rem(tokens.sp_16)).px(rem(tokens.sp_8)).pb(rem(tokens.sp_6)).border_t_1().border_color(tokens.line).child(label("Steps", tokens)))
        })
        .children(flow.into_iter().flat_map(|flow| flow.rows.iter().enumerate().map(|(ix, row)| steps::row(ix, flow, row, &this.lit_row, feature, tokens))))
        // Expanded, the forms are the panel's.
        .children(flow.filter(|_| this.expanded.is_none()).and_then(|flow| steps::below(flow, open, feature, &this.form, bind.into_any_element(), tokens, cx)))
        .child(
            h_flex()
                .id("flows-foot")
                .flex_shrink_0()
                .mt_auto()
                .px(rem(tokens.sp_8))
                .pt(rem(tokens.sp_8))
                .border_t_1()
                .border_color(tokens.line)
                .child(div().flex_1())
                .child(button("flows-add", "+ Flow", Look::Plain, false, tokens).child("+ Flow").on_click(move |_, window, cx| adding.update(cx, |feature, cx| feature.begin(Editing::AddFlow, window, cx)))),
        );
    h_flex().size_full().min_h_0().child(set(div().h_full().px(rem(tokens.sp_12)).border_r_1().border_color(tokens.line).child(list), width)).child(columns::detail(tokens).child(picture::article(this, view, feature, tokens))).into_any_element()
}

#[cfg(test)]
mod tests;
