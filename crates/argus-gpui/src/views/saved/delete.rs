//! Deleting a saved view: the column's `...` menu with its one item, and the question it opens in
//! place of the buttons (CHECKOUTS-CODE-VIEWS-SAVED-6, -7).
//!
//! Delete is for every window, and the agent that built the view would have to build it again, so
//! it asks first. The question is kept in `views/memory` under the column's key: the panel is drawn
//! again whenever anything in it changes, and a question being answered must not vanish. Yes sends
//! `DeleteSavedView`; the column goes when the daemon's list says the view has.

use argus_desktop_core::connection::page::Call;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu};
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, Anchor, App, Context, EntityId, Role, SharedString, TestSupportExt as _, Window};

use crate::connection::caller::Caller;
use crate::kit::button::{button, outlined, Look};
use crate::kit::icons::Glyph;
use crate::kit::menu::item::MenuItem;
use crate::kit::menu::popup;
use crate::kit::strip::menu_button;
use crate::theme::scale::rem;
use crate::theme::Tokens;
use crate::views::memory::Shared;

/// What the menu's one choice does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Does {
    Ask,
}

/// The menu: the view's title and Delete in the danger colour, saying who it is for.
pub fn menu(title: &str) -> Vec<MenuItem<Does>> {
    vec![MenuItem::heading(title), MenuItem::choice("Delete\u{2026}", Does::Ask).danger().noted("for every window")]
}

/// The key the question is kept under: the panel, the column and the view in it.
pub fn key(panel: &str, column: usize, id: &str) -> String {
    format!("saved-delete|{panel}|{column}|{id}")
}

/// What a press needs to reach the window's memory and draw the panel again.
#[derive(Clone)]
pub struct Reach {
    pub memory: Shared,
    pub repaint: EntityId,
}

impl Reach {
    fn ask(&self, key: &str, open: bool, cx: &mut App) {
        self.memory.borrow_mut().set_asking(key, open);
        cx.notify(self.repaint);
    }
}

fn build(menu_in: PopupMenu, items: &[MenuItem<Does>], ask: (String, Reach), held: bool, tokens: Tokens, window: &mut Window, cx: &mut Context<PopupMenu>) -> PopupMenu {
    let (key, reach) = ask;
    popup::fill(menu_in, items, tokens, move |_| held, move |_, _, cx| reach.ask(&key, true, cx), window, cx)
}

/// `More`, under which the menu opens, right-aligned with the button.
pub fn more_button(id: String, title: &str, key: String, reach: Reach, held: bool, tokens: &Tokens, cx: &App) -> impl IntoElement {
    let (items, tokens) = (menu(title), *tokens);
    div().id(SharedString::from(format!("{id}-wrap"))).role(Role::Button).test_support().aria_label("More").flex_shrink_0().child(
        menu_button(SharedString::from(id), Glyph::More, tokens.icon_sm, &tokens, cx)
            .tooltip("More")
            .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, window, cx| build(menu, &items, (key.clone(), reach.clone()), held, tokens, window, cx)),
    )
}

/// `Delete it? Yes No`, the Yes in the danger colour.
pub fn question(id: &str, view: &str, key: String, reach: Reach, caller: Caller, held: bool, tokens: &Tokens) -> impl IntoElement {
    let (yes_key, no_key) = (key.clone(), key);
    let (yes_reach, no_reach) = (reach.clone(), reach);
    let id_of = view.to_owned();
    let danger = tokens.danger;
    h_flex()
        .id(SharedString::from(format!("{id}-ask")))
        .role(Role::Group)
        .test_support()
        .aria_label("Delete this saved view?")
        .gap(rem(tokens.sp_6))
        .flex_wrap()
        .child(div().text_color(tokens.fg_muted).text_size(rem(tokens.fs_sm)).child("Delete it?"))
        .child(outlined(SharedString::from(format!("{id}-yes")), "Yes", danger, true, tokens).child("Yes").on_click(move |_, _, cx| {
            yes_reach.ask(&yes_key, false, cx);
            if !held {
                caller.call(Call::DeleteSavedView { id: id_of.clone() });
            }
        }))
        .child(button(SharedString::from(format!("{id}-no")), "No", Look::Plain, true, tokens).child("No").on_click(move |_, _, cx| no_reach.ask(&no_key, false, cx)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::menu::item::lines;

    #[test]
    fn the_menu_names_the_view_and_offers_one_item_in_danger() {
        // SAVED-6.
        assert_eq!(lines(&menu("Release 0.34")), ["# Release 0.34", "!Delete\u{2026}"]);
    }

    #[test]
    fn a_question_is_kept_per_column_and_view() {
        assert_ne!(key("saved:a", 0, "a"), key("saved:a", 1, "a"));
        assert_ne!(key("saved:a", 0, "a"), key("saved:b", 0, "a"));
    }
}
