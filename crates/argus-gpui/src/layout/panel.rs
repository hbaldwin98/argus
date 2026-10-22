//! What a docked panel's body draws.
//!
//! The console is drawn by `panes/console`, a saved view by `views/saved`; no other panel is drawn in
//! the GPUI window yet (the views agents showed and an extension's view are each their own later
//! surface), so its body is the panel's name and a quiet line saying so. It is no stand-in for the
//! panel: nothing in it can be mistaken for content. A panel ported later takes its arm in [`content`].

use argus_desktop_core::layout::docks::Panel;
use gpui_kit::component::v_flex;
use gpui_kit::{div, prelude::*, AnyElement, App, Entity, IntoElement, SharedString};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::theme::Tokens;
use crate::views::saved::registry;

pub const NOT_YET: &str = "Not in the GPUI window yet.";

pub fn body(title: impl Into<SharedString>, tokens: &Tokens) -> impl IntoElement {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_1()
        .child(div().text_sm().text_color(tokens.fg_muted).child(title.into()))
        .child(div().text_xs().text_color(tokens.fg_fainter).child(NOT_YET))
}

/// What `panel` draws in a dock or a float: the saved view's own element, else [`body`]. The console
/// is drawn by its dock; each panel ported later takes its arm here.
pub fn content(panel: &Panel, title: impl Into<SharedString>, heard: &Entity<Heard>, caller: &Caller, tokens: &Tokens, cx: &mut App) -> AnyElement {
    match registry::element(panel, heard, caller, cx) {
        Some(saved) => saved.into_any_element(),
        None => body(title, tokens).into_any_element(),
    }
}
