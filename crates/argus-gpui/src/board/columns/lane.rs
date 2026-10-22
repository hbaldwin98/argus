//! One column of the board (`.kcol`): its head with the count, its cards or the two lines it says
//! when it has none, and what is special to it: Archived folds into a single row that still takes
//! a dropped card, and unfolded ends with a button to fold it again.
//!
//! The columns are Held over Archived in the side stack and three lanes beside it. Which column a
//! card is in is its `group`, as Rust says it, and the count is the number of cards except in a
//! folded Archived, which counts what it folds (`Features::folded`).


use argus_desktop_core::board::index_words::{self, FeatureColumn};
use argus_desktop_core::connection::page::{Call, FeatureRow, Features};
use argus_desktop_core::keys::keymap::Action;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{canvas, div, prelude::*, AnyElement, Context, DefiniteLength, Length, MouseButton, MouseDownEvent, Rems, SharedString, Styled, TestSupportExt as _};

use super::card::{card, placeholder, Dress};
use super::{Boxes, Grab, Lanes, Press};
use crate::kit::icon_button::Node;
use crate::kit::icons::Glyph;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// `flex-basis`, which the kit's builder has no call for.
fn basis<E: Styled>(mut element: E, width: Rems) -> E {
    element.style().flex_basis = Some(Length::Definite(DefiniteLength::from(width)));
    element
}

/// Records the box its parent is laid out in as `column`'s.
fn measured(boxes: Boxes<FeatureColumn>, column: FeatureColumn) -> impl IntoElement {
    canvas(
        move |bounds, _, _| {
            let mut held = boxes.borrow_mut();
            match held.iter_mut().find(|(one, _)| *one == column) {
                Some(entry) => entry.1 = bounds,
                None => held.push((column, bounds)),
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
}

/// `.kdot`: a ring for what is queued, the accent for what is active, ok for what is done, and a
/// faint dot for what is set aside.
fn dot(column: FeatureColumn, tokens: &Tokens) -> impl IntoElement {
    let size = rem(tokens.dot_lg);
    let base = div().flex_shrink_0().w(size).h(size).rounded_full();
    match column {
        FeatureColumn::Queued => base.border_color(tokens.fg_faint).border_1(),
        FeatureColumn::Active => base.bg(tokens.accent),
        FeatureColumn::Done => base.bg(tokens.ok),
        _ => base.bg(tokens.fg_faint),
    }
}

/// `.kcount`: the number in a small pill.
fn count(words: String, tokens: &Tokens) -> impl IntoElement {
    div().min_w(rem(18.0)).px(rem(tokens.sp_6)).rounded(rem(tokens.r_pill)).bg(tokens.fill).typed(tokens.type_small).text_color(tokens.fg_muted).text_center().child(words)
}

/// `.khead`: the dot, the name and the count.
fn head(column: FeatureColumn, shown: usize, tokens: &Tokens) -> impl IntoElement {
    h_flex()
        .h(rem(tokens.row_h))
        .px(rem(tokens.sp_8))
        .gap(rem(tokens.sp_8))
        .items_center()
        .whitespace_nowrap()
        .typed(tokens.type_heading)
        .text_color(tokens.fg)
        .child(dot(column, tokens))
        .child(column.label())
        .child(count(shown.to_string(), tokens))
}

/// `.kempty`: what a column with no cards says, and what lands there.
fn empty(column: FeatureColumn, tokens: &Tokens) -> impl IntoElement {
    let (title, why) = column.empty();
    v_flex()
        .id(SharedString::from(format!("empty-{}", column.word())))
        .test_support()
        .aria_label(title)
        .items_center()
        .gap(rem(tokens.sp_4))
        .px(rem(tokens.sp_12))
        .py(rem(tokens.sp_24))
        .border_1()
        .border_dashed()
        .border_color(tokens.line_strong)
        .rounded(rem(tokens.r_md))
        .text_center()
        .typed(tokens.type_small)
        .text_color(tokens.fg_faint)
        .when(column == FeatureColumn::Done, |box_| box_.child(div().mb(rem(tokens.sp_8)).text_color(tokens.ok).child(Glyph::Settled.at(40.0))))
        .child(div().typed(tokens.type_body).text_color(tokens.fg).child(title))
        .child(why)
}

impl Lanes {
    pub(super) fn lane(&self, column: FeatureColumn, features: &Features, lifted: Option<&Press>, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let main = !matches!(column, FeatureColumn::Held | FeatureColumn::Archived);
        let element = self.column(column, features, lifted, tokens, cx);
        if main {
            basis(element.flex_grow(1.0).flex_shrink(1.0).min_w(rem(tokens.fx_col_min)), rem(tokens.fx_col_basis)).into_any_element()
        } else {
            element.into_any_element()
        }
    }

    /// Held over Archived, narrower than a lane and as tall as its cards.
    pub(super) fn side(&self, features: &Features, lifted: Option<&Press>, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let stack = v_flex()
            .id("feature-side")
            .gap(rem(tokens.sp_12))
            .flex_grow_0()
            .flex_shrink(1.0)
            .min_w(rem(tokens.fx_col_min))
            .child(self.column(FeatureColumn::Held, features, lifted, tokens, cx))
            .child(self.column(FeatureColumn::Archived, features, lifted, tokens, cx));
        basis(stack, rem(tokens.fx_side_basis)).into_any_element()
    }

    fn column(&self, column: FeatureColumn, features: &Features, lifted: Option<&Press>, tokens: &Tokens, cx: &mut Context<Self>) -> Node {
        let cards: Vec<&FeatureRow> = features.rows.iter().filter(|row| row.group == column.word()).collect();
        let folded = column == FeatureColumn::Archived && !features.show_folded;
        let shown = if folded { features.folded } else { cards.len() };
        let carried = lifted.and_then(|press| features.rows.iter().find(|row| row.slug == press.slug));
        let target = carried.and_then(|row| index_words::target(row, self.over));
        let taking = target == Some(column);
        let side = !matches!(column, FeatureColumn::Queued | FeatureColumn::Active | FeatureColumn::Done);
        let section = v_flex()
            .id(SharedString::from(format!("feature-column-{}", column.word())))
            .test_support()
            .aria_label(SharedString::from(format!("{}, {shown} features", column.label())))
            .aria_expanded(!folded)
            .relative()
            .min_h_0()
            .gap(rem(tokens.sp_8))
            .p(rem(if folded { tokens.sp_2 } else { tokens.sp_8 }))
            .bg(tokens.bg_chrome)
            .border_1()
            .border_color(if taking { tokens.accent } else { tokens.line })
            .when(taking, |section| section.border_dashed())
            .rounded(rem(tokens.r_lg))
            .child(measured(self.boxes.clone(), column));
        if folded {
            return section.child(self.fold_row(shown, tokens, cx));
        }
        let ctx = self.ctx.clone();
        let body = section
            .when(!side, |section| section.overflow_y_scroll().track_scroll(self.scroll_of(column)))
            .child(head(column, shown, tokens))
            .children(taking.then(|| carried.map(|row| {
                div()
                    .id(SharedString::from(format!("dropzone-{}", column.word())))
                    .test_support()
                    .aria_label(SharedString::from(index_words::drop_banner(row, column)))
                    .p(rem(tokens.sp_8))
                    .rounded(rem(tokens.r_sm))
                    .bg(tokens.bg_selected)
                    .text_color(tokens.accent)
                    .text_size(rem(tokens.fs_sm))
                    .weighted(tokens.weight_semibold)
                    .text_center()
                    .child(index_words::drop_banner(row, column))
            })).flatten())
            .children(cards.iter().map(|row| self.card(row, lifted, tokens, cx)))
            .when(cards.is_empty(), |body| body.child(empty(column, tokens)))
            .when(column == FeatureColumn::Archived, |body| {
                body.child(
                    div()
                        .id("fold-archived")
                        .test_support()
                        .aria_label("Fold the archived")
                        .px(rem(tokens.sp_4))
                        .py(rem(tokens.sp_2))
                        .text_size(rem(tokens.fs_sm))
                        .text_color(tokens.fg_muted)
                        .cursor_pointer()
                        .on_click(move |_, _, cx| ctx.send(Call::FeatureAction { action: Action::Folded }, cx))
                        .child("Fold the archived"),
                )
            });
        body
    }

    /// A folded Archived: one row the width of a column, which still takes a dropped card.
    fn fold_row(&self, shown: usize, tokens: &Tokens, _: &mut Context<Self>) -> Node {
        let (ctx, fill) = (self.ctx.clone(), tokens.fill);
        h_flex()
            .id("unfold-archived")
            .test_support()
            .aria_label("Archived")
            .aria_expanded(false)
            .w_full()
            .h(rem(tokens.ck_fold_h))
            .px(rem(tokens.sp_8))
            .gap(rem(tokens.sp_8))
            .items_center()
            .rounded(rem(tokens.r_md))
            .typed(tokens.type_heading)
            .text_color(tokens.fg)
            .cursor_pointer()
            .hover(move |style| style.bg(fill))
            .on_click(move |_, _, cx| ctx.send(Call::FeatureAction { action: Action::Folded }, cx))
            .child(Glyph::Archive.at(tokens.icon_sm))
            .child("Archived")
            .child(count(shown.to_string(), tokens))
            .child(div().flex_1())
            .child(Glyph::Right.at(tokens.icon_sm))
    }

    /// A card wired to what a press on it does: a click peeks, a double click opens, a right click
    /// peeks and opens the Move menu where the pointer is, and a press arms a drag. The card being
    /// dragged is a placeholder saying where it is going.
    fn card(&self, row: &FeatureRow, lifted: Option<&Press>, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        if lifted.is_some_and(|press| press.slug == row.slug) {
            return placeholder(row, index_words::dragged(row, self.over), tokens).into_any_element();
        }
        let dress = if row.peeked { Dress::Peeked } else { Dress::Plain };
        let (slug, cards, anchor) = (row.slug.clone(), self.cards.clone(), self.anchor.clone());
        let (click, opened, pressed, asked) = (self.ctx.clone(), row.slug.clone(), row.slug.clone(), row.slug.clone());
        let (peek, menu, ctx) = (row.slug.clone(), self.menu.clone(), self.ctx.clone());
        let peeked = row.peeked;
        card(row, dress, tokens)
            .on_click(move |event, _, cx| {
                if event.click_count() >= 2 {
                    click.send(Call::OpenFeature { slug: Some(opened.clone()) }, cx);
                } else {
                    click.send(Call::Peek { slug: asked.clone() }, cx);
                }
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, _| {
                    // Measured now: a board pushed before the drag starts redraws the card.
                    let held = this.cards.borrow().iter().find(|(one, _)| *one == pressed).map(|(_, bounds)| *bounds);
                    if let Some(bounds) = held {
                        this.grab = Some(Grab { offset: event.position - bounds.origin, width: bounds.size.width });
                    }
                    this.drag.press(event.position, Press { slug: pressed.clone() });
                }),
            )
            .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                ctx.send(Call::Peek { slug: peek.clone() }, cx);
                menu.update(cx, |menu, cx| menu.open_move(&peek, event.position, false, window, cx));
            })
            .child(canvas(
                move |bounds, _, _| {
                    let mut held = cards.borrow_mut();
                    match held.iter_mut().find(|(one, _)| *one == slug) {
                        Some(entry) => entry.1 = bounds,
                        None => held.push((slug.clone(), bounds)),
                    }
                    if peeked {
                        anchor.set(bounds);
                    }
                },
                |_, _, _, _| {},
            ).absolute().size_full())
            .into_any_element()
    }
}

