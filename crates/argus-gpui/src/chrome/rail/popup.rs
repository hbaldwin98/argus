//! A rail menu drawn: its items as the kit's popup, each choice handed back to the rail to do once
//! the menu has let go of the keys, and every choice off while the daemon is gone.

use gpui_kit::component::menu::PopupMenu;
use gpui_kit::{App, Entity, WeakEntity};

use super::menus::Does;
use super::RailView;
use crate::connection::heard::Heard;
use crate::kit::menu::item::MenuItem;
use crate::kit::menu::popup;
use crate::theme::Tokens;

pub(super) fn fill(menu: PopupMenu, items: &[MenuItem<Does>], rail: &WeakEntity<RailView>, heard: &Entity<Heard>, tokens: Tokens, cx: &App) -> PopupMenu {
    let (rail, asking) = (rail.clone(), heard.clone());
    popup::fill(
        menu,
        items,
        tokens,
        move |cx| asking.read(cx).held(),
        move |does, window, cx| {
            rail.update(cx, |rail, cx| rail.perform(does, window, cx)).ok();
        },
        cx,
    )
}
