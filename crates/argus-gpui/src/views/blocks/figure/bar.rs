//! The larger view's bar: the figure's title, how to move it, the zoom group, Fit, Source and Close
//! (FIGURE-2).

use argus_desktop_core::views::blocks::zoom::percent;
use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, Context, Role, SharedString};

use super::larger::{Larger, Move};
use crate::kit::button::{button, Look};
use crate::kit::icon_button::icon_button;
use crate::kit::icons::Glyph;
use crate::kit::overlay::host::Overlays;
use crate::kit::overlay::kind::Overlay;
use crate::kit::segment::{pill, track};
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// `.vfig-bar`: the title takes the room, the hint and the controls keep theirs.
pub fn bar(larger: &Larger, tokens: &Tokens, cx: &mut Context<Larger>) -> AnyElement {
    let title = SharedString::from(larger.figure.title.clone());
    let said = larger.zoom().map(percent).unwrap_or_default();
    let step = |id: &'static str, label: &'static str, words: &'static str, by: Move, cx: &mut Context<Larger>| {
        pill(id, Role::Button, false, tokens).aria_label(label).child(words).on_click(cx.listener(move |this, _, window, cx| this.apply(by, window, cx)))
    };
    let zoom = track("figure-zoom", "Zoom", Role::Group, tokens)
        .child(step("figure-out", "Zoom out", "\u{2212}", Move::Out, cx))
        .child(
            pill("figure-pct", Role::Button, false, tokens)
                .aria_label("Actual size")
                .aria_value(SharedString::from(said.clone()))
                .min_w(rem(tokens.fig_pct_w))
                .justify_center()
                .tooltip(tip("Actual size"))
                .on_click(cx.listener(|this, _, window, cx| this.apply(Move::Actual, window, cx)))
                .child(SharedString::from(said)),
        )
        .child(step("figure-in", "Zoom in", "+", Move::In, cx));
    let source = larger.figure.source.is_some().then(|| {
        let on = larger.is_source_shown();
        button("figure-source", "Source", Look::Plain, true, tokens)
            .aria_toggled(if on { gpui_kit::Toggled::True } else { gpui_kit::Toggled::False })
            .when(on, |button| button.text_color(tokens.fg).border_color(tokens.accent))
            .child("Source")
            .on_click(cx.listener(|this, _, _, cx| this.toggle_source(cx)))
    });
    h_flex()
        .flex_shrink_0()
        .items_center()
        .gap(rem(tokens.sp_12))
        .pt(rem(tokens.sp_8))
        .pb(rem(tokens.sp_8))
        .pl(rem(tokens.sp_16))
        .pr(rem(tokens.sp_12))
        .border_b_1()
        .border_color(tokens.line)
        .child(
            div()
                .id("figure-title")
                .test_support()
                .flex_1()
                .min_w_0()
                .truncate()
                .typed(tokens.type_heading)
                .text_color(tokens.fg)
                .tooltip(tip(title.clone()))
                .child(title),
        )
        .child(div().flex_shrink_0().typed(tokens.type_small).text_color(tokens.fg_muted).child("Wheel to zoom \u{b7} drag to move"))
        .child(zoom)
        .child(button("figure-fit", "Fit", Look::Plain, true, tokens).child("Fit").on_click(cx.listener(|this, _, window, cx| this.apply(Move::Fit, window, cx))))
        .children(source)
        .child(
            icon_button("figure-close", "Close", false, tokens)
                .tooltip(tip("Close"))
                .on_click(|_, window, cx| {
                    Overlays::get(cx).close(Overlay::Figure, window, cx);
                })
                .child(Glyph::Close.at(tokens.icon_sm)),
        )
        .into_any_element()
}
