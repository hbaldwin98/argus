//! An extension's view as a table: the agent table's columns and rows with a row's status leading,
//! the columns that give way as the box narrows, the sort a header press makes, and the body the
//! keys of the view are given to.
//!
//! What the table holds is core's (`extensions/echo` shapes it, `views::blocks::table` sorts it and
//! says which columns give way); this puts it in order and draws it: `head` is the header and `rows`
//! the rows. The sort is local and goes when the view is published again (EXTVIEW-32), as the page's
//! does when it redraws.

pub mod head;
pub mod rows;

use argus_desktop_core::extensions::echo::{shape, Shaped};
use argus_desktop_core::extensions::views::ExtTable;
use argus_desktop_core::views::blocks::table::columns::{drop_rank, shown};
use argus_desktop_core::views::blocks::table::sort::{order, Direction};
use argus_desktop_core::views::blocks::table::width;
use argus_protocol::Value;
use gpui_kit::component::v_flex;
use gpui_kit::{div, prelude::*, AnyElement, Context, SharedString};

use super::ExtensionView;
use crate::theme::Tokens;

/// What the parts of the table are drawn from, decided before anything is.
pub struct Plan<'a> {
    pub view: &'a ExtTable,
    pub shaped: Shaped,
    /// The rows in the order shown, by their place in `shaped`.
    pub order: Vec<usize>,
    /// Whether each column is drawn at the width the table has.
    pub visible: Vec<bool>,
    pub widths: Vec<f32>,
    /// The box's width in the page's pixels; zero before it is measured.
    pub across: f32,
    pub sort: Option<(usize, bool)>,
}

impl<'a> Plan<'a> {
    pub fn of(view: &'a ExtTable, sort: Option<(usize, bool)>, across: f32) -> Self {
        let shaped = shape(view);
        let refs: Vec<&[Value]> = shaped.rows.iter().map(Vec::as_slice).collect();
        let order = match sort {
            Some((c, ascending)) => order(&refs, c, if ascending { Direction::Ascending } else { Direction::Descending }),
            None => (0..refs.len()).collect(),
        };
        let titles: Vec<&str> = shaped.columns.iter().map(|column| column.title.as_str()).collect();
        let count = shaped.columns.len();
        let visible = shaped.columns.iter().enumerate().map(|(c, column)| shown(column.optional, drop_rank(c, count), across)).collect();
        let widths = width::columns(&titles, &refs);
        Self { view, shaped, order, visible, widths, across, sort }
    }
}

impl ExtensionView {
    /// The table drawn: its header over its rows, scrolling sideways when it is wider than the box.
    pub(super) fn grid(&self, view: &ExtTable, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let key = format!("ext-{}", view.panel);
        let plan = Plan::of(view, self.sort, self.widths.get(&key));
        let head = self.grid_head(&plan, tokens, cx);
        let rows = self.grid_rows(&plan, tokens, cx);
        v_flex()
            .id(SharedString::from(format!("{key}-table")))
            .relative()
            .min_w_0()
            .child(
                div()
                    .id(SharedString::from(format!("{key}-scroll")))
                    .overflow_x_scroll()
                    .child(v_flex().min_w_full().flex_shrink_0().child(head).child(rows)),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests;
