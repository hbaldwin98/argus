//! The strip's dictation chip, beside the Views launcher: whether dictation is ready and with what,
//! or the run's state while one is open (the Argus Dictation design's `.mic-chip`). A press opens
//! the Settings sheet at its Dictation section.

use argus_desktop_core::dictation::panel::{ChipPage, Tone};
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, Role, SharedString, TestSupportExt as _};

use crate::kit::icon_button::Node;
use crate::kit::tip::tip;
use crate::theme::scale::{pixels, rem};
use crate::theme::Tokens;

/// The chip for `page`. `hold` and `toggle` are the keys, for its tip; `rem_size` is the window's.
pub fn chip(page: &ChipPage, hold: &str, toggle: &str, tokens: &Tokens, rem_size: gpui_kit::Pixels) -> Node {
    let led = match (page.live, page.tone) {
        (true, _) => tokens.danger,
        (false, Tone::Warn) => tokens.attention,
        (false, Tone::Bad) => tokens.danger,
        _ => tokens.fg_fainter,
    };
    let words = SharedString::from(page.words.clone());
    let edge = if page.live { tokens.accent } else { tokens.line };
    h_flex()
        .id("dictation-chip")
        .role(Role::Button)
        .test_support()
        .aria_label(words.clone())
        .flex_none()
        .h(rem(tokens.dictate_chip_h))
        .px(rem(tokens.sp_8))
        .gap(rem(tokens.sp_6))
        .items_center()
        .rounded_full()
        .border_1()
        .border_color(edge)
        .text_size(rem(tokens.fs_xs))
        .text_color(if page.live { tokens.fg } else { tokens.fg_muted })
        .whitespace_nowrap()
        .cursor_pointer()
        .hover(move |style| style.bg(tokens.hover_fill))
        .tooltip(tip(format!("Hold {hold} to dictate, or {toggle}; press for the Dictation settings")))
        .child(
            div()
                .size(rem(tokens.dot_sm))
                .rounded_full()
                .bg(led)
                // The design's halo round a live light: a ring of its own colour, faint, as a focus halo is drawn.
                .when(page.live, |light| light.shadow(vec![gpui_kit::BoxShadow {
                    color: led.opacity(tokens.ring_halo_share),
                    offset: gpui_kit::point(gpui_kit::px(0.0), gpui_kit::px(0.0)),
                    blur_radius: gpui_kit::px(0.0),
                    spread_radius: pixels(tokens.ring_halo_width, rem_size),
                    inset: false,
                }])),
        )
        .child(words)
}
