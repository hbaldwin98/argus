//! The Record lens (BOARD-RECORD-1 to -5, -11, -12): every decision in the order it was made, as the tree
//! it was made in, beside the one under the cursor read as its record.
//!
//! What a decision says, and the forms that change it, are `decision`'s, since the Work lens reads a
//! decision under its cursor the same way. The list is core's (`RecordLens`); a click puts the cursor on
//! a row and a double-click opens its record to be edited.

use argus_desktop_core::connection::page::{Call, FeatureView, RecordRow};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, Entity, SharedString, TestSupportExt as _};

use crate::board::feature_view::columns::{self, set};
use crate::board::feature_view::OpenFeature;
use crate::board::lenses::decision;
use crate::board::lenses::forms::edit::Editing;
use crate::board::lenses::forms::view as form;
use crate::board::lenses::work::marks::{state_mark, Tone};
use crate::kit::button::{button, Look};
use crate::kit::caption::{label, meta, note};
use crate::kit::selected::selected;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// What the list says with nothing decided (BOARD-RECORD-3).
const EMPTY: &str = "Nothing decided yet: raise a decision, made already or still open.";
const NOTHING: &str = "Choose a decision to read it as a record.";

/// Whether the decision is out of the record's running: its row is faint and struck through.
pub fn out(state: &str) -> bool {
    matches!(state, "superseded" | "rejected" | "withdrawn")
}

/// How a row's label is coloured: an open one's in attention, one that is out faint, else the link colour.
pub fn label_tone(state: &str) -> Tone {
    match state {
        "open" => Tone::Attention,
        state if out(state) => Tone::Faint,
        _ => Tone::Ref,
    }
}

/// What a row says at its end: its state, blank for a decision made.
pub fn state_word(state: &str) -> &str {
    if state == "accepted" {
        ""
    } else {
        state
    }
}

fn row(ix: usize, row: &RecordRow, feature: &Entity<OpenFeature>, tokens: &Tokens) -> AnyElement {
    let (mark, tone) = state_mark(row.state);
    let (selecting, editing, id) = (feature.clone(), feature.clone(), row.id);
    let faint = out(row.state);
    let line = h_flex()
        .id(("rrow", ix))
        .test_support()
        .aria_label(SharedString::from(format!("{} {}", row.label, row.text)))
        .aria_selected(row.on)
        .flex_shrink_0()
        .min_h(rem(tokens.control_h))
        .pl(rem(tokens.sp_8 + row.depth as f32 * tokens.row_indent))
        .pr(rem(tokens.sp_8))
        .gap(rem(tokens.sp_6))
        .items_center()
        .rounded(rem(tokens.r_sm))
        .cursor_pointer()
        .text_color(tokens.fg)
        .when(!row.on, |line| line.hover(|line| line.bg(tokens.fill)))
        .child(div().flex_shrink_0().w(rem(tokens.wrow_mark_w)).text_color(tone.colour(tokens)).child(mark))
        .child(div().flex_shrink_0().min_w(rem(tokens.rrow_label_min)).font_family(SharedString::from(tokens.font_mono)).text_size(rem(tokens.fs_sm)).text_color(label_tone(row.state).colour(tokens)).child(SharedString::from(row.label.clone())))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .when(row.on && !faint, |text| text.font_weight(gpui_kit::FontWeight(tokens.weight_semibold)))
                .when(faint, |text| text.text_color(tokens.fg_faint).line_through())
                .child(SharedString::from(row.text.clone())),
        )
        .child(meta(tokens).flex_shrink_0().when(row.state == "open", |word| word.text_color(tokens.attention)).child(state_word(row.state).to_owned()))
        .on_click(move |event, window, cx| {
            selecting.update(cx, |feature, cx| feature.send(Call::SelectDecision { id }, cx));
            if event.click_count() >= 2 {
                editing.update(cx, |feature, cx| feature.edit_cursor_decision(window, cx));
            }
        });
    selected(line, row.on, tokens).into_any_element()
}

/// The lens.
pub fn lens(this: &OpenFeature, view: &FeatureView, open: Option<Editing>, feature: &Entity<OpenFeature>, tokens: &Tokens, cx: &gpui_kit::App) -> AnyElement {
    let held = this.outline.read(cx).dragging().then(|| this.outline.read(cx).width());
    let width = columns::width(this.heard.read(cx).rail.outline_width, held, tokens);
    let rows = view.record.as_ref().map(|record| record.rows.as_slice()).unwrap_or_default();
    let adding = matches!(open, Some(Editing::AddDecision { task: None, .. }));
    let (adding_decision, form_entity) = (feature.clone(), this.form.clone());
    let list = v_flex()
        .id("record-list")
        .test_support()
        .aria_label("Decisions")
        .size_full()
        .min_w_0()
        .py(rem(tokens.sp_16))
        .gap(rem(tokens.sp_2))
        .overflow_y_scroll()
        .track_scroll(&this.scroll.record)
        .child(h_flex().px(rem(tokens.sp_8)).pb(rem(tokens.sp_6)).child(label("Decisions", tokens)))
        .when(rows.is_empty(), |list| list.child(note("record-note", EMPTY, tokens).px(rem(tokens.sp_8))))
        .children(rows.iter().enumerate().map(|(ix, record)| row(ix, record, feature, tokens)))
        .when(adding, |list| {
            let words = if matches!(open, Some(Editing::AddDecision { under: None, .. })) { "Decision on the feature" } else { "Sub-decision" };
            list.child(
                v_flex()
                    .id("record-add")
                    .test_support()
                    .aria_label(words)
                    .flex_shrink_0()
                    .p(rem(tokens.sp_8))
                    .gap(rem(tokens.sp_4))
                    .child(label(words, tokens))
                    .child(form::line_field(&form_entity, "New decision", tokens, cx))
                    .child(form::buttons(&form_entity, open.unwrap_or(Editing::Brief), tokens)),
            )
        })
        .child(
            h_flex()
                .id("record-foot")
                .flex_shrink_0()
                .mt_auto()
                .px(rem(tokens.sp_8))
                .pt(rem(tokens.sp_8))
                .border_t_1()
                .border_color(tokens.line)
                .child(div().flex_1())
                .child(
                    button("record-add-decision", "+ Decision", Look::Plain, false, tokens)
                        .child("+ Decision")
                        .on_click(move |_, window, cx| adding_decision.update(cx, |feature, cx| feature.begin(Editing::AddDecision { under: None, task: None }, window, cx))),
                ),
        );
    let detail = match view.record.as_ref().and_then(|record| record.decision.as_ref()) {
        Some(decision) => {
            let chosen = this.chosen.filter(|(id, _)| *id == decision.id).map(|(_, option)| option);
            decision::detail(decision, feature, &this.form, open, chosen, tokens, cx)
        }
        None => div().id("record-detail").test_support().aria_label(NOTHING).flex_1().min_w_0().py(rem(tokens.sp_16)).child(meta(tokens).child(NOTHING)).into_any_element(),
    };
    h_flex().size_full().min_h_0().child(set(div().h_full().px(rem(tokens.sp_12)).border_r_1().border_color(tokens.line).child(list), width)).child(columns::detail(tokens).child(detail)).into_any_element()
}

/// The child of the list the cursor's row is, to scroll to: after the label.
pub fn lit_child(view: &FeatureView) -> Option<usize> {
    let at = view.record.as_ref()?.rows.iter().position(|row| row.on)?;
    Some(1 + at)
}

#[cfg(test)]
mod tests;
