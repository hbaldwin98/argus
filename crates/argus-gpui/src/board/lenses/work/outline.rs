//! The outline's column (BOARD-WORK-1, -2, -7, -8, -13): the Outline label with its legend, the tasks and
//! the decisions raised under them as one list, the form that adds one, and the two buttons that open the
//! root forms at its foot.
//!
//! Until the tasks arrive it says it is reading them, and with none it says what to do. The column scrolls
//! whole, its foot at the bottom of the box while the rows are few; the cursor's row is brought into view
//! when it moves (`feature_view::scroll`), not on every push.

use argus_desktop_core::connection::page::FeatureView;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{prelude::*, AnyElement, Entity, SharedString, TestSupportExt as _};

use super::{legend, marks, row};
use crate::board::feature_view::OpenFeature;
use crate::board::lenses::forms::edit::Editing;
use crate::board::lenses::forms::view as form;
use crate::kit::button::{button, Look};
use crate::kit::caption::{label, meta};
use crate::kit::keycap::keycap;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// What a new row is, said over its field (BOARD-WORK-13).
pub fn adding_words(view: &FeatureView, editing: Editing) -> Option<String> {
    let named = |id: Option<i64>| id.and_then(|id| view.rows.iter().find(|row| matches!(row.at, argus_desktop_core::connection::page::WorkAt::Task { id: at } | argus_desktop_core::connection::page::WorkAt::Decision { id: at } if at == id)).map(|row| row.label.clone())).unwrap_or_default();
    match editing {
        Editing::AddTask { parent: None } => Some("New task".into()),
        Editing::AddTask { parent } => Some(format!("New subtask under {}", named(parent))),
        Editing::AddDecision { under: Some(under), .. } => Some(format!("Sub-decision under {}", named(Some(under)))),
        Editing::AddDecision { task: Some(task), .. } => Some(format!("Decision raised under task {}", named(Some(task)))),
        Editing::AddDecision { .. } => Some("Decision on the feature".into()),
        _ => None,
    }
}

/// The column, filling the box its split gives it.
pub fn outline(this: &OpenFeature, view: &FeatureView, open: Option<Editing>, feature: &Entity<OpenFeature>, tokens: &Tokens, cx: &gpui_kit::App) -> AnyElement {
    let (adding_task, adding_decision) = (feature.clone(), feature.clone());
    let mixed = marks::mixed(view);
    let form_entity = this.form.clone();
    let header = h_flex().id("outline-label").px(rem(tokens.sp_8)).pb(rem(tokens.sp_6)).child(label("Outline", tokens)).child(legend::ask(this.legend, feature, tokens));
    let adding = open.and_then(|editing| adding_words(view, editing).map(|words| (editing, words)));
    let column = v_flex()
        .id("work-outline")
        .test_support()
        .aria_label("Outline")
        .relative()
        .size_full()
        .min_w_0()
        .py(rem(tokens.sp_16))
        .gap(rem(tokens.sp_2))
        .overflow_y_scroll()
        .track_scroll(&this.scroll.outline)
        .child(header)
        .when(!view.loaded, |column| column.child(meta(tokens).id("outline-note").test_support().aria_label("Reading its tasks\u{2026}").px(rem(tokens.sp_8)).child("Reading its tasks\u{2026}")))
        .when(view.loaded && view.rows.is_empty(), |column| column.child(meta(tokens).id("outline-note").test_support().aria_label("Nothing written under it yet: add a task, or raise a decision.").px(rem(tokens.sp_8)).child("Nothing written under it yet: add a task, or raise a decision.")))
        .children(view.rows.iter().enumerate().map(|(ix, work)| row::row(ix, work, mixed, feature, tokens)))
        .children(adding.map(|(editing, words)| {
            v_flex()
                .id("work-add")
                .test_support()
                .aria_label(SharedString::from(words.clone()))
                .flex_shrink_0()
                .p(rem(tokens.sp_8))
                .gap(rem(tokens.sp_4))
                .child(label(words, tokens))
                .child(form::line_field(&form_entity, "New row", tokens, cx))
                .child(form::buttons(&form_entity, editing, tokens))
        }))
        .child(
            h_flex()
                .id("work-foot")
                .flex_shrink_0()
                .mt_auto()
                .px(rem(tokens.sp_8))
                .pt(rem(tokens.sp_8))
                .gap(rem(tokens.sp_8))
                .border_t_1()
                .border_color(tokens.line)
                .child(
                    button("work-add-task", "+ Task", Look::Plain, false, tokens)
                        .child("+ Task")
                        .child(keycap("A", SharedString::from(tokens.font_mono), tokens))
                        .on_click(move |_, window, cx| adding_task.update(cx, |feature, cx| feature.begin(Editing::AddTask { parent: None }, window, cx))),
                )
                .child(
                    button("work-add-decision", "+ Decision", Look::Plain, false, tokens)
                        .child("+ Decision")
                        .child(keycap("D", SharedString::from(tokens.font_mono), tokens))
                        .on_click(move |_, window, cx| adding_decision.update(cx, |feature, cx| feature.begin(Editing::AddDecision { under: None, task: None }, window, cx))),
                ),
        )
        .when(this.legend, |column| column.child(legend::pop(mixed, tokens)));
    column.into_any_element()
}

/// The child of the column the cursor's row is, to scroll to: after the label, and the line that says
/// the tasks are still being read.
pub fn lit_child(view: &FeatureView) -> Option<usize> {
    let at = view.rows.iter().position(|row| row.on)?;
    Some(1 + usize::from(!view.loaded) + at)
}
