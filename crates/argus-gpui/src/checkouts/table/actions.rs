//! What is done to a row, in its trailing column: `Open` (`Check out` on a branch) and the `⋯`
//! menu, quiet until the row is hovered or selected. Everything else a row can do is the menu's or
//! the side column's.
//!
//! A click on the row's own button first picks the row if it is not the selected one and then acts
//! (`checkouts/ways`), and does not reach the row under it, which would only pick it again.

use argus_desktop_core::checkouts::table::CheckoutsRow;
use argus_desktop_core::checkouts::ways::Way;
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, Anchor, MouseButton, Role, SharedString, TestSupportExt as _};

use super::Ctx;
use crate::checkouts::{act, menus};
use crate::kit::button::{button, Look};
use crate::kit::icons::Glyph;
use crate::kit::strip::menu_button;
use crate::kit::tip::tip;
use crate::theme::Tokens;
use crate::theme::scale::rem;

/// The buttons of `row`'s trailing cell. `shown` is whether the row is hovered or selected: the
/// buttons are laid out either way, so the column keeps its width.
pub fn cell(row: &CheckoutsRow, group: SharedString, shown: bool, ctx: &Ctx, tokens: &Tokens, cx: &gpui_kit::App) -> impl IntoElement {
    let name = row.name.clone().unwrap_or_else(|| row.branch.clone());
    let (words, way) = if row.checkout.is_some() { ("Open", Way::Open) } else { ("Check out", Way::NewWorktree) };
    let key = row.key.clone();
    let (caller, heard) = (ctx.caller.clone(), ctx.heard.clone());
    let (menu_caller, menu_heard, menu_key, tokens_copy) = (ctx.caller.clone(), ctx.heard.clone(), row.key.clone(), *tokens);
    h_flex()
        .id(SharedString::from(format!("row-actions-{}", row.key)))
        .test_support()
        .flex_shrink_0()
        .gap(rem(tokens.sp_4))
        .items_center()
        .justify_end()
        .when(!shown, |actions| actions.invisible().group_hover(group, |style| style.visible()))
        .child(
            button(SharedString::from(format!("row-open-{}", row.key)), words, Look::Plain, true, tokens)
                .child(words)
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    act::way(&way, &key, &caller, &heard, window, cx);
                }),
        )
        .child(
            // The kit's button has no accessible label of its own, so the wrapper carries it; a
            // press on it is not a press on the row.
            div()
                .id(SharedString::from(format!("row-more-{}", row.key)))
                .role(Role::Button)
                .test_support()
                .aria_label(SharedString::from(format!("More actions for {name}")))
                .flex_shrink_0()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    menu_button(SharedString::from(format!("row-more-button-{}", row.key)), Glyph::More, tokens.icon_sm, tokens, cx)
                        .tooltip("More actions")
                        .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, window, cx| menus::row(menu, &menu_key, &menu_caller, &menu_heard, tokens_copy, window, cx)),
                ),
        )
}

/// The tooltip the page gives a whole row: what was last committed there.
pub fn tooltip(row: &CheckoutsRow) -> Option<impl Fn(&mut gpui_kit::Window, &mut gpui_kit::App) -> gpui_kit::AnyView + 'static> {
    row.last.as_ref().map(|last| tip(last.summary.clone()))
}
