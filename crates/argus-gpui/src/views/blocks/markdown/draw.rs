//! What each kind of block is drawn as: a heading no larger than a section's, a paragraph, a list
//! with its markers in a column of their own, a quote with an edge, a code block that scrolls
//! sideways, a rule and a table of cells.
//!
//! Leaves are numbered here in the order core's `leaves` numbers them (heading, paragraph, item
//! then its sub-items, quote contents, code, table head then rows), since a place in the document
//! is read against that numbering.

use argus_desktop_core::views::markdown::model::{Block, List, Table};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, Context, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, SharedString, StyledText};

use super::runs::{self, Face};
use super::Markdown;
use crate::kit::drag;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// How loud the text of a context is: a quote's is muted.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Ink {
    Plain,
    Muted,
}

impl Markdown {
    /// The whole document as an element, with the pointer and the keys it is selected and copied by.
    pub(super) fn body(&self, tokens: &Tokens, rem_size: Pixels, cx: &mut Context<Self>) -> AnyElement {
        let mut next = 0;
        let ink = if self.muted { Ink::Muted } else { Ink::Plain };
        let content = self.blocks_of(&self.blocks, &mut next, ink, tokens);
        let root = v_flex()
            .id(self.id.clone())
            .test_support()
            .relative()
            .min_w_0()
            .gap(rem(tokens.sp_8))
            .text_color(if self.muted { tokens.fg_muted } else { tokens.fg })
            .children(content)
            .when(self.hover.is_some(), |root| root.cursor_pointer())
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| this.moved(event, window, cx)))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, window, cx| this.pressed(event.position, window, cx)))
            .child(
                // Out of the flow, so the root's gap is not paid for what draws nothing: what the drag takes the
                // keys with, so Escape reaches it first, and what watches the pointer.
                div()
                    .absolute()
                    .size_0()
                    .child(
                        div()
                            .id(SharedString::from(format!("{}-keys", self.id)))
                            .size_0()
                            .track_focus(&self.keys)
                            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| drag::escape(this, event, window, cx))),
                    )
                    .child(drag::watch(cx.weak_entity())),
            )
            .children(self.address(rem_size, tokens));
        self.offer_copy(root, cx)
    }

    fn blocks_of(&self, blocks: &[Block], next: &mut usize, ink: Ink, tokens: &Tokens) -> Vec<AnyElement> {
        blocks.iter().map(|block| self.block(block, next, ink, tokens)).collect()
    }

    /// The text of the next leaf, in the face `ty` and `ink` give it.
    fn leaf(&self, next: &mut usize, weight: f32, ink: Ink, mono: bool, tokens: &Tokens) -> StyledText {
        let at = *next;
        *next += 1;
        let colour = if ink == Ink::Muted { tokens.fg_muted } else { tokens.fg };
        let styled = &self.leaves[at].styled;
        let lit = self.hover.filter(|hover| hover.leaf == at).map(|hover| hover.link);
        runs::text(at, styled, self.selected_in(at), lit, Face { colour, weight, mono }, tokens, &self.layouts)
    }

    fn block(&self, block: &Block, next: &mut usize, ink: Ink, tokens: &Tokens) -> AnyElement {
        match block {
            // A view is not a document: a heading is a section's, and below it a label.
            Block::Heading { level: 3, .. } => {
                let ty = tokens.type_heading;
                div().typed(ty).child(self.leaf(next, ty.weight, ink, false, tokens)).into_any_element()
            }
            Block::Heading { .. } => {
                let ty = tokens.type_label;
                div().typed(ty).text_color(tokens.fg_muted).child(self.leaf(next, ty.weight, Ink::Muted, false, tokens)).into_any_element()
            }
            Block::Paragraph(_) => div().child(self.leaf(next, tokens.weight_regular, ink, false, tokens)).into_any_element(),
            Block::List(list) => self.list(list, 0, next, ink, tokens),
            Block::Quote(inner) => v_flex()
                .gap(rem(tokens.sp_8))
                .pl(rem(tokens.sp_12))
                .border_l(rem(tokens.rule))
                .border_color(tokens.line_strong)
                .text_color(tokens.fg_muted)
                .children(self.blocks_of(inner, next, Ink::Muted, tokens))
                .into_any_element(),
            Block::Code(_) => {
                let at = *next;
                div()
                    .id(SharedString::from(format!("{}-code-{at}", self.id)))
                    .overflow_x_scroll()
                    .px(rem(tokens.sp_12))
                    .py(rem(tokens.sp_8))
                    .rounded(rem(tokens.r_sm))
                    .bg(tokens.bg_sunken)
                    .font_family(tokens.font_mono)
                    .text_size(rem(tokens.fs_sm))
                    .whitespace_nowrap()
                    .child(self.leaf(next, tokens.weight_regular, ink, true, tokens))
                    .into_any_element()
            }
            Block::Rule => div().my(rem(tokens.sp_4)).h(rem(tokens.hairline)).w_full().bg(tokens.line).into_any_element(),
            Block::Table(table) => self.table(table, next, ink, tokens),
        }
    }

    fn list(&self, list: &List, depth: usize, next: &mut usize, ink: Ink, tokens: &Tokens) -> AnyElement {
        let rows = list.items.iter().enumerate().map(|(k, item)| {
            let marker = if list.ordered { format!("{}.", list.start as usize + k) } else if depth == 0 { "\u{2022}".to_owned() } else { "\u{25e6}".to_owned() };
            let text = self.leaf(next, tokens.weight_regular, ink, false, tokens);
            let sub = item.sub.as_ref().map(|sub| div().mt(rem(tokens.sp_2)).child(self.list(sub, depth + 1, next, ink, tokens)));
            h_flex()
                .items_start()
                .child(div().w(rem(tokens.sp_20)).flex_shrink_0().pr(rem(tokens.sp_6)).flex().justify_end().child(marker))
                .child(v_flex().min_w_0().flex_1().child(text).children(sub))
        });
        v_flex().gap(rem(tokens.sp_2)).children(rows).into_any_element()
    }

    /// A table as wide as its cells are: each column is as wide as its widest cell, and the table is
    /// no wider than that however wide the view, scrolling sideways when the view is narrower
    /// (`.md-table`, `overflow-x: auto`). The cells are numbered a row at a time, as core numbers them,
    /// and laid out a column at a time, so a column's width is its widest cell's.
    fn table(&self, table: &Table, next: &mut usize, ink: Ink, tokens: &Tokens) -> AnyElement {
        let at = *next;
        let mut cells = |count: usize, head: bool| -> Vec<AnyElement> {
            (0..count)
                .map(|_| {
                    let weight = if head { tokens.weight_semibold } else { tokens.weight_regular };
                    let ink = if head { Ink::Muted } else { ink };
                    div()
                        .px(rem(tokens.sp_12))
                        .py(rem(tokens.sp_4))
                        .border_b(rem(tokens.hairline))
                        .border_color(tokens.line)
                        .when(head, |cell| cell.bg(tokens.bg_chrome))
                        .whitespace_nowrap()
                        .child(self.leaf(next, weight, ink, false, tokens))
                        .into_any_element()
                })
                .collect()
        };
        let mut rows = vec![cells(table.head.len(), true)];
        rows.extend(table.rows.iter().map(|row| cells(row.len(), false)));
        let width = rows.iter().map(Vec::len).max().unwrap_or(0);
        let mut columns: Vec<Vec<AnyElement>> = (0..width).map(|_| Vec::new()).collect();
        for row in rows {
            for (column, cell) in row.into_iter().enumerate() {
                columns[column].push(cell);
            }
        }
        let grid = h_flex().typed(tokens.type_small).children(columns.into_iter().map(|column| v_flex().flex_none().children(column)));
        div().id(SharedString::from(format!("{}-table-{at}", self.id))).overflow_x_scroll().child(grid).into_any_element()
    }
}
