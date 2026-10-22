//! What a docked extension view carries in its dock's tab strip: a button that opens it as a page
//! and a `...` menu of its actions (SHELL-DOCK-5, `extensions/views.ts` `extensionTools`).
//!
//! What the view can do is the extension's: its table says the actions, which answer to a key, and
//! the row selected. The menu lists the view-wide actions, then every action that answers to a key,
//! run on the selected row, so the keys are listed once here and not on every row; the ones that
//! need a row are refused while none is selected. A view that is already a page has no tools here
//! (it carries its way back on its own head). Choosing sends the call and changes nothing itself.

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::extensions::views::{ExtTable, ExtensionGesture};
use argus_desktop_core::keys::keymap::{Action, Group, Place};
use argus_desktop_core::layout::docks::Panel;
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, Anchor, App, Entity, MouseButton, Role, SharedString};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::keys::caps;
use crate::kit::icons::Glyph;
use crate::kit::menu::item::MenuItem;
use crate::kit::strip;
use crate::layout::dock_menu;
use crate::theme::Tokens;

/// The tools of one docked view.
#[derive(Debug, Clone)]
pub struct Tools {
    pub panel: Panel,
    pub title: String,
    /// What the button that opens it as a page says, with its keys.
    pub page_words: String,
    pub menu: Vec<MenuItem<Call>>,
}

impl Tools {
    /// The call that opens the view as a page.
    pub fn page(&self) -> Call {
        Call::ExtensionMode { panel: self.panel.clone(), page: true }
    }
}

fn act(panel: &Panel, action: &str) -> Call {
    Call::Extension { panel: panel.clone(), gesture: ExtensionGesture::Act { action: action.to_owned() } }
}

/// `panel`'s tools, if it is an extension's view on screen docked: the tables say which are on
/// screen, and one that is a page has none.
pub fn of(panel: &Panel, tables: &[ExtTable], keymap: &[Group]) -> Option<Tools> {
    let view = tables.iter().find(|table| &table.panel == panel).filter(|table| !table.page)?;
    // An ended console's actions are in the strip beside how it ended.
    let ended = view.console.as_ref().is_some_and(|console| console.ended.is_some());
    let wide = view.actions.iter().filter(|action| action.view_wide && !ended);
    let rows: Vec<_> = view.actions.iter().filter(|action| !action.view_wide && action.key.is_some()).collect();
    let mut menu: Vec<MenuItem<Call>> = wide.map(|action| MenuItem::choice(action.label.clone(), act(panel, &action.id))).collect();
    if !menu.is_empty() && !rows.is_empty() {
        menu.push(MenuItem::Rule);
    }
    if !rows.is_empty() {
        menu.push(MenuItem::heading("Keys, on the selected row"));
    }
    menu.extend(rows.into_iter().map(|action| {
        MenuItem::choice(action.label.clone(), act(panel, &action.id))
            .keyed(action.key.iter().map(|key| vec![key.clone()]).collect())
            .refused_when(view.selected.is_none(), "no row selected")
    }));
    Some(Tools {
        panel: panel.clone(),
        title: view.title.clone(),
        page_words: caps::titled("Open as page", keymap, Action::ExtensionPage, Place::Extension),
        menu,
    })
}

/// The two buttons, to follow the strip's tabs: each keeps its press from the panel drag.
pub fn buttons(tools: &Tools, caller: &Caller, heard: &Entity<Heard>, tokens: &Tokens, cx: &App) -> Vec<AnyElement> {
    let key = tools.panel.to_string();
    let keep = |button: gpui_kit::Div| button.flex_shrink_0().on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
    let (page, asking, answering) = (tools.page(), caller.clone(), heard.clone());
    let words = SharedString::from(tools.page_words.clone());
    let open = keep(div()).child(
        div()
            .id(SharedString::from(format!("dock-tools-page-{key}")))
            .role(Role::Button)
            .test_support()
            .aria_label("Open as page")
            .child(
                strip::menu_button(SharedString::from(format!("dock-tools-open-{key}")), Glyph::External, 14.0, tokens, cx)
                    .tooltip(words)
                    .on_click(move |_, _, cx| {
                        if !answering.read(cx).held() {
                            asking.call(page.clone());
                        }
                    }),
            ),
    );
    let (caller, heard, entries, menu_tokens) = (caller.clone(), heard.clone(), tools.menu.clone(), *tokens);
    let more = keep(div()).child(
        div()
            .id(SharedString::from(format!("dock-tools-more-{key}")))
            .role(Role::Button)
            .test_support()
            .aria_label(SharedString::from(format!("{} menu", tools.title)))
            .child(
                strip::menu_button(SharedString::from(format!("dock-tools-menu-{key}")), Glyph::More, 14.0, tokens, cx)
                    .tooltip("More")
                    .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, window, cx| dock_menu::build(menu, &entries, &caller, &heard, menu_tokens, window, cx)),
            ),
    );
    vec![open.into_any_element(), more.into_any_element()]
}

#[cfg(test)]
mod tests;
