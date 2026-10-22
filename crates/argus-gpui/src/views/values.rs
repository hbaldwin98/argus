//! One `Value` as a field, a table cell or a list item shows it. Every string is an agent's, read
//! from a ticket, so it goes in as text and nothing else.
//!
//! What a value says (its words, its tint, which references are buttons) is `argus-desktop-core`'s
//! `views::values`; this draws it. `key` names the value on screen, for the ids of what in it can
//! be pressed, so two values never share one.

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::views::values::{ago, grouped, initials, local_time, people, short_date, tint};
use argus_protocol::Value;
use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, SharedString};

use super::blocks::progress::progress_value;
use super::chip::{chip, tone_colour};
use super::ctx::Draw;
use super::link::view_link;
use super::refs::ref_box;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// `.vav`: the initials in a circle on a wash of the name's tint.
fn avatar(name: &str, tokens: &Tokens) -> impl IntoElement {
    let colour = tone_colour(tint(name), tokens);
    div()
        .id(SharedString::from(format!("avatar-{name}")))
        .test_support()
        .size(rem(tokens.icon_box))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(tokens.fill.blend(colour.opacity(tokens.share_avatar)))
        .text_color(tokens.fg)
        .text_size(rem(tokens.fs_2xs))
        .weighted(tokens.weight_bold)
        .tooltip(tip(name.to_owned()))
        .child(initials(name))
}

/// A person: initials in a circle, and the name.
pub fn person(name: &str, tokens: &Tokens) -> impl IntoElement {
    h_flex().items_center().gap(rem(tokens.sp_6)).child(avatar(name, tokens)).child(SharedString::from(name.to_owned()))
}

/// The first people overlapping in a row of avatars, their names joined, and a muted count of the
/// rest.
fn crowd(names: &[String], tokens: &Tokens) -> impl IntoElement {
    let (shown, rest) = people(names);
    h_flex()
        .items_center()
        .gap(rem(tokens.sp_6))
        .child(h_flex().items_center().children(shown.iter().enumerate().map(|(k, name)| {
            // Each avatar after the first sits over the one before it, ringed in the surface colour.
            div().when(k > 0, |avatar| avatar.ml(rem(-tokens.sp_6))).rounded_full().border_2().border_color(tokens.bg_surface).child(avatar(name, tokens))
        })))
        .child(SharedString::from(shown.join(", ")))
        .when(rest > 0, |row| row.child(div().text_color(tokens.fg_muted).child(format!("+{rest}"))))
}

/// `.vkey`: a ticket's key in monospace on the fill.
fn key_box(key: &str, linked: bool, tokens: &Tokens) -> impl IntoElement {
    div()
        .px(rem(tokens.sp_6))
        .py(rem(tokens.hairline))
        .rounded(rem(tokens.r_xs))
        .bg(tokens.fill)
        .text_color(if linked { tokens.ref_ } else { tokens.fg })
        .font_family(tokens.font_mono)
        .text_size(rem(tokens.fs_xs))
        .weighted(tokens.weight_semibold)
        .whitespace_nowrap()
        .child(SharedString::from(key.to_owned()))
}

/// What a value is when it is a saved view: its label, else the title it is held under, else its
/// id. One this window does not hold is drawn faded and struck through and is still pressed, since
/// the view may be saved later.
fn saved_link(id: &str, label: Option<&str>, key: &str, draw: &Draw) -> AnyElement {
    let held = draw.saved_title(id);
    let words = label.or(held).unwrap_or(id).to_owned();
    let title = held.map_or_else(|| format!("No saved view {id} yet"), |title| format!("Open the saved view {title}"));
    let (caller, asked) = (draw.caller.clone(), Call::OpenSavedView { id: id.to_owned(), from: draw.view.and_then(|view| view.saved.clone()) });
    ref_box(SharedString::from(format!("{key}-saved")), words.clone(), title, draw.tokens)
        .child(format!("{words} \u{2197}"))
        .when(held.is_none(), |link| link.opacity(draw.tokens.opacity_dim).line_through())
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            caller.call(asked.clone());
        })
        .into_any_element()
}

/// One value as a field, a cell or a list item shows it. `draw.view` is the agent view it is in; an
/// extension's table has none.
pub fn value(v: &Value, key: &str, draw: &Draw) -> AnyElement {
    let tokens = draw.tokens;
    match v {
        Value::Text { text } => div().child(SharedString::from(text.clone())).into_any_element(),
        Value::Number { number, unit } => h_flex()
            .font_family(tokens.font_mono)
            .text_size(rem(tokens.fs_sm))
            .child(grouped(*number))
            .children(unit.as_ref().filter(|unit| !unit.is_empty()).map(|unit| div().text_color(tokens.fg_muted).child(format!(" {unit}"))))
            .into_any_element(),
        Value::Bool { value } => {
            div().whitespace_nowrap().when(!*value, |div| div.text_color(tokens.fg_muted)).child(if *value { "\u{2713} Yes" } else { "\u{2717} No" }).into_any_element()
        }
        Value::Date { date } => div().child(short_date(draw.clock, date)).into_any_element(),
        Value::DateTime { at } => {
            let said = div().id(SharedString::from(format!("{key}-time"))).test_support().child(ago(draw.clock, at));
            match local_time(draw.clock, at) {
                Some(exact) => said.tooltip(tip(exact)).into_any_element(),
                None => said.into_any_element(),
            }
        }
        Value::Person { name } => person(name, tokens).into_any_element(),
        Value::People { names } => crowd(names, tokens).into_any_element(),
        Value::Chip { chip: c } => chip(c, tokens).into_any_element(),
        Value::Chips { chips } => h_flex().flex_wrap().gap(rem(tokens.sp_4)).children(chips.iter().map(|c| chip(c, tokens))).into_any_element(),
        Value::Link { label, url } => {
            view_link(SharedString::from(format!("{key}-link")), label.clone(), url, draw).child(SharedString::from(label.clone())).into_any_element()
        }
        Value::Ticket { key: ticket, title, url, status } => h_flex()
            .items_baseline()
            .gap(rem(tokens.sp_8))
            .min_w_0()
            .child(match url {
                Some(url) => view_link(SharedString::from(format!("{key}-link")), ticket.clone(), url, draw).child(key_box(ticket, true, tokens)).into_any_element(),
                None => key_box(ticket, false, tokens).into_any_element(),
            })
            .children(title.as_ref().filter(|title| !title.is_empty()).map(|title| div().text_color(tokens.fg_muted).child(SharedString::from(title.clone()))))
            .children(status.as_ref().map(|status| chip(status, tokens)))
            .into_any_element(),
        Value::Progress { value, max, tone } => progress_value(SharedString::from(format!("{key}-progress")), *value, *max, *tone, tokens).into_any_element(),
        Value::Duration { .. } | Value::Code { .. } | Value::Path { .. } | Value::Commit { .. } | Value::Feature { .. } | Value::Pane { .. } => {
            super::refs::ref_value(v, key, draw)
        }
        Value::View { id, label } => saved_link(id, label.as_deref(), key, draw),
        Value::Empty => div().text_color(tokens.fg_faint).child("\u{2014}").into_any_element(),
        Value::Unknown => {
            div().id(SharedString::from(format!("{key}-unknown"))).test_support().text_color(tokens.fg_muted).tooltip(tip("needs a newer Argus")).child("\u{2014}").into_any_element()
        }
    }
}

#[cfg(test)]
mod tests;
