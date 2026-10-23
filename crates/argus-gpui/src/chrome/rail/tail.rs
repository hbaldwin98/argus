//! The nav's dynamic tail: the Extensions fold and the Views fold after the built-in items, each
//! drawing nothing while it holds nothing (SHELL-STAGE-18).
//!
//! Extensions lists every view of an extension listed where the person is, its fold Rust's
//! (`toggle_extensions`); Views lists the saved views by scope, filtered to here or all, nested
//! under the view that links to them, its folds the window's own (`ViewsFold`). A row is lit while
//! the view it names is on screen, and is a handle that docks it: a press that travels is the
//! work area's drag of a panel, a press that does not is the row's own click.

use std::collections::HashSet;

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::extensions::views::ExtTab;
use argus_desktop_core::views::nav::{rows, NavView, Row};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, px, AnyElement, Context, MouseButton, MouseDownEvent, Role};

use super::row::{line, Ctx};
use super::slice::Slice;
use super::RailView;
use crate::kit::icons::Glyph;
use crate::kit::segment;
use crate::kit::tip::tip;
use crate::layout::carry::Carrier;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// What only the window knows of the Views fold: whether it is unfolded, whether it lists the views
/// about here or all, and which nested lists are unfolded, by the id of the view they hang from.
#[derive(Debug, Default)]
pub(super) struct ViewsFold {
    pub listed: bool,
    pub all: bool,
    pub unfolded: HashSet<String>,
}

impl ViewsFold {
    /// The twist of the list under a view: unfolds it, or folds it.
    pub fn twist(&mut self, id: &str) {
        if !self.unfolded.remove(id) {
            self.unfolded.insert(id.to_owned());
        }
    }
}

impl RailView {
    pub(super) fn tail(&self, slice: &Slice, ctx: Ctx, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let extensions = (!slice.extensions.is_empty()).then(|| self.extensions(slice, ctx, tokens, cx));
        let saved = (!slice.saved.is_empty()).then(|| self.saved(slice, ctx, tokens, cx));
        v_flex().gap(rem(tokens.sp_2)).children(extensions).children(saved)
    }

    /// `.nav.nav-fold`: the icon cell holding the chevron, the label, the count.
    fn fold(&self, id: &'static str, label: &'static str, count: usize, listed: bool, ctx: Ctx, tokens: &Tokens) -> super::row::Line {
        let fill = tokens.fill;
        line(id, tokens.nav_h, tokens)
            .w_full()
            .px(rem(tokens.sp_8))
            .gap(rem(tokens.sp_12))
            .text_size(rem(tokens.fs_md))
            .text_color(tokens.fg)
            .aria_expanded(listed)
            .aria_label(label)
            .hover(move |style| style.bg(fill))
            .when(ctx.held, |row| row.opacity(tokens.opacity_dim))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(rem(tokens.nav_icon_cell))
                    .flex_shrink_0()
                    .text_color(tokens.fg_muted)
                    .child(if listed { Glyph::Down } else { Glyph::Right }.at(tokens.icon_xs)),
            )
            .child(div().flex_1().min_w_0().truncate().child(label))
            .child(div().typed(tokens.type_small).text_color(tokens.fg_muted).child(count.to_string()))
    }

    fn extensions(&self, slice: &Slice, ctx: Ctx, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let held = ctx.held;
        let listed = slice.extensions_listed;
        let views: Vec<AnyElement> = if listed {
            slice.extensions.iter().map(|view| self.extension_view(view, ctx, tokens, cx).into_any_element()).collect()
        } else {
            Vec::new()
        };
        v_flex()
            .child(self.fold("nav-extensions", "Extensions", slice.extensions.len(), listed, ctx, tokens).on_click(cx.listener(move |this, _, _, _| this.ask(held, Call::ToggleExtensions))))
            // `.nav-nest`: the views, 12px in.
            .when(listed, |col| col.child(v_flex().id("nav-extensions-list").role(Role::Group).aria_label("Extensions").pl(rem(tokens.sp_12)).gap(rem(tokens.sp_2)).children(views)))
    }

    /// An extension's view: its mark, title and the extension it is from, lit while it is on screen.
    fn extension_view(&self, view: &ExtTab, ctx: Ctx, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let (held, fill) = (ctx.held, tokens.fill);
        let (extension, name) = (view.extension.clone(), view.view.clone());
        let tooltip = format!("{} \u{b7} from {}{}", view.title, view.extension, if view.page { "" } else { ", docked" });
        let (panel, title) = (view.panel.clone(), view.title.clone());
        let row = line(format!("ext-view-{}-{}", view.extension, view.view), tokens.nav_h, tokens)
            .w_full()
            .px(rem(tokens.sp_8))
            .gap(rem(tokens.sp_12))
            .text_size(rem(tokens.fs_md))
            .text_color(tokens.fg)
            .aria_selected(view.on)
            .aria_label(tooltip.clone())
            .tooltip(tip(tooltip))
            .when(view.on, |row| row.bg(tokens.bg_selected))
            .when(!view.on, |row| row.hover(move |style| style.bg(fill)))
            .on_click(cx.listener(move |this, _, _, _| this.ask(held, Call::OpenExtensionView { extension: extension.clone(), view: name.clone() })))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| this.carry_panel(event, &panel, &title, held, cx)),
            );
        row.child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(rem(tokens.nav_icon_cell))
                .flex_shrink_0()
                .text_color(if view.on { tokens.accent } else { tokens.fg_muted })
                .child(Glyph::of_extension(view.icon.as_deref()).at(tokens.icon_glyph)),
        )
        .child(div().flex_1().min_w_0().truncate().child(view.title.clone()))
        .child(div().typed(tokens.type_small).text_color(tokens.fg_muted).child(view.extension.clone()))
    }

    /// Presses a panel into the work area's drag: carried to a dock it docks, and over the stage it
    /// lands where its kind lands (Rust's `PresentPanel`, never guessed from the name).
    pub(super) fn carry_panel(&self, event: &MouseDownEvent, panel: &argus_desktop_core::layout::docks::Panel, title: &str, held: bool, cx: &mut Context<Self>) {
        if held {
            return;
        }
        let stage = self.heard.read(cx).docks.present.iter().find(|present| &present.panel == panel).is_some_and(|present| present.stage);
        Carrier::of(cx).press_panel(event.position, panel.clone(), title.to_owned(), stage, cx);
    }

    fn saved(&self, slice: &Slice, ctx: Ctx, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let listed = self.views.listed;
        let body = listed.then(|| self.saved_list(slice, ctx, tokens, cx));
        v_flex()
            .child(self.fold("nav-views", "Views", slice.saved.len(), listed, ctx, tokens).on_click(cx.listener(|this, _, _, cx| {
                this.views.listed = !this.views.listed;
                cx.notify();
            })))
            .children(body)
    }

    /// `.nav-saved-list`: the Here and All filter, then each scope's label over its views. It
    /// scrolls on its own past a share of the window, so the panes under it stay in sight.
    fn saved_list(&self, slice: &Slice, ctx: Ctx, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let all = self.views.all;
        let list = rows(&slice.saved, !all, &self.views.unfolded);
        let filter = segment::track("nav-filter", "Which saved views", Role::Group, tokens)
            .self_start()
            .mx(rem(tokens.sp_8))
            .mt(rem(tokens.sp_2))
            .mb(rem(tokens.sp_4))
            .child(self.filter_button("views-here", "Here", !all, tokens, cx))
            .child(self.filter_button("views-all", "All", all, tokens, cx));
        let rows: Vec<AnyElement> = list.iter().map(|row| self.saved_row(row, slice, ctx, tokens, cx)).collect();
        v_flex()
            .id("nav-saved-list")
            .role(Role::Group)
            .aria_label("Saved views")
            // Its rows have no icon, so they start where the fold's label does.
            .pl(rem(tokens.nav_icon_cell + tokens.sp_12))
            .max_h(px(ctx.height * tokens.nav_saved_share))
            .overflow_y_scroll()
            .child(filter)
            .children(rows)
    }

    fn filter_button(&self, id: &'static str, label: &'static str, on: bool, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let here = label == "Here";
        segment::pill(id, Role::Button, on, tokens)
            .h(rem(tokens.chip_h))
            .px(rem(tokens.sp_8))
            .text_size(rem(tokens.fs_xs))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.views.all = !here;
                cx.notify();
            }))
            .child(label)
    }

    fn saved_row(&self, row: &Row, slice: &Slice, ctx: Ctx, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        match row {
            Row::None => div().id("nav-empty").test_support().aria_label("None about here").px(rem(tokens.sp_8)).py(rem(tokens.sp_4)).typed(tokens.type_small).text_color(tokens.fg_muted).child("None about here").into_any_element(),
            Row::Group(label) => div()
                .px(rem(tokens.sp_8))
                .pt(rem(tokens.sp_8))
                .pb(rem(tokens.sp_2))
                .text_color(tokens.fg_faint)
                .text_size(rem(tokens.fs_xs))
                .weighted(tokens.weight_semibold)
                .child(label.to_uppercase())
                .into_any_element(),
            Row::View { view, label, depth, inner, folded } => {
                self.saved_view(view, label, *depth, *inner, *folded, slice.lit.contains(&view.id), ctx, tokens, cx).into_any_element()
            }
        }
    }

    /// A saved view's row, and the twist that unfolds the views listed under it.
    #[allow(clippy::too_many_arguments)]
    fn saved_view(&self, view: &NavView, label: &str, depth: usize, inner: usize, folded: bool, lit: bool, ctx: Ctx, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let (held, fill) = (ctx.held, tokens.fill);
        let (id, panel, title) = (view.id.clone(), view.panel.clone(), view.title.clone());
        let tooltip = format!("{} \u{b7} {}{}", view.title, view.scope, if view.stale { " \u{b7} stale" } else { "" });
        let open = view.id.clone();
        let twisting = view.id.clone();
        let twist = (inner > 0).then(|| {
            let words = format!("{} the {inner} {} listed under it", if folded { "Show" } else { "Hide" }, if inner == 1 { "view" } else { "views" });
            line(format!("saved-{id}-twist"), tokens.control_h_sm, tokens)
                .flex_shrink_0()
                .px(rem(tokens.sp_4))
                .gap(rem(tokens.sp_2))
                .text_size(rem(tokens.fs_xs))
                .text_color(tokens.fg_muted)
                .aria_expanded(!folded)
                .aria_label(words.clone())
                .tooltip(tip(words))
                .hover(move |style| style.bg(fill))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.views.twist(&twisting);
                    cx.notify();
                }))
                .child(inner.to_string())
                .child(if folded { Glyph::Right } else { Glyph::Down }.at(tokens.icon_xs))
        });
        h_flex()
            .items_center()
            .min_w_0()
            .pl(rem(tokens.sp_12 * depth as f32))
            .child(
                line(format!("saved-{}", view.id), tokens.nav_h, tokens)
                    .flex_1()
                    .min_w_0()
                    .px(rem(tokens.sp_8))
                    .gap(rem(tokens.sp_12))
                    .text_size(rem(tokens.fs_md))
                    .text_color(tokens.fg)
                    .aria_selected(lit)
                    .aria_label(tooltip.clone())
                    .tooltip(tip(tooltip))
                    .when(lit, |row| row.bg(tokens.bg_selected))
                    .when(!lit, |row| row.hover(move |style| style.bg(fill)))
                    .on_click(cx.listener(move |this, _, _, _| this.ask(held, Call::OpenSavedView { id: open.clone(), from: None })))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, _, cx| this.carry_panel(event, &panel, &title, held, cx)),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(label.to_owned()))
                    .children(view.stale.then(|| {
                        div().id(format!("saved-{id}-stale")).test_support().role(Role::Image).aria_label("stale").size(rem(tokens.dot_md)).flex_shrink_0().rounded_full().bg(tokens.attention)
                    })),
            )
            .children(twist)
    }
}
