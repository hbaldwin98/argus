//! How a list row inside a dialog says it is the selected one: the selection fill and the theme's
//! mark, a bar down the leading edge or a hairline all round (`--mark-sel`, `.palette-row.on`).
//!
//! Every row has a 1px edge, clear when it is not selected, so a row does not change size as the
//! selection moves.

use gpui_kit::{div, prelude::*, Hsla};

use crate::theme::scale::{rem, Mark};
use crate::theme::Tokens;

pub fn selected<E>(row: E, on: bool, tokens: &Tokens) -> E
where
    E: Styled + ParentElement + IntoElement,
{
    let edge = if on && tokens.mark_sel == Mark::Hairline { tokens.mark_sel_edge } else { Hsla::transparent_black() };
    row.relative()
        .border_1()
        .border_color(edge)
        .when(on, |row| row.bg(tokens.bg_selected))
        .when(on && tokens.mark_sel == Mark::Bar, |row| row.child(div().absolute().left_0().top_0().bottom_0().w(rem(tokens.mark_bar_w)).bg(tokens.mark_sel_edge)))
}
