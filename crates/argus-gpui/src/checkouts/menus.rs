//! The two menus of the Checkouts stage drawn from `argus-desktop-core`'s lists: the strip's `⋯`
//! (the branches, fetch all) and a row's `⋯`, which lists by the kind of row (`ways`).
//!
//! Core decides what each menu offers and what a choice comes to; this is the kit's popup menu over
//! that list, with the key that does the same beside a choice as the hint. A choice is acted on
//! when the menu has gone, against the table as it is then (`act`), and a held window offers none.

use argus_desktop_core::checkouts::ways::{self, Item, Way};
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::{App, Context, Entity, Window};

use super::act;
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::menu::item::MenuItem;
use crate::kit::menu::popup;
use crate::theme::Tokens;

/// The kit's items for core's: a rule, or a choice with its key as the hint and in danger when it
/// ends something.
pub fn items(list: &[Item]) -> Vec<MenuItem<Way>> {
    list.iter()
        .map(|item| match item {
            Item::Rule => MenuItem::Rule,
            Item::Choice(choice) => {
                let item = MenuItem::choice(choice.label.clone(), choice.way.clone());
                let item = match choice.key {
                    Some(key) => item.keyed(vec![vec![key.to_owned()]]),
                    None => item,
                };
                if choice.danger {
                    item.danger()
                } else {
                    item
                }
            }
        })
        .collect()
}

/// The strip's menu, built as it opens: what it says depends on whether the branches are shown.
pub fn strip(menu: PopupMenu, caller: &Caller, heard: &Entity<Heard>, tokens: Tokens, window: &mut Window, cx: &mut Context<PopupMenu>) -> PopupMenu {
    let list = heard.read(cx).checkouts.as_ref().map(|table| ways::strip_items(table)).unwrap_or_default();
    let (caller, held, acting) = (caller.clone(), heard.clone(), heard.clone());
    popup::fill(menu, &items(&list), tokens, move |cx| held.read(cx).held(), move |way, window, cx| act::table(way, &caller, &acting, window, cx), window, cx)
}

/// The menu of the row `key`, built as it opens.
pub fn row(menu: PopupMenu, key: &str, caller: &Caller, heard: &Entity<Heard>, tokens: Tokens, window: &mut Window, cx: &mut Context<PopupMenu>) -> PopupMenu {
    let list = {
        let heard = heard.read(cx);
        match heard.checkouts.as_ref().and_then(|table| table.rows.iter().find(|row| row.key == key).map(|row| ways::row_items(row, table, &heard.rail.templates))) {
            Some(list) => list,
            None => return menu,
        }
    };
    let (caller, held, acting, key) = (caller.clone(), heard.clone(), heard.clone(), key.to_owned());
    popup::fill(menu, &items(&list), tokens, move |cx: &App| held.read(cx).held(), move |way, window, cx| act::way(way, &key, &caller, &acting, window, cx), window, cx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::menu::item::lines;
    use argus_desktop_core::checkouts::table::Checkouts;

    #[test]
    fn core_s_list_becomes_the_kits_with_its_keys_and_its_danger() {
        let table = Checkouts { files_offered: true, ..Checkouts::default() };
        let row = crate::checkouts::stage::tests::checkout(1, "desktop", "desktop");
        let kit = items(&ways::row_items(&row, &table, &["claude".to_owned()]));
        assert_eq!(lines(&kit), ["Review", "History", "Pull", "Files", "---", "New shell", "New agent \u{b7} claude", "---", "!Remove"]);
        assert_eq!(items(&ways::strip_items(&table)).len(), 2);
    }
}
