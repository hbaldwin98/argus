//! A flow's steps and the step under the cursor (BOARD-FLOWS-3, -4, -6, -7): a numbered row for each step
//! with its route, its words and what binds it, each branch between them a row that is not pressed, and
//! under them the step read whole: what binds it, what carries it out, and the buttons that edit it.
//!
//! The rows are core's (`StepRow`, `StepDetail`); a click puts the cursor on a step. The step's own forms,
//! the words of a step and the flow's source, replace the detail while they are written.

use std::cell::Cell;
use std::rc::Rc;

use argus_desktop_core::connection::page::{Call, FlowDetail, StepDetail, StepRow};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, Bounds, Entity, Pixels, SharedString, TestSupportExt as _};

use crate::board::feature_view::act::Act;
use crate::board::feature_view::more::overflow;
use crate::board::feature_view::OpenFeature;
use crate::board::lenses::forms::edit::Editing;
use crate::board::lenses::forms::view as form;
use crate::board::lenses::forms::Form;
use crate::board::lenses::work::task::{line_list, titled};
use crate::kit::area;
use crate::kit::button::{button, Look};
use crate::kit::caption::{meta, note};
use crate::kit::chips::tkey;
use crate::kit::menu::item::MenuItem;
use crate::kit::selected::selected;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// How wide the route column is: the page's `.steprow` grid gives each row `minmax(80px, 150px)` and a track
/// with room grows to its maximum, so every row's words start in one place and the longest route is whole.
/// It shrinks toward the minimum only in a row with no room (the rows set `min_w`).
pub fn route_width(tokens: &crate::theme::scale::Scale) -> f32 {
    tokens.steprow_route_max
}

/// One line of the steps: a step the cursor can be put on, or the branch the flow takes between them.
pub fn row(ix: usize, flow: &FlowDetail, row: &StepRow, lit: &Rc<Cell<Bounds<Pixels>>>, feature: &Entity<OpenFeature>, tokens: &Tokens) -> AnyElement {
    let route_w = route_width(tokens);
    let Some(number) = row.number else {
        // `.steprow.branch`: its word in attention, then when it applies; not pressed.
        return h_flex()
            .id(("steprow", ix))
            .test_support()
            .aria_label(SharedString::from(format!("{} {}", row.route, row.text)))
            .flex_shrink_0()
            .min_h(rem(tokens.branch_row_h))
            .px(rem(tokens.sp_8))
            .gap(rem(tokens.sp_6))
            .items_center()
            .child(div().flex_shrink_0().w(rem(tokens.steprow_n_w)))
            .child(div().min_w(rem(tokens.steprow_route_min)).w(rem(route_w)).font_family(SharedString::from(tokens.font_mono)).text_color(tokens.attention).child(SharedString::from(row.route.clone())))
            .child(div().flex_1().min_w_0().text_color(tokens.fg_muted).child(SharedString::from(row.text.clone())))
            .into_any_element();
    };
    let (selecting, diagram) = (feature.clone(), flow.id);
    let line = h_flex()
        .id(("steprow", ix))
        .test_support()
        .aria_label(SharedString::from(format!("{number} {} {}", row.route, row.text)))
        .aria_selected(row.on)
        .relative()
        .flex_shrink_0()
        .min_h(rem(tokens.row_h))
        .px(rem(tokens.sp_8))
        .gap(rem(tokens.sp_6))
        .items_center()
        .rounded(rem(tokens.r_sm))
        .cursor_pointer()
        .text_color(tokens.fg)
        .when(!row.on, |line| line.hover(|line| line.bg(tokens.fill)))
        .when(row.on, |line| line.child(area::measure(lit.clone())))
        .child(div().flex_shrink_0().w(rem(tokens.steprow_n_w)).font_family(SharedString::from(tokens.font_mono)).text_size(rem(tokens.fs_sm)).text_color(if row.on { tokens.accent } else { tokens.fg_faint }).child(SharedString::from(number.to_string())))
        .child(div().min_w(rem(tokens.steprow_route_min)).w(rem(route_w)).truncate().font_family(SharedString::from(tokens.font_mono)).text_size(rem(tokens.fs_sm)).text_color(tokens.fg_muted).child(SharedString::from(row.route.clone())))
        .child(
            h_flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap(rem(tokens.sp_8))
                .child(div().min_w_0().truncate().when(row.on, |text| text.weighted(tokens.weight_semibold)).child(SharedString::from(row.text.clone())))
                .children(row.tags.iter().map(|tag| tkey(SharedString::from(tag.clone()), tokens))),
        )
        .on_click(move |_, _, cx| selecting.update(cx, |feature, cx| feature.send(Call::SelectStep { diagram, step: Some(number - 1) }, cx)));
    selected(line, row.on, tokens).into_any_element()
}

/// What stands behind a step: the decisions it is bound by and what carries it out, or that nothing binds it.
pub fn standing(step: &StepDetail, tokens: &Tokens) -> Vec<AnyElement> {
    let nothing = step.bound_by.is_empty() && step.carried_out_by.is_empty();
    let mut out = Vec::new();
    if nothing {
        out.push(note("step-note", format!("Nothing binds step {} yet.", step.number), tokens).into_any_element());
    }
    if !step.bound_by.is_empty() {
        out.push(titled(format!("Step {} is bound by", step.number), line_list(&step.bound_by, "\u{25c6}", tokens.ref_, tokens), tokens));
    }
    if !step.carried_out_by.is_empty() {
        out.push(titled("Carried out by", line_list(&step.carried_out_by, "\u{b7}", tokens.fg_faint, tokens), tokens));
    }
    out
}

/// The step's words as a form (BOARD-FLOWS-6).
pub fn says_form(step: &StepDetail, diagram: i64, form_entity: &Entity<Form>, tokens: &Tokens, cx: &gpui_kit::App) -> AnyElement {
    let words = SharedString::from(format!("Step {} says", step.number));
    v_flex()
        .id("step-form")
        .test_support()
        .aria_label(SharedString::from(format!("Edit step {}", step.number)))
        .gap(rem(tokens.sp_6))
        .child(titled(words, form::line_field(form_entity, "what is asked or answered", tokens, cx), tokens))
        .child(form::buttons(form_entity, Editing::Step { diagram, step: step.number }, tokens))
        .into_any_element()
}

/// The flow's source as a form (BOARD-FLOWS-7): its title in a line, the Mermaid in a tall area.
pub fn source_form(flow: &FlowDetail, form_entity: &Entity<Form>, tokens: &Tokens, cx: &gpui_kit::App) -> AnyElement {
    v_flex()
        .id("flow-form")
        .test_support()
        .aria_label("Edit the flow")
        .gap(rem(tokens.sp_6))
        .child(titled("Title", form::line_field(form_entity, "What the flow shows", tokens, cx), tokens))
        .child(titled("Mermaid source", form::area_field(form_entity, "sequenceDiagram", true, tokens, cx), tokens))
        .child(
            meta(tokens)
                .flex()
                .flex_wrap()
                .child("A comment under a step, ")
                .child(div().font_family(SharedString::from(tokens.font_mono)).child("%% decision #4"))
                .child(", binds it."),
        )
        .child(form::buttons(form_entity, Editing::Flow { id: flow.id }, tokens))
        .into_any_element()
}

/// Under the steps (BOARD-FLOWS-4): the step under the cursor and what stands behind it with its buttons,
/// or the form editing it or its flow. `bind` is the button that opens the bind menu.
pub fn below(flow: &FlowDetail, open: Option<Editing>, feature: &Entity<OpenFeature>, form_entity: &Entity<Form>, bind: AnyElement, tokens: &Tokens, cx: &gpui_kit::App) -> Option<AnyElement> {
    if open == Some(Editing::Flow { id: flow.id }) {
        return Some(source_form(flow, form_entity, tokens, cx));
    }
    let step = flow.step.as_ref()?;
    if open == Some(Editing::Step { diagram: flow.id, step: step.number }) {
        return Some(says_form(step, flow.id, form_entity, tokens, cx));
    }
    let (editing, id, number) = (feature.clone(), flow.id, step.number);
    Some(
        v_flex()
            .id("step-detail")
            .test_support()
            .aria_label(SharedString::from(format!("Step {}", step.number)))
            .gap(rem(tokens.sp_8))
            .pt(rem(tokens.sp_12))
            .px(rem(tokens.sp_8))
            .children(standing(step, tokens))
            .child(
                h_flex()
                    .items_center()
                    .gap(rem(tokens.sp_8))
                    .child(div().flex_1())
                    .child(button("step-edit", "Edit step", Look::Plain, true, tokens).child("Edit step").on_click(move |_, window, cx| editing.update(cx, |feature, cx| feature.begin(Editing::Step { diagram: id, step: number }, window, cx))))
                    .child(bind)
                    .child(overflow("step-more", "More: edit source", vec![MenuItem::choice("Edit source", Act::Begin(Editing::Flow { id }))], feature, tokens, cx)),
            )
            .into_any_element(),
    )
}
