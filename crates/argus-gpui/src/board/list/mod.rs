//! The list layout: the features as rows under group labels, beside the peek panel (`list` of
//! `feature_stage.ts`).
//!
//! Custom rows, not the kit's list: a row has a second line, and the groups are labels between
//! rows. Which rows there are and in what order is Rust's (`Features::rows`); the view draws a
//! label whenever the group changes and never sorts. The list keeps its own scroll across the
//! snapshots a push brings, since the entity outlives them.

pub mod row;

use std::cell::Cell;
use std::rc::Rc;

use argus_desktop_core::board::index_words::{self, FeatureColumn};
use argus_desktop_core::connection::page::Features;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{canvas, div, prelude::*, Bounds, Context, Entity, Pixels, ScrollHandle, SharedString, TestSupportExt as _, Window};

use super::cells::meta;
use super::ctx::Ctx;
use super::menus::Menu;
use super::peek;
use crate::style::Typed as _;
use crate::theme::scale::{css, rem};
use crate::theme::Tokens;
use row::Setting;

pub struct IndexList {
    ctx: Ctx,
    menu: Entity<Menu>,
    scroll: ScrollHandle,
    peek: ScrollHandle,
    /// The width the list was last laid out at, in real pixels: the Checkout column goes when it is
    /// narrow (`@container (max-width: 520px)`).
    width: Rc<Cell<Pixels>>,
    /// The width of the whole index, the peek's share is taken of (`.peek`'s `34%`).
    stage: Rc<Cell<Pixels>>,
    /// Where the peeked row is, for the Move menu the `M` key opens beside it.
    anchor: Rc<Cell<Bounds<Pixels>>>,
    /// Where the peek's `...` button is, which the menu opens under.
    more: Rc<Cell<Bounds<Pixels>>>,
}

impl IndexList {
    pub fn new(ctx: Ctx, menu: Entity<Menu>, anchor: Rc<Cell<Bounds<Pixels>>>) -> Self {
        Self { ctx, menu, scroll: ScrollHandle::new(), peek: ScrollHandle::new(), width: Rc::default(), stage: Rc::default(), anchor, more: Rc::default() }
    }
}

/// The names of the columns over the rows, in the label type.
fn labels(checkout: bool, tokens: &Tokens) -> impl IntoElement {
    let label = |words: &'static str| div().typed(tokens.type_label).text_color(tokens.fg_faint).child(words.to_uppercase());
    h_flex()
        .id("feature-labels")
        .test_support()
        .w_full()
        .flex_shrink_0()
        .items_center()
        .gap(rem(tokens.sp_8))
        .px(rem(tokens.sp_12))
        .pb(rem(tokens.sp_4))
        .child(div().w(rem(tokens.fx_num_w)).flex_shrink_0())
        .child(label("Feature").flex_1().min_w(rem(tokens.fx_title_min)))
        .child(label("Tasks").w(rem(tokens.fx_tasks_w)).flex_shrink_0())
        .child(label("Questions").w(rem(tokens.fx_questions_w)).flex_shrink_0())
        .when(checkout, |line| line.child(label("Checkout").flex_shrink_0().w(rem(tokens.fx_checkout_max))))
}

/// The heading a run of rows of one group sits under, tinted as the group's standing is.
fn group(column: FeatureColumn, tokens: &Tokens) -> impl IntoElement {
    let colour = super::cells::tone_colour(index_words::group_tone(column), tokens.fg_faint, tokens);
    div()
        .id(SharedString::from(format!("group-{}", column.word())))
        .test_support()
        .aria_label(column.label())
        .px(rem(tokens.sp_12))
        .pt(rem(tokens.sp_12))
        .pb(rem(tokens.sp_2))
        .typed(tokens.type_label)
        .text_color(colour)
        .child(column.label().to_uppercase())
}

fn rows(features: &Features, setting: &Setting, ctx: &Ctx, menu: &Entity<Menu>, tokens: &Tokens) -> Vec<gpui_kit::AnyElement> {
    let mut drawn = Vec::new();
    let mut under = None;
    for feature in &features.rows {
        let column = FeatureColumn::of(feature.group);
        if column != under {
            under = column;
            drawn.extend(column.map(|column| group(column, tokens).into_any_element()));
        }
        drawn.push(row::row(feature, setting, ctx, menu, tokens).into_any_element());
    }
    drawn
}

impl Render for IndexList {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let Some(features) = self.ctx.features(cx) else { return div().into_any_element() };
        let wide = css(self.width.get(), window.rem_size());
        let checkout = wide <= 0.0 || wide > tokens.fx_list_hides_checkout_below;
        let setting = Setting { checkout, anchor: self.anchor.clone() };
        let stage = css(self.stage.get(), window.rem_size());
        let (measured, this) = (self.width.clone(), cx.entity());
        let (stage_cell, stage_this) = (self.stage.clone(), cx.entity());
        let section = v_flex()
            .id("feature-list")
            .test_support()
            .aria_label("Features")
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .p(rem(tokens.sp_12))
            .gap(rem(tokens.sp_2))
            .border_r_1()
            .border_color(tokens.line)
            .child(
                canvas(
                    move |bounds, _, cx| {
                        if measured.get() != bounds.size.width {
                            measured.set(bounds.size.width);
                            let this = this.clone();
                            cx.defer(move |cx| this.update(cx, |_, cx| cx.notify()));
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(labels(checkout, &tokens))
            .children(rows(&features, &setting, &self.ctx, &self.menu, &tokens))
            .when(features.rows.is_empty(), |list| {
                let why = index_words::nothing_listed(&features);
                list.child(
                    meta(why.clone(), &tokens)
                        .id("feature-list-empty")
                        .test_support()
                        .aria_label(SharedString::from(why))
                        .px(rem(tokens.sp_12))
                        .pt(rem(tokens.sp_12))
                        .pb(rem(tokens.sp_2))
                        .whitespace_normal(),
                )
            });
        h_flex()
            .id("feature-index")
            .test_support()
            .size_full()
            .min_h_0()
            .relative()
            .child(
                canvas(
                    move |bounds, _, cx| {
                        if stage_cell.get() != bounds.size.width {
                            stage_cell.set(bounds.size.width);
                            let this = stage_this.clone();
                            cx.defer(move |cx| this.update(cx, |_, cx| cx.notify()));
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(section)
            .child(peek::panel(&features, &self.ctx, &self.menu, &self.peek, self.more.clone(), stage, &tokens))
            .into_any_element()
    }
}
