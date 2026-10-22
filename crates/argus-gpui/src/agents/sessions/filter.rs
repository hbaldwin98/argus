//! The Filter button and the menu under it: the harness and the checkout the listing is narrowed
//! to, one button for both because they are seldom set (SESSIONS-9).
//!
//! The choices and the call each sends are core's (`said::filter_items`): choosing one filter keeps
//! the other. The menu opens under the button's bottom-right edge, as the page's does; its keys and
//! its Escape (which closes the menu only and leaves the dialog up) are the kit menu's, held to
//! the layer's order by `keys` passing every key while a menu is open.

use argus_desktop_core::agents::said::{filter_active, filter_items, FilterItem};
use argus_desktop_core::agents::sessions_dialog::SessionsPage;
use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme as _, Sizable as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu};
use gpui_kit::{div, prelude::*, Anchor, Context, MouseButton, Role, SharedString, TestSupportExt as _, Window};

use super::SessionsView;
use crate::kit::icons::Glyph;
use crate::kit::menu::item::MenuItem;
use crate::kit::menu::popup;
use crate::style::Typed as _;
use crate::theme::scale::pixels;
use crate::theme::Tokens;

/// The kit's items for core's: a heading, a rule, and a choice with a tick at its right edge when
/// it is the value now.
pub fn items(list: &[FilterItem]) -> Vec<MenuItem<FilterItem>> {
    list.iter()
        .map(|item| match item {
            FilterItem::Heading(words) => MenuItem::heading(*words),
            FilterItem::Rule => MenuItem::Rule,
            FilterItem::Choice { label, checked, .. } => {
                let choice = MenuItem::choice(label.clone(), item.clone());
                if *checked {
                    choice.noted("\u{2713}")
                } else {
                    choice
                }
            }
        })
        .collect()
}

impl SessionsView {
    pub(super) fn filter_button(&self, page: &SessionsPage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let (heard, caller, tokens) = (self.heard.clone(), self.caller.clone(), *tokens);
        let active = filter_active(page);
        let (menu_heard, held_heard) = (heard.clone(), heard);
        div()
            .id("sessions-filter")
            .role(Role::Button)
            .test_support()
            .aria_label("Filter sessions")
            .flex_shrink_0()
            // A press on the button must not reach what is under the dialog's scrim.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                Button::new("sessions-filter-button")
                    .outline()
                    .small()
                    .rounded(pixels(tokens.r_pill, cx.theme().font_size))
                    .child(Glyph::Filter.at(tokens.icon_sm))
                    .child("Filter")
                    .children(active.map(|active| div().typed(tokens.type_small).text_color(tokens.fg_muted).child(SharedString::from(active))))
                    .dropdown_menu_with_anchor(Anchor::TopRight, move |menu: PopupMenu, window: &mut Window, cx: &mut Context<PopupMenu>| {
                        let page = menu_heard.read(cx).sessions.clone();
                        let list = page.as_deref().map(filter_items).unwrap_or_default();
                        let (caller, held) = (caller.clone(), held_heard.clone());
                        popup::fill(
                            menu,
                            &items(&list),
                            tokens,
                            move |cx| held.read(cx).held(),
                            move |item, _, _| {
                                if let Some(call) = item.call() {
                                    caller.call(call);
                                }
                            },
                            window,
                            cx,
                        )
                    }),
            )
    }
}
