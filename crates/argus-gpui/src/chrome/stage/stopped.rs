//! The card over the window while the daemon was stopped on purpose: `argusd was stopped`, and the
//! one way out, `Reconnect` (SHELL-STAGE-14).
//!
//! Someone ran `argus server stop`, so nothing reconnects until this is pressed, and Rust honours
//! the call only while stopped. The card is an overlay of the host (`kit/overlay`) that Escape and
//! a press on the scrim do not dismiss. It is up exactly while the link is stopped: it comes with
//! the status and goes with the next one, never on the press, because the window changes when the
//! daemon says it has.

use argus_desktop_core::connection::page::Call;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, App, SharedString, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Link;
use crate::kit::button::{button, Look};
use crate::kit::icons::Glyph;
use crate::kit::overlay::host::{Dismiss, Overlays, Spec};
use crate::kit::overlay::kind::Overlay;
use crate::style::Typed as _;
use crate::theme::scale::{pixels, rem};
use crate::theme::Tokens;

/// Whether the card is up for `link`: only a stopped daemon has it.
pub fn wanted(link: &Link) -> bool {
    matches!(link, Link::Stopped)
}

/// Puts the card up or takes it away to match `link`.
pub fn follow(link: &Link, caller: &Caller, window: &mut Window, cx: &mut App) {
    let host = Overlays::get(cx);
    if !wanted(link) {
        host.close(Overlay::Stopped, window, cx);
    } else if !host.is_open(Overlay::Stopped) {
        let caller = caller.clone();
        host.open(
            Spec::new(Overlay::Stopped).dismissed(Dismiss::Never),
            move |dialog: Dialog, window, cx, _| {
                let tokens = *Tokens::global(cx);
                let caller = caller.clone();
                dialog.w(pixels(tokens.dialog_w_sm, window.rem_size())).child(
                    v_flex()
                        .id("stopped")
                        .items_center()
                        .gap(rem(tokens.sp_12))
                        .pt(rem(tokens.sp_16))
                        .text_center()
                        .child(div().text_color(tokens.fg_muted).child(Glyph::Server.at(tokens.stopped_icon)))
                        .child(div().typed(tokens.type_title).child("argusd was stopped"))
                        .child(
                            h_flex()
                                .flex_wrap()
                                .justify_center()
                                .text_color(tokens.fg_muted)
                                .child("Someone ran ")
                                .child(div().text_color(tokens.fg).font_family(SharedString::from(tokens.font_mono)).child("argus server stop"))
                                .child(". Its panes ended with it; this is the last tree it sent."),
                        )
                        .child(
                            button("reconnect", "Reconnect", Look::Primary, false, &tokens)
                                .mt(rem(tokens.sp_8))
                                .child("Reconnect")
                                .on_click(move |_, _, _| caller.call(Call::Reconnect)),
                        ),
                )
            },
            window,
            cx,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_stopped_daemon_has_the_card_and_a_reconnecting_one_is_not_stopped() {
        assert!(wanted(&Link::Stopped));
        for link in [Link::Connecting, Link::Reconnecting, Link::Connected { host: None }] {
            assert!(!wanted(&link), "{link:?}");
        }
    }
}
