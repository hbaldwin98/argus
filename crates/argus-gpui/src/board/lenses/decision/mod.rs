//! A decision read as its record, in either lens (BOARD-RECORD-4 to -9, BOARD-FORM-12, -13): what made it
//! necessary, what was decided and why, what it costs and buys, what was turned down, and what carries it
//! out; or an open one's options to take. The Work lens reads a decision under its cursor the same way the
//! Record lens does, so this is both's.
//!
//! The body scrolls and the actions stay under it, so a short window cannot push them beneath the key
//! bar. The record's form, the answer in words and the supersede line replace the body while they are
//! written; each sends one edit.

pub mod options;

use argus_desktop_core::connection::page::{DecisionDetail, FeatureEdit};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, Entity, SharedString, TestSupportExt as _};

use crate::board::feature_view::act::{copy_record, Act};
use crate::board::feature_view::more::overflow;
use crate::board::feature_view::OpenFeature;
use crate::board::lenses::forms::edit::Editing;
use crate::board::lenses::forms::view as form;
use crate::board::lenses::forms::Form;
use crate::board::lenses::work::task::{line_list, titled};
use crate::kit::button::{button, off, Look};
use crate::kit::caption::{label, meta, note};
use crate::kit::menu::item::MenuItem;
use crate::kit::pill::status_pill;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// A consequence as it is written: `+` a gain, `-` a cost, and the words after the sign.
pub fn consequence(line: &str) -> (bool, &str) {
    (line.starts_with('+'), line.trim_start_matches(['+', '-']).trim_start())
}

/// `.cmark`: a gain is a round plus and a cost a square minus, so the two differ in shape and sign and
/// not only in colour.
fn mark(gain: bool, tokens: &Tokens) -> AnyElement {
    let colour = if gain { tokens.ok } else { tokens.attention };
    div()
        .flex_shrink_0()
        .mt(rem(tokens.sp_2))
        .size(rem(tokens.check_box))
        .flex()
        .items_center()
        .justify_center()
        .text_size(rem(tokens.fs_sm))
        .weighted(tokens.weight_bold)
        .text_color(colour)
        .bg(colour.opacity(tokens.share_cmark))
        .when(gain, |mark| mark.rounded_full())
        .when(!gain, |mark| mark.rounded(rem(tokens.r_xs)))
        .child(if gain { "+" } else { "\u{2212}" })
        .into_any_element()
}

/// The head: its label, how it stands, who raised it, then what it chose and whether it is out.
fn top(decision: &DecisionDetail, tokens: &Tokens) -> AnyElement {
    let open = decision.state == "open";
    let state = match (open, decision.state) {
        (true, _) => status_pill("decision-state", "needs you", "waiting", tokens).into_any_element(),
        (false, "accepted") => status_pill("decision-state", "decided", "done", tokens).into_any_element(),
        (false, other) => status_pill("decision-state", other, "exited", tokens).into_any_element(),
    };
    let under = decision.raised_under.as_ref().map(|under| format!("raised under {under} \u{b7} ")).unwrap_or_default();
    v_flex()
        .child(
            h_flex()
                .items_center()
                .gap(rem(tokens.sp_8))
                .min_h(rem(tokens.sp_20))
                .child(div().font_family(SharedString::from(tokens.font_mono)).text_color(if open { tokens.attention } else { tokens.ref_ }).child(SharedString::from(format!("#{}", decision.label))))
                .child(state)
                .child(div().flex_1())
                .child(meta(tokens).child(SharedString::from(format!("{under}{}", decision.by)))),
        )
        .child(div().mt(rem(tokens.sp_6)).typed(tokens.type_title).text_color(tokens.fg).child(SharedString::from(decision.chose.clone())))
        .children(decision.superseded_by.clone().map(|by| note("decision-superseded", format!("Superseded by {by}: kept on the record."), tokens)))
        .when(decision.withdrawn, |top| top.child(note("decision-withdrawn", "Withdrawn: kept on the record, and no longer shown to agents.", tokens)))
        .into_any_element()
}

fn prose(words: &str, tokens: &Tokens) -> AnyElement {
    div().text_color(tokens.fg_muted).max_w(rem(tokens.prose_max_w)).child(SharedString::from(words.to_owned())).into_any_element()
}

/// The record's sections: context, the decision and why (or the options of an open one), what it costs
/// and buys, what was considered, and what carries it out.
fn body(decision: &DecisionDetail, feature: &Entity<OpenFeature>, chosen: Option<usize>, tokens: &Tokens) -> Vec<AnyElement> {
    let open = decision.state == "open";
    let mut body = vec![top(decision, tokens)];
    if let Some(context) = &decision.context {
        body.push(titled("Context", prose(context, tokens), tokens));
    }
    if open {
        body.push(options::options(decision, feature, chosen, tokens));
    } else {
        if let Some(detail) = &decision.detail {
            body.push(titled("Decision", prose(detail, tokens), tokens));
        }
        if let Some(because) = &decision.because {
            body.push(titled("Why", prose(because, tokens), tokens));
        }
        if !decision.consequences.is_empty() {
            let lines = v_flex().children(decision.consequences.iter().map(|line| {
                let (gain, words) = consequence(line);
                h_flex()
                    .items_start()
                    .gap(rem(tokens.sp_8))
                    .child(mark(gain, tokens))
                    .child(SharedString::from(words.to_owned()))
            }));
            body.push(titled("Consequences", lines, tokens));
        }
    }
    if decision.over.is_some() || !decision.considered.is_empty() {
        let considered = v_flex()
            .children(decision.over.as_ref().map(|over| prose(over, tokens)))
            .children(decision.considered.iter().map(|line| {
                v_flex()
                    .child(
                        div()
                            .child(div().text_color(tokens.fg_faint).child("\u{2715} "))
                            .child(div().font_family(SharedString::from(tokens.font_mono)).text_color(tokens.fg_faint).child(SharedString::from(line.label.clone())))
                            .child(div().text_color(tokens.fg_muted).line_through().child(SharedString::from(format!(" {}", line.text)))),
                    )
                    .children(line.aside.clone().map(|why| meta(tokens).pl(rem(tokens.sp_20)).child(SharedString::from(format!("rejected: {why}")))))
            }));
        body.push(titled("Considered", considered, tokens));
    }
    if !decision.carried_out_by.is_empty() {
        body.push(titled("Carried out by", line_list(&decision.carried_out_by, "\u{b7}", tokens.fg_faint, tokens), tokens));
    }
    body
}

/// The actions under a decision: its record, answering it, taking an option, and the rarer ones.
fn foot(decision: &DecisionDetail, feature: &Entity<OpenFeature>, chosen: Option<usize>, tokens: &Tokens, cx: &gpui_kit::App) -> AnyElement {
    let open = decision.state == "open";
    let (editing, answering, taking) = (feature.clone(), feature.clone(), feature.clone());
    let id = decision.id;
    let more = vec![
        Some(MenuItem::choice("Sub-decision", Act::Begin(Editing::AddDecision { under: Some(id), task: None }))),
        decision.superseded_by.is_none().then(|| MenuItem::choice("Supersede\u{2026}", Act::Begin(Editing::Supersede { id }))),
        Some(MenuItem::choice("Copy as ADR", Act::Call(copy_record(decision)))),
        Some(MenuItem::Rule),
        Some(if decision.withdrawn {
            MenuItem::choice("Restore", Act::Edit(FeatureEdit::Withdraw { id, withdrawn: false }))
        } else {
            MenuItem::choice("Withdraw", Act::Edit(FeatureEdit::Withdraw { id, withdrawn: true })).danger()
        }),
    ];
    h_flex()
        .id("decision-foot")
        .flex_shrink_0()
        .items_center()
        .gap(rem(tokens.sp_8))
        .child(div().flex_1())
        .child(button("decision-edit", "Edit record", Look::Plain, false, tokens).child("Edit record").on_click(move |_, window, cx| editing.update(cx, |feature, cx| feature.begin(Editing::Record { id }, window, cx))))
        .when(open, |foot| {
            let take = match chosen {
                Some(option) => button("decision-take", format!("Take option {}", option + 1), Look::Primary, false, tokens)
                    .child(SharedString::from(format!("Take option {}", option + 1)))
                    .on_click(move |_, _, cx| taking.update(cx, |feature, cx| feature.send(argus_desktop_core::connection::page::Call::EditFeature { edit: FeatureEdit::Take { id, option } }, cx)))
                    .into_any_element(),
                None => off("decision-take", "Take an option", false, tokens).child("Take an option").into_any_element(),
            };
            foot.child(button("decision-answer", "Answer in words\u{2026}", Look::Plain, false, tokens).child("Answer in words\u{2026}").on_click(move |_, window, cx| answering.update(cx, |feature, cx| feature.begin(Editing::Answer { id }, window, cx))))
                .child(take)
        })
        .child(overflow("decision-more", "More: sub-decision, supersede, copy, withdraw", more.into_iter().flatten().collect(), feature, tokens, cx))
        .into_any_element()
}

/// The form that replaces the body while it is written, if one is open for this decision.
fn written(decision: &DecisionDetail, form_entity: &Entity<Form>, open: Option<Editing>, tokens: &Tokens, cx: &gpui_kit::App) -> Option<AnyElement> {
    let id = decision.id;
    let article = |inner: Vec<AnyElement>| {
        v_flex().id("decision-form").test_support().aria_label(SharedString::from(format!("Decision {}", decision.label))).flex_1().min_w_0().h_full().py(rem(tokens.sp_16)).gap(rem(tokens.sp_16)).overflow_y_scroll().children(inner).into_any_element()
    };
    match open? {
        Editing::Record { id: at } if at == id => Some(article(vec![
            label(format!("Editing {} as its record", decision.label), tokens).text_color(tokens.accent).into_any_element(),
            form::area_field(form_entity, "The decision as a record", true, tokens, cx),
            meta(tokens)
                .flex()
                .flex_wrap()
                .child("The first line is the decision. Each ")
                .child(div().font_family(SharedString::from(tokens.font_mono)).child("## section"))
                .child(" is a part of the record; under Consequences, + is a gain and - a cost.")
                .into_any_element(),
            form::buttons(form_entity, Editing::Record { id }, tokens),
        ])),
        Editing::Answer { id: at } if at == id => Some(article(vec![
            top(decision, tokens),
            titled("Answer", form::area_field(form_entity, "What was decided", false, tokens, cx), tokens),
            titled("Because", form::line_field(form_entity, "Why, if it needs saying", tokens, cx), tokens),
            note("form-note", "The options not taken stay listed as rejected.", tokens).into_any_element(),
            form::buttons(form_entity, Editing::Answer { id }, tokens),
        ])),
        Editing::Supersede { id: at } if at == id => Some(article(vec![
            top(decision, tokens),
            titled(format!("What takes {}'s place", decision.label), form::line_field(form_entity, "What takes its place", tokens, cx), tokens),
            note("form-note", format!("{} stays on the record, dimmed, with what replaced it.", decision.label), tokens).into_any_element(),
            form::buttons(form_entity, Editing::Supersede { id }, tokens),
        ])),
        _ => None,
    }
}

/// The decision, pinned: a body that scrolls over the foot of actions.
pub fn detail(decision: &DecisionDetail, feature: &Entity<OpenFeature>, form_entity: &Entity<Form>, open: Option<Editing>, chosen: Option<usize>, tokens: &Tokens, cx: &gpui_kit::App) -> AnyElement {
    if let Some(form) = written(decision, form_entity, open, tokens, cx) {
        return form;
    }
    v_flex()
        .id("decision-detail")
        .test_support()
        .aria_label(SharedString::from(format!("Decision {}", decision.label)))
        .flex_1()
        .min_w_0()
        .h_full()
        .overflow_hidden()
        .pt(rem(tokens.sp_16))
        .pb(rem(tokens.sp_12))
        .gap(rem(tokens.sp_16))
        .child(v_flex().id("decision-scroll").flex_1().min_h_0().gap(rem(tokens.sp_16)).overflow_y_scroll().children(body(decision, feature, chosen, tokens)))
        .child(foot(decision, feature, chosen, tokens, cx))
        .into_any_element()
}

#[cfg(test)]
mod tests;
