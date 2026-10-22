//! A choice of a few drawn as pills in one rounded track: the Git stage's
//! Changes and History tabs, the side a review reads and its layout.
//!
//! Two controls, each said by what it changes, so they do not read as one
//! four-way choice. The pills are ours rather than `gpui-kit`'s `TabBar` or
//! `ButtonGroup` so a test can read which one is on (`aria_selected`) and the
//! press is a plain click that sends one call and changes nothing itself. The
//! look is the page's `.seg` and `.segb`.

use gpui_kit::base::ObservedElement;
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, px, transparent_black, ElementId, Role, SharedString, Stateful, TestSupportExt as _};

use crate::style::Typed as _;
use crate::theme::Tokens;

/// `.seg`: the track a group of pills sits in, named for a screen reader and
/// a test. A strong edge, 2px of padding and 2px between pills.
pub fn track(id: &'static str, label: &'static str, role: Role, tokens: &Tokens) -> ObservedElement<Stateful<gpui_kit::Div>> {
    h_flex()
        .id(id)
        .role(role)
        .test_support()
        .aria_label(SharedString::from(label))
        .flex_shrink_0()
        .p(px(tokens.sp_2))
        .gap(px(tokens.sp_2))
        .items_center()
        .rounded(px(tokens.r_sm))
        .border_1()
        .border_color(tokens.line_strong)
}

/// `.segb`: one pill, `--control-h-sm` tall, lit while it is the choice. The
/// caller adds what it says and what a press does.
///
/// The one on is raised with an edge rather than tinted, the edge going to
/// the accent under the pointer; another lifts to `--fill`, and any presses
/// to the selected colour. The page draws the edge as an
/// inset shadow; here it is a border every pill has, clear when off, so the
/// pills are one size.
pub fn pill(id: impl Into<ElementId>, role: Role, on: bool, tokens: &Tokens) -> ObservedElement<Stateful<gpui_kit::Div>> {
    let (fill, text, selected, ring) = (tokens.fill, tokens.fg, tokens.bg_selected, tokens.accent);
    h_flex()
        .id(id)
        .role(role)
        .test_support()
        .aria_selected(on)
        .h(px(tokens.control_h_sm))
        .px(px(tokens.sp_12))
        .gap(px(tokens.sp_4))
        .items_center()
        .rounded(px(tokens.r_xs))
        .border_1()
        .text_size(px(tokens.fs_sm))
        .weighted(tokens.weight_semibold)
        .whitespace_nowrap()
        .cursor_pointer()
        .active(move |style| style.bg(selected).text_color(text))
        .map(|pill| {
            if on {
                pill.bg(tokens.bg_raised)
                    .border_color(tokens.line_strong)
                    .text_color(text)
                    .hover(move |style| style.border_color(ring))
            } else {
                pill.border_color(transparent_black())
                    .text_color(tokens.fg_muted)
                    .hover(move |style| style.bg(fill).text_color(text))
            }
        })
}

/// A count that rides on a pill: quiet beside the pill's own words.
pub fn count(text: impl Into<SharedString>, tokens: &Tokens) -> impl IntoElement {
    div().text_color(tokens.fg_faint).child(text.into())
}
