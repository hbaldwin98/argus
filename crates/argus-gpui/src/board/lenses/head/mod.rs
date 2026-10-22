//! An open feature's header (BOARD-HEAD-1 to -5): one quiet line under the window's title, that says how
//! it stands, how far it is, its tickets and its brief, and the tools on the title's own line, the
//! lenses it is read through and a menu for what is rarer.
//!
//! The brief takes what room the rest leaves and is the way into editing it, so the header has no Edit
//! button; while it is written, the line is the brief's form. The words of the tickets and the reconcile
//! are core's (`FeatureView::tickets`, `reconcile_entry`).

pub mod menu;
pub mod tools;

use argus_desktop_core::connection::page::{Call, FeatureView};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, Entity, SharedString, TestSupportExt as _};

use crate::board::feature_view::OpenFeature;
use crate::board::lenses::forms::edit::Editing;
use crate::board::lenses::forms::Form;
use crate::board::lenses::forms::view as form;
use crate::kit::button::{button, Look};
use crate::kit::pill::{standing_status, status_pill};
use crate::kit::progress::{bar, percent};
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// What the progress bar says to a pointer: `2 of 7 tasks · 1 decided · 1 open`, a part left out
/// when it is nothing.
pub fn tally(view: &FeatureView) -> String {
    let mut parts = vec![format!("{} of {} tasks", view.tasks_done, view.tasks_total)];
    if view.decided > 0 {
        parts.push(format!("{} decided", view.decided));
    }
    if view.open > 0 {
        parts.push(format!("{} open", view.open));
    }
    parts.join(" \u{b7} ")
}

/// The line under the title, or the brief's form in its place.
pub fn status_line(view: &FeatureView, feature: &Entity<OpenFeature>, form_entity: &Entity<Form>, form_open: Option<Editing>, tokens: &Tokens, cx: &gpui_kit::App) -> AnyElement {
    // `.fhead`: under a hairline, 8 and 20 inside.
    let frame = |inner: AnyElement| v_flex().id("feature-head").test_support().flex_shrink_0().border_b_1().border_color(tokens.line).px(rem(tokens.sp_20)).py(rem(tokens.sp_8)).child(inner).into_any_element();
    if form_open == Some(Editing::Brief) {
        return frame(
            v_flex()
                .id("brief-edit")
                .test_support()
                .aria_label("Edit the brief")
                .gap(rem(tokens.sp_6))
                .pb(rem(tokens.sp_12))
                .child(form::area_field(form_entity, "Brief", false, tokens, cx))
                .child(form::buttons(form_entity, Editing::Brief, tokens))
                .into_any_element(),
        );
    }
    let (opening, linking) = (feature.clone(), feature.clone());
    let brief = if view.brief.is_empty() { "No brief yet.".to_owned() } else { view.brief.clone() };
    let tip_words = if view.brief.is_empty() { "Write the brief (B)".to_owned() } else { format!("{}\n\nEdit the brief (B)", view.brief) };
    let progress = (view.tasks_total > 0).then(|| {
        let share = percent(view.tasks_done as u32, view.tasks_total as u32).unwrap_or(0);
        h_flex()
            .id("feature-progress")
            .gap(rem(tokens.sp_8))
            .items_center()
            .flex_shrink_0()
            .tooltip(tip(tally(view)))
            .child(bar("feature-bar", share, tally(view), tokens))
            .child(SharedString::from(format!("{} of {}", view.tasks_done, view.tasks_total)))
    });
    let tickets = view.tickets.clone().map(|label| {
        button("feature-tickets", label.clone(), Look::Plain, true, tokens)
            .child(SharedString::from(label))
            .tooltip(tip("The linked tickets (T)"))
            .on_click(move |_, _, cx| linking.update(cx, |feature, cx| feature.send(Call::OpenLink { map: false }, cx)))
    });
    let words = h_flex()
        .id("feature-brief")
        .test_support()
        .aria_label(SharedString::from(tip_words.clone()))
        .flex_1()
        .min_w_0()
        .gap(rem(tokens.sp_8))
        .cursor_pointer()
        .hover(|line| line.text_color(tokens.fg))
        .tooltip(tip(tip_words))
        .child(div().flex_shrink_0().text_color(tokens.fg).child("Brief"))
        .child(div().min_w_0().truncate().when(view.brief.is_empty(), |text| text.text_color(tokens.fg_faint)).child(SharedString::from(brief)))
        .on_click(move |_, window, cx| opening.update(cx, |feature, cx| feature.begin(Editing::Brief, window, cx)));
    frame(
        h_flex()
            .items_center()
            .gap(rem(tokens.sp_12))
            .min_w_0()
            .typed(tokens.type_small)
            .text_color(tokens.fg_muted)
            .child(status_pill("feature-standing", view.standing, standing_status(view.standing), tokens))
            .children(progress)
            .children(tickets)
            .child(words)
            .into_any_element(),
    )
}
