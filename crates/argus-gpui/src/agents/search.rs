//! The search field in the head of the sessions dialog and of the transcript reader: `.field.filter`,
//! a plain field `--filter-w` wide that gives way to `--filter-min-w`.
//!
//! What it says is the field's own once drawn; Rust filters and the rows follow its next page, so
//! the text is never written back while it has the keys (`kit/field`).

use gpui_kit::component::input::Input;
use gpui_kit::{div, prelude::*, SharedString, TestSupportExt as _};

use crate::kit::field::Field;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The field named `id`, read as `label`.
pub fn filter(id: &'static str, label: &'static str, field: &Field, tokens: &Tokens) -> impl IntoElement {
    div()
        .id(id)
        .test_support()
        .w(rem(tokens.filter_field_w))
        .min_w(rem(tokens.filter_field_min_w))
        .flex_shrink(1.0)
        .child(Input::new(field.input()).id(SharedString::from(format!("{id}-field"))).h(rem(tokens.control_h)).aria_label(label))
}
