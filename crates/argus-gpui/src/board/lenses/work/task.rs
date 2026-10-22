//! The task under the cursor, in full (BOARD-WORK-15, -16, -26): what it is, what done means for it
//! with each criterion a checkbox, the decisions it is built on, the notes agents and people left, and
//! what can be done to it; or its form while it is edited.
//!
//! Core builds the detail (`TaskDetail`); a criterion's checkbox sends the edit that checks it and
//! draws what the pushed task says next, as every control does.

use argus_desktop_core::connection::page::{Call, FeatureEdit, FeatureView, Line, TaskDetail};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, Entity, SharedString, TestSupportExt as _};

use crate::board::feature_view::act::Act;
use crate::board::feature_view::more::overflow;
use crate::board::feature_view::OpenFeature;
use crate::board::lenses::forms::edit::Editing;
use crate::board::lenses::forms::view as form;
use crate::board::lenses::forms::Form;
use crate::kit::button::{button, Look};
use crate::kit::caption::{label, meta};
use crate::kit::choice::check_row;
use crate::kit::icon_button::icon_button;
use crate::kit::icons::Glyph;
use crate::kit::keycap::keycap;
use crate::kit::menu::item::MenuItem;
use crate::kit::pill::status_pill;
use crate::kit::status_dot::status_dot;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// `.stack-4`: a label over what it names.
pub fn titled(words: impl Into<SharedString>, body: impl IntoElement, tokens: &Tokens) -> AnyElement {
    let words = words.into();
    v_flex().id(words.clone()).test_support().aria_label(words.clone()).gap(rem(tokens.sp_4)).child(label(words, tokens)).child(body).into_any_element()
}

/// Lines under a heading: a mark, the label in mono in the link colour, and the words.
pub fn line_list(lines: &[Line], mark: &'static str, mark_colour: gpui_kit::Hsla, tokens: &Tokens) -> AnyElement {
    v_flex()
        .children(lines.iter().map(|line| {
            h_flex()
                .child(div().text_color(mark_colour).child(SharedString::from(format!("{mark} "))))
                .child(div().font_family(SharedString::from(tokens.font_mono)).text_color(tokens.ref_).child(SharedString::from(line.label.clone())))
                .child(div().text_color(tokens.fg_muted).child(SharedString::from(format!(" {}", line.text))))
        }))
        .into_any_element()
}

/// The head of a task: its label, how it stands, what it waits behind and what it is under, then its title.
fn top(task: &TaskDetail, tokens: &Tokens) -> AnyElement {
    let state = match task.state {
        "todo" => status_pill("task-state", "to do", "idle", tokens).into_any_element(),
        "doing" => status_pill("task-state", "in progress", "working", tokens).into_any_element(),
        "done" => status_pill("task-state", "done", "done", tokens).into_any_element(),
        other => status_pill("task-state", other, "idle", tokens).into_any_element(),
    };
    v_flex()
        .child(
            h_flex()
                .items_center()
                .gap(rem(tokens.sp_8))
                .min_h(rem(tokens.sp_20))
                .child(div().font_family(SharedString::from(tokens.font_mono)).text_color(tokens.accent).child(SharedString::from(format!("#{}", task.label))))
                .child(state)
                .children(task.waits_on.clone().map(|waits| {
                    meta(tokens).flex().items_center().gap(rem(tokens.sp_4)).whitespace_nowrap().child(Glyph::Lock.at(tokens.icon_xs)).child(SharedString::from(format!("waits on {waits}")))
                }))
                .child(div().flex_1())
                .children(task.parent.clone().map(|parent| meta(tokens).child(SharedString::from(format!("under {parent}"))))),
        )
        .child(div().mt(rem(tokens.sp_6)).typed(tokens.type_title).text_color(tokens.fg).child(SharedString::from(task.title.clone())))
        .into_any_element()
}

/// One criterion: its checkbox and words, and the × that takes it away.
fn criterion(index: usize, task: &TaskDetail, feature: &Entity<OpenFeature>, tokens: &Tokens) -> AnyElement {
    let item = &task.criteria[index];
    let (checking, removing, id) = (feature.clone(), feature.clone(), task.id);
    let text = SharedString::from(item.text.clone());
    h_flex()
        .id(("criterion", index))
        .group("criterion")
        .items_start()
        .gap(rem(tokens.sp_8))
        .child(
            div().flex_1().min_w_0().text_size(rem(tokens.fs_md)).child(check_row(("criterion-check", index), text.clone(), item.done, false, tokens, move |done, _, cx| {
                checking.update(cx, |feature, cx| feature.send(Call::EditFeature { edit: FeatureEdit::SetCriterion { id, index, done } }, cx))
            })),
        )
        .child(
            icon_button(("criterion-remove", index), format!("Remove \u{201c}{}\u{201d}", item.text), false, tokens)
                .invisible()
                .group_hover("criterion", |button| button.visible())
                .tooltip(tip("Remove"))
                .child("\u{d7}")
                .on_click(move |_, _, cx| removing.update(cx, |feature, cx| feature.send(Call::EditFeature { edit: FeatureEdit::RemoveCriterion { id, index } }, cx))),
        )
        .into_any_element()
}

/// The task's form (BOARD-WORK-16): its title in a line and its description in an area.
fn editing(task: &TaskDetail, form_entity: &Entity<Form>, tokens: &Tokens, cx: &gpui_kit::App) -> AnyElement {
    v_flex()
        .id("task-form")
        .test_support()
        .aria_label(SharedString::from(format!("Task {}", task.label)))
        .flex_1()
        .gap(rem(tokens.sp_16))
        .child(label(format!("Editing task {}", task.label), tokens).text_color(tokens.accent))
        .child(titled("Title", form::line_field(form_entity, "The task, in a line", tokens, cx), tokens))
        .child(titled("Description", form::area_field(form_entity, "What it is, and anything the one doing it should know", false, tokens, cx), tokens))
        .child(form::buttons(form_entity, Editing::Task { id: task.id }, tokens))
        .into_any_element()
}

/// The task read: description, criteria, what binds it, its notes, and its foot of actions.
pub fn detail(view: &FeatureView, task: &TaskDetail, feature: &Entity<OpenFeature>, form_entity: &Entity<Form>, open: Option<Editing>, tokens: &Tokens, cx: &gpui_kit::App) -> AnyElement {
    if open == Some(Editing::Task { id: task.id }) {
        return editing(task, form_entity, tokens, cx);
    }
    let adding = open == Some(Editing::Criterion { id: task.id });
    let (describing, writing, subtask, deciding, editing_row, going) = (feature.clone(), feature.clone(), feature.clone(), feature.clone(), feature.clone(), feature.clone());
    let _ = (&subtask, &deciding);
    let description = if task.body.is_empty() {
        div()
            .id("task-add-description")
            .test_support()
            .role(gpui_kit::Role::Button)
            .aria_label("Add a description")
            .cursor_pointer()
            .text_color(tokens.ref_)
            .child("Add a description")
            .on_click(move |_, window, cx| describing.update(cx, |feature, cx| feature.edit_cursor_row(window, cx)))
            .into_any_element()
    } else {
        div().text_color(tokens.fg_muted).max_w(rem(tokens.prose_max_w)).child(SharedString::from(task.body.clone())).into_any_element()
    };
    let criteria = v_flex()
        .gap(rem(tokens.sp_4))
        .children((0..task.criteria.len()).map(|index| criterion(index, task, feature, tokens)))
        .when(task.criteria.is_empty() && !adding, |list| list.child(div().text_color(tokens.fg_faint).child("Nothing says when it is done yet.")))
        .child(if adding {
            v_flex()
                .gap(rem(tokens.sp_4))
                .child(form::line_field(form_entity, "One checkable line", tokens, cx))
                .child(form::buttons(form_entity, Editing::Criterion { id: task.id }, tokens))
                .into_any_element()
        } else {
            let id = task.id;
            button("criterion-add", "+ Add criterion", Look::Quiet, true, tokens)
                .child("+ Add criterion")
                .on_click(move |_, window, cx| writing.update(cx, |feature, cx| feature.begin(Editing::Criterion { id }, window, cx)))
                .into_any_element()
        });
    let agent = view.agent.as_ref().map(|agent| {
        let pane = agent.pane;
        button("task-agent", agent.title.clone(), Look::Plain, false, tokens)
            .tooltip(tip("Go to its pane (G)"))
            .child(status_dot(format!("task-agent-{pane}"), agent.status, tokens))
            .child(SharedString::from(agent.title.clone()))
            .on_click(move |_, _, cx| going.update(cx, |feature, cx| feature.send(Call::GoToPane { pane }, cx)))
    });
    let id = task.id;
    let foot = h_flex()
        .id("task-foot")
        .items_center()
        .gap(rem(tokens.sp_8))
        .child(div().flex_1())
        .child(
            button("task-edit", "Edit", Look::Plain, false, tokens)
                .child("Edit")
                .child(keycap("E", SharedString::from(tokens.font_mono), tokens))
                .on_click(move |_, window, cx| editing_row.update(cx, |feature, cx| feature.edit_cursor_row(window, cx))),
        )
        .children(agent)
        .child(overflow(
            "task-more",
            "More: subtask, decision",
            vec![MenuItem::choice("Add subtask", Act::Begin(Editing::AddTask { parent: Some(id) })), MenuItem::choice("Raise a decision", Act::Begin(Editing::AddDecision { under: None, task: Some(id) }))],
            feature,
            tokens,
            cx,
        ));
    v_flex()
        .id("task-detail")
        .test_support()
        .aria_label(SharedString::from(format!("Task {}", task.label)))
        .flex_1()
        .min_w_0()
        .h_full()
        .py(rem(tokens.sp_16))
        .gap(rem(tokens.sp_16))
        .overflow_y_scroll()
        .child(top(task, tokens))
        .child(titled("Description", description, tokens))
        .child(titled("Done when", criteria, tokens))
        .when(!task.bound_by.is_empty(), |detail| detail.child(titled("Bound by", line_list(&task.bound_by, "\u{25c6}", tokens.ref_, tokens), tokens)))
        .when(!task.notes.is_empty(), |detail| {
            detail.child(titled(
                "Notes",
                v_flex().children(task.notes.iter().map(|note| v_flex().child(meta(tokens).child(SharedString::from(note.label.clone()))).child(div().text_color(tokens.fg_muted).child(SharedString::from(note.text.clone()))))),
                tokens,
            ))
        })
        .child(div().flex_1())
        .child(foot)
        .into_any_element()
}
