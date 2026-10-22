//! A view's blocks, each handed to the module that draws its kind by the path of the block under
//! the view (`pane/id.2.0`), which is what keys everything the window remembers about it.
//!
//! The page's `views/blocks.ts`. Only dispatch is here: the layout blocks that hold blocks are
//! `section`, `columns`, `tabs`, `card`, `grid` and `details`, each in its own module, and every
//! block drawn through `blocks` is one more recursion into this.

use argus_protocol::Block;
use gpui_kit::component::v_flex;
use gpui_kit::{prelude::*, AnyElement};

use super::pen::Pen;
use super::pending::unknown;
use super::{board, callout, card, changed, chart, checklist, code, columns, details, diagram, embed, fields, form, graph, grid, list, log, matrix, patch, row, section, stats, table, tabs, timeline, tree};
use crate::theme::scale::rem;
use crate::views::ctx::ViewCtx;
use super::progress::progress_block;

/// `.vblocks`: the blocks of a container one under another, 16px apart. `ctx` is the container's;
/// the block at `i` is drawn at its path `.i`.
pub fn blocks(all: &[Block], ctx: &ViewCtx, pen: &mut Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let drawn: Vec<AnyElement> = all.iter().enumerate().map(|(k, block)| one(block, &ctx.at(k), pen)).collect();
    v_flex().min_w_0().gap(rem(tokens.sp_16)).children(drawn).into_any_element()
}

/// One block, washed when it is among those that changed since the person saw them (BLOCKS-2).
fn one(block: &Block, ctx: &ViewCtx, pen: &mut Pen) -> AnyElement {
    let drawn = draw(block, ctx, pen);
    changed::marked(drawn, ctx, pen.draw.tokens, pen.window, pen.cx)
}

fn draw(block: &Block, ctx: &ViewCtx, pen: &mut Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let key = ctx.path.clone();
    match block {
        Block::Section { heading, blocks, opens } => section::section(heading, opens.as_deref(), blocks, ctx, pen),
        Block::Columns { columns } => columns::columns(columns, ctx, pen),
        Block::Tabs { tabs } => tabs::tabs(tabs, ctx, pen),
        Block::Markdown { text } => pen.markdown(&format!("{key}|md"), text, false),
        Block::Fields { fields } => fields::fields(fields, &key, &pen.at(ctx)),
        Block::List { items } => list::list(items, ctx, pen),
        Block::Stats { stats } => stats::stats(stats, ctx, pen),
        Block::Code { lang, text } => code::code(lang.as_deref(), text, &key, pen),
        Block::Diagram { source } => diagram::diagram(source, ctx, pen),
        Block::Chart { shape, x, series, unit } => chart::chart(*shape, x, series, unit.as_deref(), ctx, pen),
        Block::Graph { nodes, edges, direction } => graph::graph(nodes, edges, *direction, ctx, pen),
        Block::Callout { tone, text } => callout::callout(*tone, text, &key, tokens),
        Block::Card { title, chip, tone, blocks, actions, opens } => {
            card::card(card::Card { title: title.as_deref(), chip: chip.as_ref(), tone: *tone, blocks, actions, opens: opens.as_deref() }, ctx, pen)
        }
        Block::Grid { min, items } => grid::grid(*min, items, ctx, pen),
        Block::Details { summary, chip, open, blocks, opens } => {
            details::details(details::Details { summary, chip: chip.as_ref(), open: *open, opens: opens.as_deref(), blocks }, ctx, pen)
        }
        Block::Row { align, items } => row::row(*align, items, ctx, pen),
        Block::Embed { view } => embed::embed(view, ctx, pen),
        Block::Divider => section::divider(tokens),
        Block::Progress { label, value, max, tone, note } => progress_block(label, *value, *max, *tone, note.as_deref(), &key, tokens).into_any_element(),
        Block::Timeline { events } => timeline::timeline(events, ctx, pen),
        Block::Tree { nodes } => tree::tree(nodes, ctx, pen),
        Block::Matrix { rows, columns, cells } => matrix::matrix(rows, columns, cells, ctx, pen),
        Block::Checklist { items } => checklist::checklist(items, ctx, pen),
        Block::Log { lines } => log::log(lines, ctx, pen),
        Block::Board { columns } => board::board(columns, ctx, pen),
        Block::Table { columns, rows, sorted_by, total, row_keys, select, actions, details } => table::table(
            table::Table { columns, rows, sorted_by: *sorted_by, total: *total, row_keys, select: *select, actions, details },
            ctx,
            pen,
        ),
        Block::Diff { path, patch } => patch::patch(path.as_deref(), patch, ctx, pen),
        Block::Form { id, fields, submit } => form::form(id, fields, submit, ctx, pen),
        Block::Unknown => unknown(tokens),
    }
}

