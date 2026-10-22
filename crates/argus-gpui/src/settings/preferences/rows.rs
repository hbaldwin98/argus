//! What a row of the Settings sheet and a section of rows are made of: `.setrow`, `.setsection`
//! and the one bordered group the rows sit in.
//!
//! A row is the title over its detail at the left and the control at the right; a section is its
//! title and a line under it, then its rows in one rounded group. The rows are not divided from
//! each other (the page's `.settings .setrow` has no rule), only padded.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, ElementId, Role, SharedString, TestSupportExt as _};

use crate::style::Typed as _;
use crate::theme::scale::{rem, Type};
use crate::theme::Tokens;

/// `.setrow`: `title` and its `detail` in a column that takes the room, then `control`. `notes` are
/// the lines that appear under the detail only while they have something to say (a font that is
/// not installed, a backdrop that cannot be drawn).
pub fn row(title: &str, detail: Option<&str>, notes: Vec<String>, control: impl IntoElement, tokens: &Tokens) -> AnyElement {
    h_flex()
        .items_center()
        .gap(rem(tokens.sp_16))
        .px(rem(tokens.sp_16))
        .py(rem(tokens.sp_12))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(rem(tokens.sp_2))
                .child(SharedString::from(title.to_owned()))
                .children(detail.map(|detail| meta(detail, tokens)))
                .children(notes.into_iter().map(|note| meta(&note, tokens))),
        )
        .child(control)
        .into_any_element()
}

/// `.meta`: the small type in the muted colour.
pub fn meta(words: &str, tokens: &Tokens) -> gpui_kit::Div {
    div().typed(tokens.type_small).text_color(tokens.fg_muted).child(SharedString::from(words.to_owned()))
}

/// `.setsection`: its name (600, `--fs-lg`) and the line about it, then `rows` in one group with
/// the line's border. `id` is what the nav scrolls to.
pub fn section(id: impl Into<ElementId>, name: &str, about: &str, rows: Vec<AnyElement>, tokens: &Tokens) -> AnyElement {
    v_flex()
        .id(id)
        .role(Role::Group)
        .test_support()
        .aria_label(SharedString::from(name.to_owned()))
        .flex_shrink_0()
        .gap(rem(tokens.sp_8))
        .child(
            v_flex()
                .gap(rem(tokens.sp_2))
                .child(div().typed(Type::new(tokens.weight_semibold, tokens.fs_lg, 1.3)).child(SharedString::from(name.to_owned())))
                .child(meta(about, tokens)),
        )
        .child(v_flex().border_1().border_color(tokens.line).rounded(rem(tokens.r_md)).children(rows))
        .into_any_element()
}
