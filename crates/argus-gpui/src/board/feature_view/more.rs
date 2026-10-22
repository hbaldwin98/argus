//! The overflow of a row of actions (`overflow` in the page's `work.ts`): what is rarer than the
//! buttons beside it, behind one `...` so the row never wraps a lone button under itself.
//!
//! A choice is an [`Act`], so it does what its key does; the menu opens under the button, its right edge
//! on the button's.

use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu};
use gpui_kit::{div, prelude::*, Anchor, AnyElement, App, Context, Entity, MouseButton, Role, TestSupportExt as _, Window};

use super::act::Act;
use super::OpenFeature;
use crate::kit::icons::Glyph;
use crate::kit::menu::item::MenuItem;
use crate::kit::menu::popup;
use crate::kit::strip::menu_button;
use crate::theme::Tokens;

fn build(menu: PopupMenu, items: &[MenuItem<Act>], feature: &Entity<OpenFeature>, tokens: Tokens, window: &mut Window, cx: &mut Context<PopupMenu>) -> PopupMenu {
    let (acting, held) = (feature.clone(), feature.read(cx).heard.clone());
    popup::fill(menu, items, tokens, move |cx| held.read(cx).held(), move |act, window, cx| acting.update(cx, |feature, cx| feature.act(act.clone(), window, cx)), window, cx)
}

/// The `...` button named `label`, with `items` under it.
pub fn overflow(id: &'static str, label: &'static str, items: Vec<MenuItem<Act>>, feature: &Entity<OpenFeature>, tokens: &Tokens, cx: &App) -> AnyElement {
    let (feature, tokens) = (feature.clone(), *tokens);
    div()
        .id(id)
        .role(Role::Button)
        .test_support()
        .aria_label(label)
        .flex_shrink_0()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            menu_button(gpui_kit::SharedString::from(format!("{id}-button")), Glyph::More, tokens.icon_sm, &tokens, cx)
                .tooltip(label)
                .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, window, cx| build(menu, &items, &feature, tokens, window, cx)),
        )
        .into_any_element()
}
