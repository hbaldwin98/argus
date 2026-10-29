//! What a row of the Settings sheet and a section of rows are made of: `.setrow`, `.setsection`,
//! and the groups a section's rows fall into under a small capital label.
//!
//! A row is the title over its detail at the left and the control at the right; a section is its
//! title and the line about it, then its groups. Rows are divided by a hairline and sit in no box of
//! their own, as the restyle's reference page sets its rows (the page's `.settings .setrow` is
//! padded inside one bordered group and not divided).

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, ElementId, Role, SharedString, TestSupportExt as _};

use crate::kit::caption;
use crate::style::Typed as _;
use crate::theme::scale::{rem, Type};
use crate::theme::Tokens;

/// Some of a section's rows under the small capital label that names them; a section of one kind of
/// row has one group and no label.
pub struct Group {
    pub label: Option<&'static str>,
    pub rows: Vec<AnyElement>,
}

impl Group {
    pub fn named(label: &'static str, rows: Vec<AnyElement>) -> Self {
        Self { label: Some(label), rows }
    }

    pub fn plain(rows: Vec<AnyElement>) -> Self {
        Self { label: None, rows }
    }
}

/// `.setrow`: `title` and its `detail` in a column that takes the room, then `control`. `notes` are
/// the lines that appear under the detail only while they have something to say (a font that is
/// not installed, a backdrop that cannot be drawn).
pub fn row(title: &str, detail: Option<&str>, notes: Vec<String>, control: impl IntoElement, tokens: &Tokens) -> AnyElement {
    h_flex()
        .items_center()
        .gap(rem(tokens.sp_16))
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

/// A group: its label, then its rows with a hairline between each and the next.
fn group(group: Group, tokens: &Tokens) -> impl IntoElement {
    v_flex()
        .children(group.label.map(|label| caption::label(label, tokens).pt(rem(tokens.sp_12)).pb(rem(tokens.sp_4))))
        .children(group.rows.into_iter().enumerate().map(|(at, row)| div().when(at > 0, |row| row.border_t_1().border_color(tokens.line)).child(row)))
}

/// `.setsection`: its name (600, `--fs-lg`) and the line about it, then its groups. `id` is what the
/// nav scrolls to.
pub fn section(id: impl Into<ElementId>, name: &str, about: &str, groups: Vec<Group>, tokens: &Tokens) -> AnyElement {
    v_flex()
        .id(id)
        .role(Role::Group)
        .test_support()
        .aria_label(SharedString::from(name.to_owned()))
        .flex_shrink_0()
        .gap(rem(tokens.sp_4))
        .child(
            v_flex()
                .gap(rem(tokens.sp_2))
                .child(div().typed(Type::new(tokens.weight_semibold, tokens.fs_lg, 1.3)).child(SharedString::from(name.to_owned())))
                .child(meta(about, tokens)),
        )
        .children(groups.into_iter().map(|each| group(each, tokens)))
        .into_any_element()
}
