//! The one row every tree and list is drawn from (`project/Kit.dc.html`, "Row"): the Files tree,
//! the Git change tree, the outline and History's commit files.
//!
//! 26px: a 12px chevron slot, a 16px icon slot, the label (cut with an ellipsis) and, right-aligned,
//! an end in mono 11px. The slots are always there, so labels line up whether or not a row has a
//! chevron or an icon. Picked, the row is a filled tile on a 1px ring. Children hang under their
//! parent on a 1px guide line (`nest`).

use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, AnyElement, Div, ElementId, Role, SharedString, Stateful, TestSupportExt as _};

use super::icon_button::Node;
use super::icons::Glyph;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// What a row says.
pub struct Slots {
    /// `Some(open)` draws a chevron, `None` leaves the slot empty (a leaf).
    pub chevron: Option<bool>,
    pub icon: Option<AnyElement>,
    pub label: SharedString,
    pub end: Option<AnyElement>,
    /// The row being read, or the one the keys are at.
    pub on: bool,
    /// Drawn faint, as an ignored entry is.
    pub dim: bool,
}

impl Slots {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self { chevron: None, icon: None, label: label.into(), end: None, on: false, dim: false }
    }
}

/// The row. A press and a tooltip are the caller's, put on what this returns.
pub fn row(id: impl Into<ElementId>, slots: Slots, tokens: &Tokens) -> Node {
    let (fill, hover_on) = (tokens.fill, tokens.selected_hover);
    let on = slots.on;
    h_flex()
        .id(id)
        .role(Role::TreeItem)
        .test_support()
        .aria_label(slots.label.clone())
        .aria_selected(on)
        .h(rem(tokens.row_h_sm))
        .flex_shrink_0()
        .min_w_0()
        .mx(rem(tokens.sp_6))
        .pl(rem(tokens.sp_4))
        .pr(rem(tokens.sp_8))
        .gap(rem(7.0))
        .items_center()
        .rounded(rem(tokens.r_sm))
        // The ring is there on every row, clear until picked, so a row never changes size.
        .border_1()
        .border_color(if on { tokens.line_strong } else { gpui_kit::transparent_black() })
        .text_size(rem(tokens.fs_md))
        .text_color(if slots.dim { tokens.fg_faint } else if on { tokens.fg } else { tokens.fg_muted.blend(tokens.fg.opacity(0.6)) })
        .cursor_pointer()
        .map(|row| if on { row.bg(tokens.bg_selected).hover(move |style| style.bg(hover_on)) } else { row.hover(move |style| style.bg(fill)) })
        .child(
            div()
                .flex_none()
                .w(rem(tokens.icon_xs))
                .flex()
                .justify_center()
                .text_color(tokens.fg_faint)
                .children(slots.chevron.map(|open| if open { Glyph::Down } else { Glyph::Right }.at(tokens.icon_xs))),
        )
        .child(div().flex_none().w(rem(tokens.sp_16)).flex().justify_center().text_color(tokens.fg_muted).children(slots.icon))
        .child(div().flex_1().min_w_0().truncate().child(slots.label))
        .children(slots.end.map(|end| div().flex_none().pl(rem(tokens.sp_8)).child(end)))
}

/// A row's end in the mono face, `colour` its tone (`4`, `M`, `+3`).
pub fn end_text(text: impl Into<SharedString>, colour: gpui_kit::Hsla, tokens: &Tokens) -> Div {
    div().font_family(SharedString::from(tokens.font_mono)).text_size(rem(tokens.fs_xs)).text_color(colour).child(text.into())
}

/// The children of an open row, one level in on a 1px guide line.
pub fn nest(id: impl Into<ElementId>, children: Vec<AnyElement>, tokens: &Tokens) -> Stateful<Div> {
    div().id(id).flex_none().ml(rem(tokens.sp_16)).pl(rem(tokens.sp_2)).border_l_1().border_color(tokens.line).flex().flex_col().children(children)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::host;
    use gpui_kit::TestAppContext;

    fn tree(tokens: &Tokens) -> AnyElement {
        let end = |t: &'static str| Some(end_text(t, tokens.changed, tokens).into_any_element());
        div()
            .w(rem(260.0))
            .child(row("parent", Slots { chevron: Some(true), end: end("4"), ..Slots::new("chrome/rail") }, tokens))
            .child(nest(
                "kids",
                vec![
                    row("kid", Slots { on: true, end: end("M"), ..Slots::new("mod.rs") }, tokens).into_any_element(),
                    row("other", Slots::new("slice.rs"), tokens).into_any_element(),
                ],
                tokens,
            ))
            .into_any_element()
    }

    /// Kit.dc.html: every row is 26px, picked or not, so the pick moves nothing.
    #[gpui_kit::test]
    fn a_row_is_26px_picked_or_not(cx: &mut TestAppContext) {
        let handle = host::open(cx, 300.0, tree);
        for id in ["parent", "kid", "other"] {
            let (_, _, _, h) = host::bounds(cx, handle, id);
            assert_eq!(h, 26.0, "{id}");
        }
    }

    /// Kit.dc.html: children nest one level under the parent's guide line, labels aligned among siblings.
    #[gpui_kit::test]
    fn children_nest_under_the_parent_and_line_up_among_themselves(cx: &mut TestAppContext) {
        let handle = host::open(cx, 300.0, tree);
        let (parent_left, ..) = host::bounds(cx, handle, "parent");
        let (kid_left, ..) = host::bounds(cx, handle, "kid");
        let (other_left, ..) = host::bounds(cx, handle, "other");
        assert!(kid_left > parent_left + 12.0, "a child is indented");
        assert_eq!(kid_left, other_left);
    }
}
