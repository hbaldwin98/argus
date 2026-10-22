//! A grid block: as many blocks side by side as fit at a named minimum width, wrapping onto rows.
//! The agent names a width and the window owns the pixels (`sm`, `md`, `lg`).

use argus_desktop_core::views::blocks::flow::{columns, Fill};
use argus_protocol::{Block, GridMin};
use gpui_kit::{div, prelude::*, AnyElement};

use super::dispatch::blocks;
use super::pen::Pen;
use crate::theme::scale::rem;
use crate::theme::Tokens;
use crate::views::ctx::ViewCtx;

/// The narrowest an item of the grid may be.
pub fn min_width(min: GridMin, tokens: &Tokens) -> f32 {
    match min {
        GridMin::Sm => tokens.grid_min_sm,
        GridMin::Md => tokens.grid_min_md,
        GridMin::Lg => tokens.grid_min_lg,
    }
}

/// `.vgrid`: equal columns, as many as fit, the tracks kept when there are fewer items than fit.
pub fn grid(min: GridMin, items: &[Block], ctx: &ViewCtx, pen: &mut Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let key = format!("{}|grid", ctx.path);
    let n = columns(pen.width(&key), min_width(min, tokens), tokens.sp_12, items.len(), Fill::Fill);
    let cells: Vec<AnyElement> =
        items.iter().enumerate().map(|(k, item)| div().min_w_0().child(blocks(std::slice::from_ref(item), &ctx.at(k), pen)).into_any_element()).collect();
    div().relative().grid().grid_cols(n as u16).gap(rem(tokens.sp_12)).child(pen.watch(key)).children(cells).into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::themes::ledger;

    #[test]
    fn each_named_width_is_its_own_pixels() {
        let tokens = ledger::tokens();
        assert_eq!(
            [GridMin::Sm, GridMin::Md, GridMin::Lg].map(|min| min_width(min, &tokens)),
            [tokens.grid_min_sm, tokens.grid_min_md, tokens.grid_min_lg]
        );
        assert!(tokens.grid_min_sm < tokens.grid_min_md && tokens.grid_min_md < tokens.grid_min_lg);
    }
}
