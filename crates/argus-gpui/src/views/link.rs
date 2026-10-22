//! A link in a view an agent showed. The window never navigates and never opens a URL itself: a
//! press asks Rust to give it to the system's opener, which opens only an `http` or `https` one.
//! The URL is an agent's, read from a ticket, so it is data, shown nowhere but the tooltip.

use argus_desktop_core::connection::page::Call;
use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{prelude::*, ElementId, Role, SharedString};

use super::ctx::Draw;
use crate::kit::icon_button::Node;
use crate::kit::tip::tip;
use crate::theme::Tokens;

/// What a press of a link at `url` asks for.
pub fn open(url: &str) -> Call {
    Call::OpenUrl { url: url.to_owned() }
}

/// `.vlink`: in the reference colour, underlined under the pointer, its tooltip the address. `id`
/// names it for a test and `words` is what a screen reader says; what is drawn in it is the
/// caller's children.
pub fn view_link(id: impl Into<ElementId>, words: impl Into<SharedString>, url: &str, draw: &Draw) -> Node {
    let caller = draw.caller.clone();
    let asked = open(url);
    link_box(id, words, url, draw.tokens).on_click(move |_, _, cx| {
        cx.stop_propagation();
        caller.call(asked.clone());
    })
}

/// The box a link is drawn in, without what a press does.
pub fn link_box(id: impl Into<ElementId>, words: impl Into<SharedString>, url: &str, tokens: &Tokens) -> Node {
    h_flex()
        .id(id)
        .role(Role::Link)
        .test_support()
        .aria_label(words)
        .text_color(tokens.ref_)
        .cursor_pointer()
        .hover(|style| style.underline())
        .tooltip(tip(url.to_owned()))
}
