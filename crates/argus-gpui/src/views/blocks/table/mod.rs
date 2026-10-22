//! A table block: the agent's rows as a table that scrolls sideways when narrow, re-sorts when a
//! header is pressed, and, for an agent's table, picks rows, offers actions on them, opens a row to
//! blocks of its own, filters and copies itself as TSV (TABLE-1 to TABLE-7).
//!
//! Every rule (what a cell sorts by, which rows a filter lets through, what select-all shows, how
//! wide a column is, which columns give way) is core's `views::blocks::table`. This module only
//! gathers what the parts need into a [`Plan`] and puts them in order: the bar over the table
//! (`bar`), its header (`head`) and its rows (`body`). The rows ticked, the rows opened and the
//! filter typed are `views/memory`'s, so they outlast the agent showing the view again; the sort is
//! `kept`'s and does not.
//!
//! The extension tables of the page, which add a key per row, a selected one and hover actions
//! (`RowPicks`), are a later item's; this is the agent's table.

pub mod bar;
pub mod body;
pub mod head;

use std::collections::BTreeSet;

use argus_desktop_core::views::blocks::table::columns::shown as column_shown;
use argus_desktop_core::views::blocks::table::filter::{matches, row_text, showing, FILTERABLE};
use argus_desktop_core::views::blocks::table::pick::{mode as pick_mode, row_ids};
use argus_desktop_core::views::blocks::table::sort::{order, Direction};
use argus_desktop_core::views::blocks::table::width;
use argus_protocol::{Block, Column, RowSelect, Value, ViewAction};
use gpui_kit::component::v_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, SharedString};

use super::pen::Pen;
use crate::kit::wheel::Wheel;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::views::ctx::ViewCtx;

/// What a table block says.
pub struct Table<'a> {
    pub columns: &'a [Column],
    pub rows: &'a [Vec<Value>],
    pub sorted_by: Option<usize>,
    pub total: Option<u64>,
    pub row_keys: &'a [String],
    pub select: RowSelect,
    pub actions: &'a [ViewAction],
    pub details: &'a [Option<Vec<Block>>],
}

impl Table<'_> {
    /// The agent's key for row `r`, when it gave one for every row.
    pub fn key(&self, r: usize) -> Option<&str> {
        (self.row_keys.len() == self.rows.len()).then(|| self.row_keys.get(r).map(String::as_str)).flatten()
    }

    /// Whether row `r` opens to blocks of its own.
    pub fn openable(&self, r: usize) -> bool {
        self.details.get(r).and_then(Option::as_ref).is_some_and(|blocks| !blocks.is_empty())
    }
}

/// What the parts of a table are drawn from: the table, where it is, and what is decided about it
/// before anything is drawn.
pub struct Plan<'a> {
    pub table: &'a Table<'a>,
    pub path: String,
    /// What the table's parts are named under: `<path>-table`.
    pub key: String,
    pub mode: RowSelect,
    /// The rows in the order they are shown, by their place in the agent's.
    pub order: Vec<usize>,
    /// Whether the filter lets each row through, by the agent's place.
    pub shown: Vec<bool>,
    pub chosen: BTreeSet<String>,
    pub open: BTreeSet<String>,
    /// What each row's open state is remembered by.
    pub ids: Vec<String>,
    /// How wide each column is taken to be, and whether it is drawn at the width the table has.
    pub widths: Vec<f32>,
    pub visible: Vec<bool>,
    pub has_details: bool,
    /// The column the person sorted and whether it runs ascending; none until they did.
    pub sort: Option<(usize, bool)>,
}

impl Plan<'_> {
    /// The column that wears the arrow: the person's, else the agent's.
    pub fn marked(&self) -> Option<usize> {
        self.sort.map(|(c, _)| c).or(self.table.sorted_by)
    }

    /// The keys of the rows the filter shows that have one, in the order shown.
    pub fn shown_keys(&self) -> Vec<&str> {
        self.order.iter().filter(|r| self.shown[**r]).filter_map(|r| self.table.key(*r)).collect()
    }
}

pub fn table(t: Table, ctx: &ViewCtx, pen: &mut Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let key = format!("{}-table", ctx.path);
    if t.rows.is_empty() {
        return div().id(SharedString::from(key)).test_support().aria_label("No rows").p(rem(tokens.sp_8)).text_color(tokens.fg_muted).typed(tokens.type_small).child("No rows").into_any_element();
    }
    let path = ctx.path.clone();
    let mode = pick_mode(t.select, t.row_keys.len(), t.rows.len());
    let ids = row_ids(t.row_keys, t.rows.len());
    let has_details = (0..t.rows.len()).any(|r| t.openable(r));
    let (chosen, open, typed) = {
        let mut memory = pen.memory.borrow_mut();
        let chosen = if mode == RowSelect::None { BTreeSet::new() } else { memory.selection(&path, t.row_keys) };
        let open = if has_details { memory.open_rows(&path, &ids) } else { BTreeSet::new() };
        // The page keeps the filter's text only where the table has a filter to type it in.
        (chosen, open, if t.rows.len() > FILTERABLE { memory.filter(&path).to_owned() } else { String::new() })
    };
    let refs: Vec<&[Value]> = t.rows.iter().map(Vec::as_slice).collect();
    let sort = pen.kept.sorts.get(&path);
    let rows_in_order = match sort {
        Some((c, ascending)) => order(&refs, c, if ascending { Direction::Ascending } else { Direction::Descending }),
        None => (0..t.rows.len()).collect(),
    };
    let shown = refs.iter().map(|cells| matches(&row_text(cells), &typed)).collect();
    let titles: Vec<&str> = t.columns.iter().map(|c| c.title.as_str()).collect();
    let across = pen.width(&key);
    let plan = Plan {
        table: &t,
        path,
        key: key.clone(),
        mode,
        order: rows_in_order,
        shown,
        chosen,
        open,
        ids,
        widths: width::columns(&titles, &refs),
        visible: t.columns.iter().map(|c| column_shown(c.optional, 0, across)).collect(),
        has_details,
        sort,
    };

    let bar = bar::bar(&plan, &typed, ctx, pen);
    let head = head::head(&plan, pen);
    let body = body::rows(&plan, ctx, pen);
    let scroll = pen.scroll(&format!("{key}-scroll"));
    let wheel = Wheel::new(&scroll);
    let more = showing(t.rows.len(), t.total);
    v_flex()
        .id(SharedString::from(key.clone()))
        .test_support()
        .relative()
        .min_w_0()
        .child(pen.watch(key.clone()))
        .child(bar)
        .child(
            div()
                .id(SharedString::from(format!("{key}-scroll")))
                .overflow_x_scroll()
                .track_scroll(&scroll)
                .on_scroll_wheel(wheel.keep())
                .child(wheel.probe())
                .child(v_flex().min_w_full().flex_shrink_0().child(head).child(body)),
        )
        .children(more.map(|more| {
            div().id(SharedString::from(format!("{key}-more"))).test_support().aria_label(more.clone()).p(rem(tokens.sp_8)).text_color(tokens.fg_muted).typed(tokens.type_small).child(more)
        }))
        .into_any_element()
}
