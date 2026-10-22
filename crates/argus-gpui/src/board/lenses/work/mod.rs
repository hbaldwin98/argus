//! The Work lens (BOARD-WORK-1): the outline of tasks and decisions on the left, the row under the
//! cursor in full on the right, and a sash between them that sizes the outline.
//!
//! Core builds both (`lenses::work`): the rows with their depth and state, and the task or decision under
//! the cursor. This lays them out and sends what is pressed; the outline's width is Rust's, reported once
//! when the sash is released, and a double-click on it gives the usual width back.

pub mod legend;
pub mod marks;
pub mod outline;
pub mod row;
pub mod task;

use argus_desktop_core::connection::page::FeatureView;
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, AnyElement, Entity, TestSupportExt as _};

use crate::board::feature_view::columns::{self, set};
use crate::kit::area;
use crate::board::feature_view::OpenFeature;
use crate::board::lenses::decision;
use crate::board::lenses::forms::edit::Editing;
use crate::kit::caption::meta;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// What the right side says with nothing under the cursor (BOARD-WORK-18).
const NOTHING: &str = "Choose a task or a decision to read it here.";

/// The detail beside the outline: the task, the decision, or what to do.
pub fn detail(this: &OpenFeature, view: &FeatureView, open: Option<Editing>, feature: &Entity<OpenFeature>, tokens: &Tokens, cx: &gpui_kit::App) -> AnyElement {
    if let Some(task) = &view.task {
        return task::detail(view, task, feature, &this.form, open, tokens, cx);
    }
    if let Some(decision) = &view.decision {
        let chosen = this.chosen.filter(|(id, _)| *id == decision.id).map(|(_, option)| option);
        return decision::detail(decision, feature, &this.form, open, chosen, tokens, cx);
    }
    div().id("work-detail").test_support().aria_label(NOTHING).flex_1().min_w_0().py(rem(tokens.sp_16)).child(meta(tokens).child(NOTHING)).into_any_element()
}

/// The lens: the outline at its width with the sash on its right edge, the detail beside it.
pub fn lens(this: &OpenFeature, view: &FeatureView, open: Option<Editing>, feature: &Entity<OpenFeature>, tokens: &Tokens, cx: &gpui_kit::App) -> AnyElement {
    let held = this.outline.read(cx).dragging().then(|| this.outline.read(cx).width());
    let width = columns::width(this.heard.read(cx).rail.outline_width, held, tokens);
    // The sash is a zone straddling the edge, as wide as the page's, and a child of the detail so it is drawn over the outline's edge.
    let sash = div().absolute().top_0().bottom_0().left(rem(-tokens.sp_12)).w(rem(tokens.split_sash_w)).child(this.outline.clone());
    h_flex()
        .size_full()
        .min_h_0()
        .child(set(div().relative().h_full().px(rem(tokens.sp_12)).border_r_1().border_color(tokens.line).child(area::measure(this.outline_area.clone())).child(outline::outline(this, view, open, feature, tokens, cx)), width))
        .child(columns::detail(tokens).child(detail(this, view, open, feature, tokens, cx)).child(sash))
        .into_any_element()
}
