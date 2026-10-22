//! The Details fold under a column's header: what an agent needs to build the view again, as it
//! was saved (CHECKOUTS-CODE-VIEWS-SAVED-8). What each row says is core's (`views::column`); this
//! draws the rows and keeps whether the fold is open in `views/memory`, so a redraw keeps it.

use argus_desktop_core::views::column::Details;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, Role, SharedString};

use crate::theme::scale::rem;
use crate::theme::Tokens;
use crate::views::ctx::Draw;
use crate::views::link::view_link;

/// The key the fold's state is kept under.
pub fn key(panel: &str, column: usize, id: &str) -> String {
    format!("saved-details|{panel}|{column}|{id}")
}

fn row(label: &'static str, value: impl IntoElement, tokens: &Tokens) -> impl IntoElement {
    h_flex()
        .id(SharedString::from(format!("saved-meta-{label}")))
        .test_support()
        .gap(rem(tokens.sp_8))
        .items_start()
        .child(div().w(rem(tokens.saved_label_w)).flex_shrink_0().text_color(tokens.fg_muted).child(label))
        .child(div().min_w_0().flex_1().child(value))
}

/// `.saved-meta`: Scope, Built, then Features, Recipe and Sources when there are any.
pub fn list(details: &Details, id: &str, draw: &Draw) -> AnyElement {
    let tokens = draw.tokens;
    let recipe = details.recipe.clone().map(|text| row("Recipe", div().child(SharedString::from(text)), tokens));
    let sources = (!details.sources.is_empty()).then(|| {
        let items: Vec<AnyElement> = details
            .sources
            .iter()
            .enumerate()
            .map(|(k, source)| {
                let text = SharedString::from(source.text.clone());
                let said = if source.link {
                    view_link(SharedString::from(format!("saved-source-{id}-{k}")), text.clone(), &source.text, draw).child(text).into_any_element()
                } else {
                    div().child(text).into_any_element()
                };
                h_flex().gap(rem(tokens.sp_4)).items_start().child(div().text_color(tokens.fg_faint).text_size(rem(tokens.fs_xs)).child(SharedString::from(format!("{} ", source.kind)))).child(said).into_any_element()
            })
            .collect();
        row("Sources", v_flex().children(items), tokens)
    });
    v_flex()
        .id(SharedString::from(format!("saved-meta-{id}")))
        .role(Role::Group)
        .test_support()
        .aria_label("What it was built from")
        .mt(rem(tokens.sp_6))
        .gap(rem(tokens.sp_4))
        .text_size(rem(tokens.fs_sm))
        .child(row("Scope", div().child(SharedString::from(details.scope.clone())), tokens))
        .child(row("Built", div().child(SharedString::from(details.built.clone())), tokens))
        .children(details.features.clone().map(|text| row("Features", div().child(SharedString::from(text)), tokens)))
        .children(recipe.map(|recipe| recipe.into_any_element()))
        .children(sources)
        .into_any_element()
}
