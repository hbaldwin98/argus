//! The box every selectable line of the rail is made of, and the mark that says a line is open or
//! shut.

use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{prelude::*, px, Hsla, Role};

use crate::kit::icons::Glyph;
use crate::theme::Tokens;

/// A selectable line of the rail: its box, with the colours and padding left
/// to the kind of row, since the page gives each its own.
pub(super) type Line = gpui_kit::base::ObservedElement<gpui_kit::Stateful<gpui_kit::Div>>;

pub(super) fn line(id: impl Into<gpui_kit::ElementId>, height: f32, tokens: &Tokens) -> Line {
    h_flex()
        .id(id)
        .role(Role::Button)
        .test_support()
        .h(px(height))
        .flex_shrink_0()
        .items_center()
        .rounded(px(tokens.r_sm))
        .cursor_pointer()
}

/// The turn of a chevron is `--t-fast`'s neighbour, 0.15s, in the page; here
/// the mark swaps, since the glyph set has one chevron per direction.
pub(super) fn chevron(id: String, open: bool, width: f32, colour: Hsla) -> Line {
    h_flex()
        .id(id)
        .test_support()
        .aria_expanded(open)
        .items_center()
        .justify_center()
        .w(px(width))
        .flex_shrink_0()
        .text_color(colour)
        .child(if open { Glyph::Down } else { Glyph::Right }.at(12.0))
}
