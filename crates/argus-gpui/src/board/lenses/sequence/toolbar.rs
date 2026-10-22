//! The expanded diagram's toolbar (BOARD-SEQ-8): where it is as crumbs, the flow it draws, what a call,
//! a reply and a call to itself look like, the zoom, Fit, the steps panel and the way out.
//!
//! What a control does is `Act`: a button and its key are one thing. The crumbs go back (the first also
//! leaves the feature), and collapsing puts away a step or source form being written without asking.

use argus_desktop_core::connection::page::{FeatureView, FlowDetail};
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, AnyElement, Entity, Role, SharedString, TestSupportExt as _};

use crate::board::feature_view::act::Act;
use crate::board::feature_view::{Expanded, OpenFeature};
use crate::kit::button::{button, Look};
use crate::kit::caption::meta;
use crate::kit::keycap::keycap;
use crate::kit::segment::{pill, track};
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// `.crumb`: a button of the trail, a little taller than its words.
fn crumb(id: &'static str, words: impl Into<SharedString>, tokens: &Tokens) -> crate::kit::icon_button::Node {
    let words = words.into();
    div()
        .id(id)
        .test_support()
        .role(Role::Button)
        .aria_label(words.clone())
        .h(rem(tokens.control_h_sm))
        .px(rem(tokens.sp_4))
        .flex()
        .items_center()
        .rounded(rem(tokens.r_sm))
        .cursor_pointer()
        .text_color(tokens.fg_muted)
        .hover(|crumb| crumb.bg(tokens.fill).text_color(tokens.fg))
        .child(words)
}

/// `.seq-legend`: a call a solid line, a reply a dashed one, a call to itself a loop.
fn legend(tokens: &Tokens) -> AnyElement {
    let mark = || div().w(rem(tokens.seq_mark_w)).ml(rem(tokens.sp_8)).border_t_2().border_color(tokens.fg_muted);
    meta(tokens)
        .id("seq-legend")
        .test_support()
        .aria_label("Legend")
        .flex()
        .items_center()
        .gap(rem(tokens.sp_6))
        .child(mark())
        .child("call")
        .child(mark().border_dashed())
        .child("reply")
        .child(div().ml(rem(tokens.sp_8)).w(rem(tokens.seq_self_w)).h(rem(tokens.seq_self_h)).border_2().border_l_0().border_color(tokens.fg_muted))
        .child("to itself")
        .into_any_element()
}

/// The zoom as a segment: out, the percent that gives 100% back, in.
fn zoom(held: Expanded, feature: &Entity<OpenFeature>, tokens: &Tokens) -> AnyElement {
    let (out, back, into) = (feature.clone(), feature.clone(), feature.clone());
    track("seq-zoom", "Zoom", Role::Group, tokens)
        .child(pill("seq-zoom-out", Role::Button, false, tokens).aria_label("Zoom out").child("\u{2212}").on_click(move |_, window, cx| out.update(cx, |feature, cx| feature.act(Act::Zoom(-argus_desktop_core::board::lenses::picture::ZOOM_BY), window, cx))))
        .child(
            pill("seq-zoom-pct", Role::Button, false, tokens)
                .aria_label(SharedString::from(format!("{}%", (held.zoom * 100.0).round())))
                .tooltip(tip("Back to 100%"))
                .child(SharedString::from(format!("{}%", (held.zoom * 100.0).round())))
                .on_click(move |_, window, cx| back.update(cx, |feature, cx| feature.act(Act::ZoomReset, window, cx))),
        )
        .child(pill("seq-zoom-in", Role::Button, false, tokens).aria_label("Zoom in").child("+").on_click(move |_, window, cx| into.update(cx, |feature, cx| feature.act(Act::Zoom(argus_desktop_core::board::lenses::picture::ZOOM_BY), window, cx))))
        .into_any_element()
}

/// The toolbar of the expanded diagram of `flow` in feature `view`.
pub fn toolbar(view: &FeatureView, flow: &FlowDetail, held: Expanded, feature: &Entity<OpenFeature>, tokens: &Tokens) -> AnyElement {
    let (leaving, collapsing, flows, switching, fitting, panelling, closing) = (feature.clone(), feature.clone(), feature.clone(), feature.clone(), feature.clone(), feature.clone(), feature.clone());
    let sep = || div().text_color(tokens.fg_faint).child("\u{203a}");
    let on = |button: crate::kit::icon_button::Node, lit: bool| if lit { button.border_color(tokens.accent).text_color(tokens.fg) } else { button };
    h_flex()
        .id("seq-bar")
        .test_support()
        .flex_shrink_0()
        .flex_wrap()
        .items_center()
        .gap(rem(tokens.sp_12))
        .px(rem(tokens.sp_16))
        .py(rem(tokens.sp_8))
        .border_b_1()
        .border_color(tokens.line)
        .child(
            h_flex()
                .items_center()
                .gap(rem(tokens.sp_8))
                .typed(tokens.type_small)
                .child(crumb("seq-crumb-features", "Features", tokens).on_click(move |_, window, cx| leaving.update(cx, |feature, cx| feature.close_all(window, cx))))
                .child(sep())
                .child(crumb("seq-crumb-feature", format!("#{}", view.number), tokens).on_click(move |_, window, cx| collapsing.update(cx, |feature, cx| feature.collapse(window, cx))))
                .child(sep())
                .child(crumb("seq-crumb-flows", "Flows", tokens).on_click(move |_, window, cx| flows.update(cx, |feature, cx| feature.collapse(window, cx))))
                .child(sep()),
        )
        .child(
            button("seq-flow", flow.title.clone(), Look::Plain, true, tokens)
                .aria_label("Flow")
                .child(SharedString::from(flow.title.clone()))
                .child("\u{25be}")
                .on_click(move |event, window, cx| {
                    let at = event.position();
                    switching.update(cx, |feature, cx| feature.open_flow_menu(at, window, cx))
                }),
        )
        .child(legend(tokens))
        .child(div().flex_1())
        .child(zoom(held, feature, tokens))
        .child(on(button("seq-fit", "Fit", Look::Plain, false, tokens).child("Fit").on_click(move |_, window, cx| fitting.update(cx, |feature, cx| feature.act(Act::Fit, window, cx))), held.fit))
        .child(on(
            button("seq-steps", if held.panel { "Hide steps" } else { "Show steps" }, Look::Plain, false, tokens)
                .child(if held.panel { "Hide steps" } else { "Show steps" })
                .on_click(move |_, window, cx| panelling.update(cx, |feature, cx| feature.act(Act::Panel, window, cx))),
            held.panel,
        ))
        .child(
            button("seq-collapse", "Collapse", Look::Plain, false, tokens)
                .child("Collapse ")
                .child(keycap("Esc", SharedString::from(tokens.font_mono), tokens))
                .on_click(move |_, window, cx| closing.update(cx, |feature, cx| feature.collapse(window, cx))),
        )
        .into_any_element()
}

