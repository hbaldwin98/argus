//! A box to tick that can also read mixed: the select-all of a table, ticked when every row is,
//! dashed when only some are. The kit's checkbox has no third state, so this draws the page's own.
//!
//! It is controlled like the rest of `kit/choice`: it draws the mark it is given, and the press is
//! the caller's to hear and to answer by drawing what Rust says next.

use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, ElementId, Role, SharedString};

use super::icon_button::Node;
use super::icons::Glyph;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// What the box shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Clear,
    Mixed,
    Checked,
}

/// The box. `words` names it for a reader that cannot see it.
pub fn mixed_box(id: impl Into<ElementId>, words: impl Into<SharedString>, mark: Mark, tokens: &Tokens) -> Node {
    let on = mark != Mark::Clear;
    div()
        .id(id)
        .role(Role::CheckBox)
        .test_support()
        .aria_label(words)
        .flex_shrink_0()
        .size(rem(tokens.check_box))
        .flex()
        .items_center()
        .justify_center()
        .rounded(rem(tokens.r_xs))
        .border(rem(tokens.check_edge))
        .border_color(if on { tokens.accent } else { tokens.line_strong })
        .cursor_pointer()
        .when(on, |tick| tick.bg(tokens.accent).text_color(tokens.fg_on_accent))
        .when(mark == Mark::Checked, |tick| tick.child(Glyph::Tick.at(tokens.icon_sm)))
        .when(mark == Mark::Mixed, |tick| tick.child(div().w(rem(tokens.check_box * 0.5)).h(rem(tokens.check_edge)).bg(tokens.fg_on_accent)))
}
