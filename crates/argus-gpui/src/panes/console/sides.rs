//! The console's sides: one screen under its header while split, and what to press when no
//! console is on screen.

use argus_desktop_core::panes::console::{ConsoleGesture, ConsolePage, ConsoleTab};
use gpui_kit::component::button::Button;
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, Context, Entity, Role, SharedString};

use super::ConsoleView;
use super::head::dot;
use super::menu::build;
use super::strip::{self};
use crate::kit::tip::tip;
use crate::kit::strip as chrome_strip;
use crate::panes::terminal::Terminal;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

impl ConsoleView {
    /// A split side's header: its console's title and dot, the whole of it a
    /// button that gives that side the keys, and a close that takes it out of
    /// the split and leaves its console a tab. The side with the keys is marked
    /// by the selected colour and an accent edge under it (`.thead.on`).
    pub(super) fn side_head(&self, tab: &ConsoleTab, page: &ConsolePage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let pane = tab.pane;
        let typing = page.typing == Some(pane);
        let offers = strip::tab_offers(tab, true, &page.tabs);
        let (caller, heard, tokens) = (self.caller.clone(), self.heard.clone(), *tokens);
        let status = tab.status;
        let (fill, foreground, muted, selected) = (tokens.fill, tokens.fg, tokens.fg_muted, tokens.bg_selected);
        let title = SharedString::from(tab.title.clone());
        let words = if typing { tab.title.clone() } else { format!("Type in {}", tab.title) };
        h_flex()
            .id(SharedString::from(format!("console-side-head-{pane}")))
            .test_support()
            .aria_selected(typing)
            .relative()
            .flex_shrink_0()
            .h(rem(tokens.pane_head_h))
            .items_center()
            .bg(if typing { selected } else { tokens.bg_chrome })
            .border_b_1()
            .border_color(tokens.line)
            .typed(tokens.type_small)
            .weighted(tokens.weight_medium)
            .text_color(if typing { foreground } else { muted })
            // `.thead:not(.on):hover`: the fill and the title lit; `.thead.on:hover`: the
            // selected colour with 15% of the fill over it.
            .when(!typing, |head| head.hover(move |style| style.bg(fill).text_color(foreground)))
            .when(typing, |head| head.hover(move |style| style.bg(selected.blend(fill.opacity(tokens.share_press_icon)))))
            .child(
                h_flex()
                    .id(SharedString::from(format!("console-side-pick-{pane}")))
                    .role(Role::Button)
                    .test_support()
                    .aria_label(SharedString::from(words.clone()))
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .px(rem(tokens.sp_12))
                    .gap(rem(tokens.sp_6))
                    .items_center()
                    .cursor_pointer()
                    .tooltip(tip(words.clone()))
                    .on_click(cx.listener(move |this, _, _, cx| this.gesture(ConsoleGesture::Select { pane }, cx)))
                    .child(chrome_strip::title(title))
                    .when(strip::wears_dot(status), |pick| {
                        pick.child(dot(format!("console-side-dot-{pane}"), status, &tokens))
                    }),
            )
            .child(
                chrome_strip::close_x(
                    SharedString::from(format!("console-side-close-{pane}")),
                    format!("Take {} out of the split", tab.title),
                    &tokens,
                )
                .tooltip(tip("Take out of the split"))
                .on_click(cx.listener(move |this, _, _, cx| this.gesture(ConsoleGesture::CloseSide { pane }, cx))),
            )
            .when(typing, |head| head.child(div().absolute().left_0().right_0().bottom_0().h(rem(tokens.rule)).bg(tokens.accent)))
            .context_menu(move |menu, window, cx| build(menu, &offers, &caller, &heard, tokens, window, cx))
    }

    /// One side of the console: its screen, under its header while split.
    pub(super) fn side(&self, id: &'static str, pane: u64, screen: Entity<Terminal>, page: &ConsolePage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let head = page
            .beside
            .and_then(|_| page.tabs.iter().find(|tab| tab.pane == pane))
            .map(|tab| self.side_head(tab, page, tokens, cx).into_any_element());
        v_flex()
            .id(id)
            .test_support()
            .flex_1()
            .min_w_0()
            .h_full()
            .children(head)
            .child(div().flex_1().min_h_0().child(screen))
    }

    /// With no console on screen: what to press to get one.
    pub(super) fn empty(&self, page: &ConsolePage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let chosen = page.tabs.iter().find(|tab| tab.on).or(page.tabs.first()).map(|tab| tab.pane);
        let body = v_flex().id("console-empty").test_support().flex_1().min_h_0().items_center().justify_center().gap_2();
        match chosen {
            None => body
                .child(
                    div()
                        .text_sm()
                        .text_color(tokens.fg_muted)
                        .child("No consoles yet: a shell in your home directory, in no checkout."),
                )
                .when(page.can_spawn, |body| {
                    body.child(
                        Button::new("console-start")
                            .label("Start a console")
                            .on_click(cx.listener(|this, _, _, cx| this.gesture(ConsoleGesture::Spawn, cx))),
                    )
                }),
            Some(pane) => body.child(
                Button::new("console-type")
                    .label("Type in the console")
                    .on_click(cx.listener(move |this, _, _, cx| this.gesture(ConsoleGesture::Select { pane }, cx))),
            ),
        }
    }
}
