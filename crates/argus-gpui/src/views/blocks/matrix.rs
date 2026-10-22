//! A matrix block: a grid of cells tinted by tone and weight (core's `blocks::matrix` says how
//! strongly), under its column names and beside its row names, scrolling sideways when wide.

use argus_desktop_core::views::blocks::matrix::{share, title};
use argus_protocol::MatrixCell;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, SharedString};

use super::pen::Pen;
use crate::kit::tip::tip;
use crate::kit::wheel::Wheel;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::views::chip::tone_colour;
use crate::views::ctx::ViewCtx;

pub fn matrix(rows: &[String], columns: &[String], cells: &[Vec<MatrixCell>], ctx: &ViewCtx, pen: &mut Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let key = format!("{}-matrix", ctx.path);
    let scroll = pen.scroll(&key);
    let wheel = Wheel::new(&scroll);
    let head = |text: &str, right: bool| {
        div()
            .px(rem(tokens.sp_8))
            .py(rem(tokens.sp_4))
            .text_size(rem(tokens.fs_sm))
            .weighted(tokens.weight_semibold)
            .text_color(tokens.fg_muted)
            .whitespace_nowrap()
            .when(right, |head| head.text_right())
            .when(!right, |head| head.text_center())
            .child(SharedString::from(text.to_owned()))
            .into_any_element()
    };
    let mut grid: Vec<AnyElement> = vec![div().into_any_element()];
    grid.extend(columns.iter().map(|column| head(column, false)));
    for (r, row) in rows.iter().enumerate() {
        grid.push(head(row, true));
        for (c, column) in columns.iter().enumerate() {
            let cell = cells.get(r).and_then(|line| line.get(c));
            let base = div().min_w(rem(tokens.matrix_cell_w)).h(rem(tokens.matrix_cell_h)).px(rem(tokens.sp_8)).flex().items_center().justify_center().rounded(rem(tokens.r_xs));
            grid.push(match cell {
                None => base.into_any_element(),
                Some(cell) => {
                    let label = cell.label.as_deref().unwrap_or_default();
                    let said = title(row, column, cell.label.as_deref());
                    base.id(SharedString::from(format!("{key}-{r}-{c}")))
                        .test_support()
                        .aria_label(said.clone())
                        .bg(tokens.fill.blend(tone_colour(cell.tone, tokens).opacity(share(cell.weight))))
                        .text_color(tokens.fg)
                        .text_size(rem(tokens.fs_sm))
                        .tooltip(tip(said))
                        .child(SharedString::from(label.to_owned()))
                        .into_any_element()
                }
            });
        }
    }
    div()
        .id(SharedString::from(key))
        .overflow_x_scroll()
        .track_scroll(&scroll)
        .on_scroll_wheel(wheel.keep())
        .child(wheel.probe())
        .child(div().grid().grid_cols_max_content(columns.len() as u16 + 1).gap(rem(tokens.matrix_gap)).children(grid))
        .into_any_element()
}
