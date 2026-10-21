//! A choice of a few drawn as pills in one rounded track: the Git stage's
//! Changes and History tabs, the side a review reads and its layout.
//!
//! Two controls, each said by what it changes, so they do not read as one
//! four-way choice. The pills are ours rather than `gpui-kit`'s `TabBar` or
//! `ButtonGroup` so a test can read which one is on (`aria_selected`) and the
//! press is a plain click that sends one call and changes nothing itself.

use gpui_kit::base::ObservedElement;
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, px, ElementId, FontWeight, Role, SharedString, Stateful, TestSupportExt as _};

use crate::theme::Ledger;

/// The track a group of pills sits in, named for a screen reader and a test.
pub fn track(id: &'static str, label: &'static str, role: Role, ledger: &Ledger) -> ObservedElement<Stateful<gpui_kit::Div>> {
    h_flex()
        .id(id)
        .role(role)
        .test_support()
        .aria_label(SharedString::from(label))
        .flex_shrink_0()
        .p(px(2.0))
        .gap_px()
        .items_center()
        .rounded(px(6.0))
        .border_1()
        .border_color(ledger.line)
        .bg(ledger.raised)
}

/// One pill, lit while it is the choice. The caller adds what it says and what a press does.
pub fn pill(id: impl Into<ElementId>, role: Role, on: bool, ledger: &Ledger) -> ObservedElement<Stateful<gpui_kit::Div>> {
    let (hover, foreground) = (ledger.foreground, ledger.foreground);
    h_flex()
        .id(id)
        .role(role)
        .test_support()
        .aria_selected(on)
        .h(px(22.0))
        .px_2()
        .gap_1()
        .items_center()
        .rounded(px(4.0))
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .when(on, |pill| pill.bg(ledger.line).text_color(foreground))
        .when(!on, |pill| pill.text_color(ledger.faint).hover(move |style| style.text_color(hover)))
}

/// A count that rides on a pill: quiet beside the pill's own words.
pub fn count(text: impl Into<SharedString>, ledger: &Ledger) -> impl IntoElement {
    div().text_color(ledger.faint).child(text.into())
}
