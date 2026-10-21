//! What a docked panel's body draws.
//!
//! The console is drawn by `panes/console`; no other panel is drawn in the GPUI
//! window yet (the views agents showed, an extension's view and a saved view are
//! each their own later surface), so the body is the panel's name and a quiet line saying so. It
//! is no stand-in for the panel: nothing in it can be mistaken for content.
//! A panel ported later takes its place here by `PanelKind`.

use gpui_kit::component::v_flex;
use gpui_kit::{div, prelude::*, IntoElement, SharedString};

use crate::theme::Ledger;

pub const NOT_YET: &str = "Not in the GPUI window yet.";

pub fn body(title: impl Into<SharedString>, ledger: &Ledger) -> impl IntoElement {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_1()
        .child(div().text_sm().text_color(ledger.muted).child(title.into()))
        .child(div().text_xs().text_color(ledger.fainter).child(NOT_YET))
}
