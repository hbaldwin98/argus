//! The pane's `...` button at the strip's end and the menu under it: Rename and Close
//! (`paneMenu`, SHELL-STAGE-7).
//!
//! The menu is the page's list without the rail row's `Show` and `Open beside`, since the pane is
//! already the one on screen. Close is sent and the menu changes nothing: the pane goes when the
//! daemon's tree says it has. Rename opens the dialog once the menu has let go of the keys
//! (`kit::menu::popup`), as the console's menu does.

use argus_desktop_core::chrome::strip::Named;
use argus_desktop_core::connection::page::Call;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu};
use gpui_kit::{div, prelude::*, Anchor, App, Context, Entity, MouseButton, Role, SharedString, TestSupportExt as _, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::icons::Glyph;
use crate::kit::menu::item::MenuItem;
use crate::kit::menu::popup;
use crate::kit::strip::menu_button;
use crate::panes::console::rename;
use crate::theme::Tokens;

/// What a choice of the menu does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Does {
    Rename { pane: u64, title: String },
    Close { pane: u64 },
}

/// The pane's menu: its name, Rename, a rule, and Close in the danger colour.
pub fn menu(pane: &Named) -> Vec<MenuItem<Does>> {
    vec![
        MenuItem::heading(pane.title.clone()),
        MenuItem::choice("Rename\u{2026}", Does::Rename { pane: pane.pane, title: pane.title.clone() }),
        MenuItem::Rule,
        MenuItem::choice("Close", Does::Close { pane: pane.pane }).danger(),
    ]
}

fn build(menu: PopupMenu, items: &[MenuItem<Does>], caller: &Caller, heard: &Entity<Heard>, tokens: Tokens, window: &mut Window, cx: &mut Context<PopupMenu>) -> PopupMenu {
    let (caller, asking, opening) = (caller.clone(), heard.clone(), heard.clone());
    popup::fill(
        menu,
        items,
        tokens,
        move |cx| asking.read(cx).held(),
        move |does, window, cx| match does {
            Does::Close { pane } => caller.call(Call::Close { pane: *pane }),
            Does::Rename { pane, title } => rename::open(*pane, title.clone(), caller.clone(), opening.clone(), window, cx),
        },
        window,
        cx,
    )
}

/// `More: rename or close this pane`, under which the menu opens, right-aligned with the button.
pub fn button(pane: &Named, caller: &Caller, heard: &Entity<Heard>, tokens: &Tokens, cx: &App) -> impl IntoElement {
    let (items, caller, heard, tokens) = (menu(pane), caller.clone(), heard.clone(), *tokens);
    let words = SharedString::from(format!("More about {}", pane.title));
    // The kit's button has no accessible label of its own, so the wrapper carries it.
    div().id("head-more").role(Role::Button).test_support().aria_label(words).flex_shrink_0().on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).child(
        menu_button("head-more-button", Glyph::More, tokens.icon_sm, &tokens, cx)
            .tooltip("More: rename or close this pane")
            .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, window, cx| build(menu, &items, &caller, &heard, tokens, window, cx)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::menu::item::lines;

    #[test]
    fn the_menu_names_the_pane_and_offers_rename_and_a_close_in_danger() {
        let items = menu(&Named { pane: 7, title: "Trim context".into() });
        assert_eq!(lines(&items), ["# Trim context", "Rename\u{2026}", "---", "!Close"]);
    }

    #[test]
    fn each_choice_acts_on_the_pane_it_is_of() {
        let items = menu(&Named { pane: 7, title: "Trim context".into() });
        let does: Vec<_> = items.iter().filter_map(|item| item.as_choice().map(|choice| choice.does.clone())).collect();
        assert_eq!(does, [Does::Rename { pane: 7, title: "Trim context".into() }, Does::Close { pane: 7 }]);
    }
}
