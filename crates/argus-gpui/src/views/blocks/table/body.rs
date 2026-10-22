//! The rows of an agent's table: a tick or a radio, a chevron that opens the row to blocks of its
//! own, the cells, and the opened blocks in a full-width row under it (TABLE-4, TABLE-6, TABLE-7).
//!
//! A row the filter hides is not drawn, nor is its opened detail. A long text cell is cut with its
//! whole text for a tooltip.

use argus_desktop_core::views::blocks::table::width::LONG;
use argus_protocol::{RowSelect, Value};
use gpui_kit::component::radio::Radio;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, percentage, prelude::*, AnyElement, Role, SharedString, Transformation};

use super::head::{lead, track};
use super::Plan;
use crate::kit::icons::Glyph;
use crate::kit::mixed::{mixed_box, Mark};
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::views::blocks::dispatch::blocks;
use crate::views::blocks::pen::Pen;
use crate::views::ctx::ViewCtx;
use crate::views::values::value;

pub fn rows(plan: &Plan, ctx: &ViewCtx, pen: &mut Pen) -> AnyElement {
    let drawn: Vec<AnyElement> = plan.order.iter().filter(|r| plan.shown[**r]).map(|r| row(plan, *r, ctx, pen)).collect();
    v_flex().children(drawn).into_any_element()
}

fn row(plan: &Plan, r: usize, ctx: &ViewCtx, pen: &mut Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let t = plan.table;
    let key = format!("{}-row{r}", plan.key);
    let fill = tokens.fill;

    let pick = match (plan.mode, t.key(r)) {
        (RowSelect::None, _) | (_, None) => None,
        (mode, Some(row_key)) => {
            let on = plan.chosen.contains(row_key);
            let (memory, path, repaint, row_key) = (pen.memory.clone(), plan.path.clone(), pen.repaint, row_key.to_owned());
            let id = SharedString::from(format!("{key}-pick"));
            Some(lead(tokens).child(if mode == RowSelect::Many {
                mixed_box(id, "Select row", if on { Mark::Checked } else { Mark::Clear }, tokens)
                    .on_click(move |_, _, cx| {
                        memory.borrow_mut().toggle_selected(&path, &row_key);
                        cx.notify(repaint);
                    })
                    .into_any_element()
            } else {
                Radio::new(id)
                    .accessibility_label("Select row")
                    .checked(on)
                    .on_change(move |_, _, cx| {
                        memory.borrow_mut().set_selection(&path, [row_key.clone()]);
                        cx.notify(repaint);
                    })
                    .into_any_element()
            }))
        }
    };

    let id = &plan.ids[r];
    let open = plan.open.contains(id);
    let chevron = plan.has_details.then(|| {
        let cell = lead(tokens);
        if !t.openable(r) {
            return cell;
        }
        let (memory, path, repaint, id) = (pen.memory.clone(), plan.path.clone(), pen.repaint, id.clone());
        let glyph = Glyph::Right.at(tokens.icon_sm);
        cell.child(
            div()
                .id(SharedString::from(format!("{key}-chevron")))
                .role(Role::Button)
                .test_support()
                .aria_label("Show row details")
                .p(rem(tokens.sp_4))
                .flex()
                .text_color(tokens.fg_muted)
                .cursor_pointer()
                .on_click(move |_, _, cx| {
                    memory.borrow_mut().toggle_row(&path, &id);
                    cx.notify(repaint);
                })
                .child(if open { glyph.transform(Transformation::rotate(percentage(0.25))) } else { glyph }),
        )
    });

    let cells: Vec<AnyElement> = t
        .columns
        .iter()
        .enumerate()
        .filter(|(c, _)| plan.visible[*c])
        .map(|(c, column)| {
            let cell = t.rows[r].get(c);
            let at = format!("{key}-c{c}");
            let long = cell.and_then(|cell| match cell {
                Value::Text { text } if text.chars().count() > LONG => Some(text.clone()),
                _ => None,
            });
            let inner = cell.map(|cell| value(cell, &at, &pen.at(ctx)));
            let body = div().id(SharedString::from(at)).min_w_0().px(rem(tokens.sp_8)).when(column.numeric, |cell| cell.flex().justify_end().w_full()).children(inner);
            track(plan.widths[c])
                .overflow_hidden()
                .whitespace_nowrap()
                .child(match long {
                    Some(text) => body.truncate().tooltip(tip(text)).into_any_element(),
                    None => body.into_any_element(),
                })
                .into_any_element()
        })
        .collect();

    let line = h_flex()
        .id(SharedString::from(key.clone()))
        .test_support()
        .h(rem(tokens.table_row_h))
        .border_b_1()
        .border_color(tokens.line)
        .hover(move |style| style.bg(fill))
        .children(pick)
        .children(chevron)
        .children(cells);

    let detail = (open && t.openable(r)).then(|| {
        let inner = t.details[r].as_deref().unwrap_or_default();
        div()
            .id(SharedString::from(format!("{key}-detail")))
            .test_support()
            .px(rem(tokens.sp_12))
            .py(rem(tokens.sp_8))
            .bg(tokens.bg_sunken)
            .border_b_1()
            .border_color(tokens.line)
            .whitespace_normal()
            .typed(tokens.type_body)
            .child(blocks(inner, &ctx.at(format!("d{r}")), pen))
    });
    v_flex().child(line).children(detail).into_any_element()
}
