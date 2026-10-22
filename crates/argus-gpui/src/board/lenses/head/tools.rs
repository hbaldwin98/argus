//! What the strip carries for an open feature (BOARD-HEAD-5, -7, -9): the lenses as a radio, each with
//! the count it has to show, and the feature menu with a dot while a reconcile's proposal waits.
//!
//! They sit on the title's own line, in place of the index's layout switch. The lenses switch what the
//! feature is shown as, so they are segments and not tabs; choosing one sends `Call::SetLens` and
//! puts any form being written away.

use argus_desktop_core::connection::page::{FeatureView, Lens};
use gpui_kit::component::h_flex;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu};
use gpui_kit::{div, prelude::*, AnyElement, Anchor, App, Context, Entity, MouseButton, Role, SharedString, TestSupportExt as _, Window};

use super::menu::{button_label, items};
use crate::board::feature_view::OpenFeature;
use crate::board::lenses::forms::edit::Editing;
use crate::kit::icons::Glyph;
use crate::kit::menu::popup;
use crate::kit::segment::{count, pill, track};
use crate::kit::strip::menu_button;
use crate::kit::tip::tip;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The lenses in the order the radio lists them, each with its key.
pub const LENSES: [(Lens, &str, &str); 3] = [(Lens::Work, "Work", "1"), (Lens::Record, "Record", "2"), (Lens::Flows, "Flows", "3")];

/// What a lens has to show, as the suffix on its segment: the tasks, the decisions made and open, the
/// flows once they have come.
pub fn lens_count(view: &FeatureView, lens: Lens) -> Option<usize> {
    match lens {
        Lens::Work => Some(view.tasks_total),
        Lens::Record => Some(view.decided + view.open),
        Lens::Flows => view.flows.as_ref().filter(|flows| flows.loaded).map(|flows| flows.flows.len()),
    }
}

fn radio(view: &FeatureView, feature: &Entity<OpenFeature>, tokens: &Tokens) -> impl IntoElement {
    track("feature-lens", "Lens", Role::RadioGroup, tokens).children(LENSES.iter().map(|&(lens, name, key)| {
        let choosing = feature.clone();
        pill(SharedString::from(format!("lens-{key}")), Role::RadioButton, view.lens == lens, tokens)
            .aria_label(name)
            .tooltip(tip(format!("{name} ({key})")))
            .child(name)
            .children(lens_count(view, lens).map(|n| count(format!(" {n}"), tokens)))
            .on_click(move |_, window, cx| choosing.update(cx, |feature, cx| feature.lens(lens, window, cx)))
    }))
}

fn build(menu: PopupMenu, feature: &Entity<OpenFeature>, tokens: Tokens, window: &mut Window, cx: &mut Context<PopupMenu>) -> PopupMenu {
    let list = {
        let heard = feature.read(cx).heard.read(cx);
        let Some(open) = heard.features.as_ref().and_then(|features| features.open.as_ref()) else { return menu };
        let saved = heard.saved.as_ref().map(|saved| argus_desktop_core::views::offered::for_feature(&saved.views, heard.rail.repository, open.number)).unwrap_or_default();
        items(open, &saved)
    };
    let (acting, held) = (feature.clone(), feature.read(cx).heard.clone());
    popup::fill(
        menu,
        &list,
        tokens,
        move |cx| held.read(cx).held(),
        move |does, window, cx| match does.call() {
            Some(call) => acting.update(cx, |feature, cx| feature.send(call, cx)),
            None => acting.update(cx, |feature, cx| feature.begin(Editing::Brief, window, cx)),
        },
        window,
        cx,
    )
}

/// The tools: the lens radio and the menu button. Nothing while no feature is open.
pub fn tools(feature: &Entity<OpenFeature>, cx: &App) -> AnyElement {
    let tokens = *Tokens::global(cx);
    let heard = feature.read(cx).heard.read(cx);
    let Some(view) = heard.features.as_ref().and_then(|features| features.open.as_ref()) else { return div().into_any_element() };
    let (label, waiting) = (button_label(view), view.reconcile_entry.as_ref().is_some_and(|entry| entry.attention));
    let building = feature.clone();
    h_flex()
        .id("feature-tools")
        .gap(rem(tokens.sp_8))
        .items_center()
        .child(radio(view, feature, &tokens))
        .child(
            div()
                .id("feature-menu")
                .role(Role::Button)
                .test_support()
                .aria_label(label)
                .relative()
                .flex_shrink_0()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    menu_button("feature-menu-button", Glyph::More, tokens.icon_sm, &tokens, cx)
                        .tooltip(label)
                        .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, window, cx| build(menu, &building, tokens, window, cx)),
                )
                .when(waiting, |button| {
                    button.child(div().id("feature-attention").test_support().aria_label("a proposal waits").absolute().top(rem(tokens.sp_4)).right(rem(tokens.sp_4)).size(rem(tokens.dot_sm)).rounded_full().bg(tokens.attention))
                }),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests;
