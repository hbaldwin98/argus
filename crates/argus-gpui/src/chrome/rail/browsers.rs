//! The Browsers item's own parts in the nav (SHELL-STAGE-17, SHELL-BROWSER-14): the count of open
//! browsers, the chevron that folds the list, and one sub-row for each browser in the stage's order.
//!
//! A sub-row is a dot of who has the browser (`browser::chip`), the agent's name and, muted, the
//! page's title or else its address. It is lit while the Workspace is up and the browser's pane is on
//! screen anywhere in its grid. A press shows that pane (`show_browsers{pane}`); a press that
//! travels is the drag of that pane with no checkout, so no checkout row is a target (SHELL-RAIL-37).
//! The fold starts unfolded and is the window's own, as the page's is.

use argus_desktop_core::browser::status::{status, Status};
use argus_desktop_core::browser::PageBrowser;
use argus_desktop_core::connection::page::{Call, Rail, Stage};
use gpui_kit::component::v_flex;
use gpui_kit::{div, prelude::*, Context, MouseButton, MouseDownEvent, Role, SharedString, TestSupportExt as _};

use super::row::{line, Ctx};
use super::slice::Slice;
use super::RailView;
use crate::browser::chip::dot;
use crate::kit::icons::Glyph;
use crate::kit::tip::tip;
use crate::layout::carry::{Carried, Carrier};
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// One browser as the nav lists it.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Sub {
    pub pane: u64,
    pub agent: String,
    /// The page's title, else its address.
    pub meta: String,
    pub tooltip: String,
    pub status: Option<Status>,
    pub on: bool,
}

impl Sub {
    pub fn of(browser: &PageBrowser, rail: &Rail) -> Sub {
        let grid = rail.shown.as_ref().is_some_and(|shown| shown.pane == browser.pane) || rail.row.beside.iter().flatten().any(|shown| shown.pane == browser.pane);
        Sub {
            pane: browser.pane,
            agent: browser.agent.clone(),
            meta: if browser.title.is_empty() { browser.url.clone() } else { browser.title.clone() },
            tooltip: format!("{} \u{b7} {}", browser.agent, browser.url),
            status: status(browser),
            on: rail.stage == Stage::Workspace && grid,
        }
    }
}

impl RailView {
    /// The count of open browsers and the chevron that folds their list; both blank with none.
    pub(super) fn browsers_tail(&self, slice: &Slice, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let count = slice.subs.len();
        let listed = self.browsers_listed;
        let words = if listed { "Fold the open browsers" } else { "List the open browsers" };
        let fill = tokens.fill;
        let chevron = (count > 0).then(|| {
            div()
                .id("nav-browsers-fold")
                .test_support()
                .role(Role::Button)
                .aria_expanded(listed)
                .aria_label(words)
                .tooltip(tip(words))
                .flex()
                .items_center()
                .justify_center()
                .flex_none()
                .size(rem(tokens.icon_box))
                .rounded(rem(tokens.r_sm))
                .text_color(tokens.fg_muted)
                .hover(move |style| style.bg(fill).text_color(tokens.fg))
                // A press on the chevron folds the list and does not press the item under it.
                .on_click(cx.listener(|this, _, _, cx| {
                    cx.stop_propagation();
                    this.browsers_listed = !this.browsers_listed;
                    cx.notify();
                }))
                .child(if listed { Glyph::Down } else { Glyph::Right }.at(tokens.icon_xs))
        });
        gpui_kit::component::h_flex()
            .items_center()
            .gap(rem(tokens.sp_4))
            .child(div().id("nav-browsers-count").typed(tokens.type_small).text_color(tokens.fg_muted).child(if count > 0 { count.to_string() } else { String::new() }))
            .children(chevron)
    }

    /// `.nav-nest`: a sub-row for each browser, 12px in.
    pub(super) fn browsers_list(&self, slice: &Slice, ctx: Ctx, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let rows: Vec<_> = slice.subs.iter().map(|sub| self.sub(sub, ctx, tokens, cx)).collect();
        v_flex().id("nav-browsers-list").role(Role::Group).aria_label("Open browsers").pl(rem(tokens.sp_12)).gap(rem(tokens.sp_2)).children(rows)
    }

    fn sub(&self, sub: &Sub, ctx: Ctx, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let (held, fill, pane) = (ctx.held, tokens.fill, sub.pane);
        let carrier = Carrier::of(cx);
        let title = sub.agent.clone();
        let tooltip = SharedString::from(sub.tooltip.clone());
        line(format!("nav-browser-{pane}"), tokens.nav_h, tokens)
            .w_full()
            .px(rem(tokens.sp_8))
            .gap(rem(tokens.sp_12))
            .text_size(rem(tokens.fs_md))
            .text_color(tokens.fg)
            .aria_selected(sub.on)
            .aria_label(tooltip.clone())
            .tooltip(tip(tooltip))
            .when(sub.on, |row| row.bg(tokens.bg_selected))
            .when(!sub.on, |row| row.hover(move |style| style.bg(fill)))
            .on_click(cx.listener(move |this, _, _, _| this.ask(held, Call::ShowBrowsers { pane: Some(pane) })))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_, event: &MouseDownEvent, _, cx| {
                    if !held {
                        carrier.press(event.position, Carried::Rail { pane, title: title.clone(), checkout: None }, cx);
                    }
                }),
            )
            .child(div().flex().items_center().justify_center().size(rem(tokens.nav_icon_cell)).flex_none().child(dot(format!("nav-browser-{pane}-dot"), sub.status, tokens)))
            .child(div().flex_1().min_w_0().truncate().child(sub.agent.clone()))
            .child(div().min_w_0().max_w(gpui_kit::relative(0.5)).truncate().typed(tokens.type_small).text_color(tokens.fg_muted).child(sub.meta.clone()))
    }
}
