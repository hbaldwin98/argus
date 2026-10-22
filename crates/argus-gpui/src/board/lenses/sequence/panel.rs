//! The steps beside the expanded diagram (BOARD-SEQ-16): the steps list, and under it the step read
//! whole with what can be done to it: its participants, which move a lifeline left or right, what binds
//! it and carries it out, and its buttons; or the form editing the step or the flow in its place.
//!
//! The steps list is the Flows lens's (`flows::steps`); a participant's arrows send `MoveParticipant`
//! naming the lifeline as the source calls it, to the place beside its own, and are held off at the ends.

use argus_desktop_core::connection::page::{FeatureEdit, FlowDetail};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, Entity, SharedString, TestSupportExt as _};

use crate::board::feature_view::OpenFeature;
use crate::board::lenses::flows::steps;
use crate::board::lenses::forms::edit::Editing;
use crate::board::lenses::work::task::{line_list, titled};
use crate::kit::button::{button, Look};
use crate::kit::caption::{aside, label};
use crate::kit::icon_button::icon_button;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// Where a participant is, and whether it can move left or right: its place among the lifelines (none if
/// the step names one the diagram has not), and the place a move would put it.
pub fn moves(flow: &FlowDetail, label: &str) -> (Option<usize>, Option<usize>, Option<usize>) {
    let count = flow.diagram.lifelines.len();
    let at = flow.diagram.lifelines.iter().position(|lifeline| lifeline.label == label);
    let left = at.filter(|&at| at > 0).map(|at| at - 1);
    let right = at.filter(|&at| at + 1 < count).map(|at| at + 1);
    (at, left, right)
}

fn participant(ix: usize, flow: &FlowDetail, words: &str, feature: &Entity<OpenFeature>, tokens: &Tokens) -> AnyElement {
    let (_, left, right) = moves(flow, words);
    let name = flow.diagram.lifelines.iter().find(|lifeline| lifeline.label == words).map(|lifeline| lifeline.name.clone());
    let mover = |id: &'static str, glyph: &'static str, dir: &str, to: Option<usize>| {
        let (feature, name, diagram) = (feature.clone(), name.clone(), flow.id);
        let button = icon_button((id, ix), format!("Move {words} {dir}"), false, tokens).child(glyph);
        match (to, name) {
            (Some(to), Some(name)) => button.on_click(move |_, _, cx| feature.update(cx, |feature, cx| feature.send(argus_desktop_core::connection::page::Call::EditFeature { edit: FeatureEdit::MoveParticipant { diagram, name: name.clone(), to } }, cx))).into_any_element(),
            _ => button.opacity(tokens.opacity_wait).cursor_default().into_any_element(),
        }
    };
    h_flex()
        .items_center()
        .child(div().flex_1().child(SharedString::from(words.to_owned())))
        .child(mover("seq-move-left", "\u{2039}", "left", left))
        .child(mover("seq-move-right", "\u{203a}", "right", right))
        .into_any_element()
}

/// The step in full, with its participants and buttons.
fn step_in_full(flow: &FlowDetail, feature: &Entity<OpenFeature>, bind: AnyElement, tokens: &Tokens) -> Option<AnyElement> {
    let step = flow.step.as_ref()?;
    let (editing, sourcing, id, number) = (feature.clone(), feature.clone(), flow.id, step.number);
    Some(
        v_flex()
            .id("seq-step")
            .test_support()
            .aria_label(SharedString::from(format!("Step {}", step.number)))
            .gap(rem(tokens.sp_12))
            .child(h_flex().items_center().gap(rem(tokens.sp_8)).child(label(format!("Step {}", step.number), tokens)).child(aside(step.route.clone(), None, tokens)))
            .child(div().weighted(tokens.weight_semibold).text_color(tokens.fg).child(SharedString::from(step.text.clone())))
            .child(titled("Participants", v_flex().children(step.participants.iter().enumerate().map(|(ix, words)| participant(ix, flow, words, feature, tokens))), tokens))
            .child(titled(
                "Bound by",
                if step.bound_by.is_empty() { div().text_color(tokens.fg_faint).child("No decision binds it yet.").into_any_element() } else { line_list(&step.bound_by, "\u{25c6}", tokens.ref_, tokens) },
                tokens,
            ))
            .when(!step.carried_out_by.is_empty(), |panel| panel.child(titled("Carried out by", line_list(&step.carried_out_by, "\u{b7}", tokens.fg_faint, tokens), tokens)))
            .child(
                h_flex()
                    .gap(rem(tokens.sp_8))
                    .flex_wrap()
                    .child(button("seq-edit-step", "Edit step", Look::Plain, false, tokens).child("Edit step").on_click(move |_, window, cx| editing.update(cx, |feature, cx| feature.begin(Editing::Step { diagram: id, step: number }, window, cx))))
                    .child(bind)
                    .child(button("seq-edit-source", "Edit source", Look::Plain, false, tokens).child("Edit source").on_click(move |_, window, cx| sourcing.update(cx, |feature, cx| feature.begin(Editing::Flow { id }, window, cx)))),
            )
            .into_any_element(),
    )
}

/// The panel: `aside.expanded-panel`, 340 wide, under a hairline at its left.
pub fn panel(this: &OpenFeature, flow: &FlowDetail, open: Option<Editing>, feature: &Entity<OpenFeature>, bind: AnyElement, tokens: &Tokens, cx: &gpui_kit::App) -> AnyElement {
    let (lit, form_entity) = (&this.lit_row, &this.form);
    let below = if open == Some(Editing::Flow { id: flow.id }) {
        Some(steps::source_form(flow, form_entity, tokens, cx))
    } else if let Some(step) = flow.step.as_ref().filter(|step| open == Some(Editing::Step { diagram: flow.id, step: step.number })) {
        Some(steps::says_form(step, flow.id, form_entity, tokens, cx))
    } else {
        step_in_full(flow, feature, bind, tokens)
    };
    v_flex()
        .id("seq-panel")
        .test_support()
        .aria_label("Steps")
        .flex_shrink_0()
        .w(rem(tokens.seq_panel_w))
        .h_full()
        .p(rem(tokens.sp_16))
        .gap(rem(tokens.sp_12))
        .border_l_1()
        .border_color(tokens.line)
        .overflow_y_scroll()
        .child(h_flex().items_center().child(label("Steps", tokens)).child(div().flex_1()).child(aside("drag an arrow to reorder", None, tokens)))
        .child(v_flex().gap(rem(tokens.sp_2)).children(flow.rows.iter().enumerate().map(|(ix, row)| steps::row(ix, flow, row, lit, feature, tokens))))
        .children(below)
        .into_any_element()
}
