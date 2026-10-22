//! A cell's toolbar (SHELL-BROWSER-7): Back, Forward, Reload, the address, and Close; and the text
//! button every control of the cell is drawn with.
//!
//! The three moves are always enabled and show no state; Close closes the browser for every
//! client with no confirmation, and the cell goes when the daemon's tree says so.

use argus_desktop_core::connection::page::Call;
use argus_protocol::BrowserInput;
use gpui_kit::component::h_flex;
use gpui_kit::{prelude::*, App, SharedString, TestSupportExt as _, Window};

use super::address::Address;
use crate::connection::caller::Caller;
use crate::kit::button::{button, Look};
use crate::kit::icon_button::Node;
use crate::kit::tip::tip;
use crate::theme::scale::rem;
use crate::theme::Tokens;

pub const CLOSE_TIP: &str = "Close this browser; the agent’s next request opens another";

/// `.btn` in the strip or the bar: a plain pill with its words and a tooltip.
pub fn text_button(
    id: impl Into<String>,
    words: &'static str,
    title: &'static str,
    tokens: &Tokens,
    press: impl Fn(&mut Window, &mut App) + 'static,
) -> Node {
    button(SharedString::from(id.into()), words, Look::Plain, false, tokens)
        .tooltip(tip(title))
        .child(words)
        .on_click(move |_, window, cx| press(window, cx))
}

/// `.bbar`.
pub fn bar(pane: u64, address: &Address, caller: &Caller, tokens: &Tokens) -> impl IntoElement {
    let go = |name: &'static str, glyph: &'static str, input: BrowserInput| {
        let caller = caller.clone();
        text_button(format!("browser-{}-{pane}", name.to_lowercase()), glyph, name, tokens, move |_, _| {
            caller.call(Call::BrowserInput { pane, input: input.clone() })
        })
        .aria_label(name)
        .text_size(rem(tokens.fs_md))
    };
    let close = caller.clone();
    h_flex()
        .id(SharedString::from(format!("browser-bar-{pane}")))
        .test_support()
        .flex_none()
        .items_center()
        .gap(rem(tokens.sp_6))
        .px(rem(tokens.sp_12))
        .py(rem(tokens.sp_8))
        .border_b_1()
        .border_color(tokens.line)
        .bg(tokens.bg_surface)
        .child(go("Back", "←", BrowserInput::Back))
        .child(go("Forward", "→", BrowserInput::Forward))
        .child(go("Reload", "⟳", BrowserInput::Reload))
        .child(address.view(tokens))
        .child(text_button(format!("browser-close-{pane}"), "Close", CLOSE_TIP, tokens, move |_, _| close.call(Call::CloseBrowser { pane })))
}
