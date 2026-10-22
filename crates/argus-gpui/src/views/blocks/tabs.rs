//! A tabs block: a bar of tabs over the blocks of the one chosen. Which tab is chosen is the
//! window's alone (`views/memory`, by the block's path) and outlives a redraw: a view that comes
//! again keeps the tab being read, held to the tabs there are now.

use argus_protocol::Tab;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, SharedString};

use super::dispatch::blocks;
use super::pen::Pen;
use crate::kit::strip::{tab as tab_button, title};
use crate::theme::scale::rem;
use crate::views::ctx::ViewCtx;

/// `.vtabs`: the bar is `.lens`, the one tab chip of the page (`kit/strip`), over a hairline.
pub fn tabs(all: &[Tab], ctx: &ViewCtx, pen: &mut Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let chosen = pen.memory.borrow().tab(&ctx.path, all.len());
    let bar = h_flex().gap(rem(tokens.sp_4)).pb(rem(tokens.sp_8)).border_b_1().border_color(tokens.line).children(all.iter().enumerate().map(|(k, tab)| {
        let (memory, path, repaint) = (pen.memory.clone(), ctx.path.clone(), pen.repaint);
        tab_button(SharedString::from(format!("{}-tab{k}", ctx.path)), k == chosen, false, tab.title.clone(), tokens).child(title(tab.title.clone())).on_click(move |_, _, cx| {
            memory.borrow_mut().choose_tab(&path, k);
            cx.notify(repaint);
        })
    }));
    let body = all.get(chosen).map(|tab| blocks(&tab.blocks, &ctx.at(chosen), pen));
    v_flex().gap(rem(tokens.sp_12)).child(bar).child(div().children(body)).into_any_element()
}
