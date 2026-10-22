//! The values that point into Argus rather than out of it (a path, a commit, a feature, a pane), and
//! the plain ones drawn beside them (a duration, inline code).
//!
//! A pointer is a small button that opens its target in this window, never in a browser; Rust finds
//! the target from the pane that showed the view, so a press names the reference and nothing more.
//! Which paths are buttons is core's (`values::pointer`, which asks `is_checkout_relative`), so a
//! path Rust would refuse is text here and is not written a second time.

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::views::values::{duration, pointer, Pointer};
use argus_protocol::Value;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, ElementId, Role, SharedString};

use super::ctx::{Draw, ViewCtx};
use crate::kit::icon_button::Node;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// `.vicode`: inline monospace on the fill.
pub fn inline_code(text: impl Into<SharedString>, tokens: &Tokens) -> impl IntoElement {
    div()
        .px(rem(tokens.sp_4))
        .py(rem(tokens.hairline))
        .rounded(rem(tokens.r_xs))
        .bg(tokens.fill)
        .text_color(tokens.fg)
        .font_family(tokens.font_mono)
        .text_size(rem(tokens.fs_xs))
        .child(text.into())
}

/// `.vref`: the box of a button that opens something in Argus, in the reference colour on the fill.
pub fn ref_box(id: impl Into<ElementId>, words: impl Into<SharedString>, title: impl Into<SharedString>, tokens: &Tokens) -> Node {
    gpui_kit::component::h_flex()
        .id(id)
        .role(Role::Button)
        .test_support()
        .aria_label(words)
        .min_h(rem(tokens.sp_24))
        .px(rem(tokens.sp_8))
        .py(rem(tokens.sp_4))
        .items_center()
        .rounded(rem(tokens.r_xs))
        .bg(tokens.fill)
        .text_color(tokens.ref_)
        .font_family(tokens.font_mono)
        .text_size(rem(tokens.fs_xs))
        .weighted(tokens.weight_semibold)
        .whitespace_nowrap()
        .cursor_pointer()
        .hover(|style| style.underline())
        .tooltip(tip(title))
}

/// What a press of `pointer` in the view `view` asks for: the pane that showed the view, none for an
/// extension's table or a saved view, and the reference.
pub fn open_ref(pointer: &Pointer, view: Option<&ViewCtx>) -> Call {
    Call::OpenRef { pane: ViewCtx::pane_of(view), reference: pointer.reference.clone() }
}

/// A pointer drawn as its button.
pub fn pointer_button(id: impl Into<ElementId>, pointer: &Pointer, draw: &Draw) -> Node {
    let (caller, asked) = (draw.caller.clone(), open_ref(pointer, draw.view));
    ref_box(id, pointer.text.clone(), pointer.title, draw.tokens).child(SharedString::from(pointer.text.clone())).on_click(move |_, _, cx| {
        cx.stop_propagation();
        caller.call(asked.clone());
    })
}

/// A duration, a piece of code, a path, a commit, a feature or a pane, as a value draws it; `key`
/// makes the ids of what is pressable. Any other value is not this module's and draws as nothing.
pub fn ref_value(value: &Value, key: &str, draw: &Draw) -> AnyElement {
    let tokens = draw.tokens;
    match value {
        Value::Duration { ms } => div()
            .id(SharedString::from(format!("{key}-duration")))
            .test_support()
            .font_family(tokens.font_mono)
            .text_size(rem(tokens.fs_sm))
            .tooltip(tip(format!("{ms} ms")))
            .child(duration(*ms))
            .into_any_element(),
        Value::Code { text } => inline_code(text.clone(), tokens).into_any_element(),
        Value::Path { path, line } => match pointer(value) {
            Some(found) => pointer_button(SharedString::from(format!("{key}-ref")), &found, draw).into_any_element(),
            None => inline_code(line.map_or_else(|| path.clone(), |line| format!("{path}:{line}")), tokens).into_any_element(),
        },
        Value::Commit { title, .. } => {
            let found = pointer(value).expect("a commit is a pointer");
            gpui_kit::component::h_flex()
                .items_baseline()
                .gap(rem(tokens.sp_8))
                .min_w_0()
                .child(pointer_button(SharedString::from(format!("{key}-ref")), &found, draw))
                .children(title.as_ref().filter(|title| !title.is_empty()).map(|title| div().text_color(tokens.fg_muted).child(SharedString::from(title.clone()))))
                .into_any_element()
        }
        Value::Feature { .. } | Value::Pane { .. } => {
            let found = pointer(value).expect("a feature and a pane are pointers");
            pointer_button(SharedString::from(format!("{key}-ref")), &found, draw).into_any_element()
        }
        _ => div().into_any_element(),
    }
}
