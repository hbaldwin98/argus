//! The console's strip: a tab per console with its dot, and the buttons that make, hide and open
//! the menu of the console.

use argus_desktop_core::keys::keymap::{Action, Place};
use argus_desktop_core::layout::docks::DocksPage;
use argus_desktop_core::panes::console::{ConsoleGesture, ConsolePage, ConsoleTab};
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _};
use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, px, Anchor, ClickEvent, Context, MouseButton, Role, SharedString};

use super::ConsoleView;
use super::menu::build;
use super::strip::{self};
use crate::kit::tip::tip;
use crate::keys::caps;
use crate::kit::icons::Glyph;
use crate::kit::status_dot::status_dot;
use crate::kit::strip as chrome_strip;
use crate::theme::Tokens;

impl ConsoleView {
    pub(super) fn tab(&self, tab: &ConsoleTab, page: &ConsolePage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let pane = tab.pane;
        let offers = strip::tab_offers(tab, false, &page.tabs);
        let (caller, heard) = (self.caller.clone(), self.heard.clone());
        let tokens = *tokens;
        let status = tab.status;
        let title = SharedString::from(tab.title.clone());
        chrome_strip::tab(SharedString::from(format!("console-tab-{pane}")), tab.on, true, title.clone(), &tokens)
            // The press is the tab's alone, as the page's `closesOnMiddle`
            // keeps it from autoscrolling or reaching what is under it.
            .on_mouse_down(MouseButton::Middle, |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
            })
            // A click, as the page's `auxclick`: the press and the release both on the tab.
            .on_aux_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                if event.is_middle_click() {
                    cx.stop_propagation();
                    this.gesture(ConsoleGesture::Close { pane }, cx)
                }
            }))
            .child(
                // `.ctab-pick`: the tab's own button, 6px between the title and the dot.
                h_flex()
                    .id(SharedString::from(format!("console-tab-pick-{pane}")))
                    .role(Role::Button)
                    .test_support()
                    .aria_label(title.clone())
                    .min_w_0()
                    .h_full()
                    .gap(px(tokens.sp_6))
                    .items_center()
                    .cursor_pointer()
                    .tooltip(tip(format!("shell, {status}")))
                    // The dock's drag begins on a press in the strip; a tab's is a pick.
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _, _, cx| this.gesture(ConsoleGesture::Select { pane }, cx)))
                    .child(chrome_strip::title(title.clone()))
                    .when(strip::wears_dot(status), |pick| {
                        pick.child(dot(format!("console-tab-dot-{pane}"), status, &tokens))
                    }),
            )
            .child(
                chrome_strip::close_x(SharedString::from(format!("console-tab-close-{pane}")), format!("Close {}", tab.title), &tokens)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _, _, cx| this.gesture(ConsoleGesture::Close { pane }, cx))),
            )
            .context_menu(move |menu, _, cx| build(menu, &offers, &caller, &heard, tokens, cx))
    }

    pub(super) fn head(&self, page: &ConsolePage, docks: &DocksPage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs: Vec<_> = page.tabs.iter().map(|tab| self.tab(tab, page, tokens, cx).into_any_element()).collect();
        let (offers, hide) = {
            let keymap = &self.heard.read(cx).keymap;
            (
                strip::strip_offers(page, docks, keymap),
                caps::titled("Hide the console", keymap, Action::ToggleConsole, Place::Anywhere),
            )
        };
        let (caller, heard, ledger_copy) = (self.caller.clone(), self.heard.clone(), *tokens);
        chrome_strip::bar(tokens)
            .id("console-head")
            .test_support()
            // `.chead`: a little more room before the first tab.
            .pl(px(tokens.sp_12))
            .child(
                h_flex()
                    .id("console-tabs")
                    .role(Role::TabList)
                    .min_w_0()
                    .ml(px(tokens.sp_4))
                    .gap(px(tokens.sp_4))
                    .overflow_hidden()
                    .children(tabs),
            )
            .when(page.can_spawn, |head| {
                head.child(keep(
                    chrome_strip::menu_button("console-new", Glyph::Plus, 14.0, tokens, cx)
                        .tooltip("New console")
                        .on_click(cx.listener(|this, _, _, cx| this.gesture(ConsoleGesture::Spawn, cx))),
                ))
            })
            .child(div().flex_1())
            .child(
                // The kit's button has no accessible label of its own, so the wrapper carries it.
                div().id("console-menu-label").role(Role::Button).test_support().aria_label("Console menu").child(keep(
                    chrome_strip::menu_button("console-menu", Glyph::More, 14.0, tokens, cx)
                        .tooltip("Split, move, fill the window")
                        .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, cx| build(menu, &offers, &caller, &heard, ledger_copy, cx)),
                )),
            )
            .child(
                // The tooltip's words are the button's name too, which the kit's button lacks.
                div().id("console-hide-label").role(Role::Button).test_support().aria_label(hide.clone()).child(keep(
                    chrome_strip::menu_button("console-hide", Glyph::Close, 12.0, tokens, cx)
                        .tooltip(hide)
                        .on_click(cx.listener(|this, _, _, cx| this.gesture(ConsoleGesture::Hide, cx))),
                )),
            )
    }
}

/// The dot a console wears, named so a reader can find it.
pub(super) fn dot(id: String, status: &str, tokens: &Tokens) -> impl IntoElement {
    div().id(SharedString::from(id.clone())).test_support().flex_shrink_0().child(status_dot(id, status, tokens))
}

/// A strip button that keeps the press: the kit's button does not stop the one the dock reads as a drag.
pub(super) fn keep(button: impl IntoElement) -> gpui_kit::Div {
    div().flex_shrink_0().on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).child(button)
}
