//! The table: its head, the checkouts, the fold row and the branches, as one flat virtual list.
//!
//! The kit's table has no row that spans the columns, which the fold row is, so the rows are a
//! list of three kinds of line of two heights (`checkouts/partition` splits what the session sent;
//! its order is the protocol's). The selection is the session's: a click sends the row's key and
//! the highlight moves when the next table says so. The table follows its selection and keeps no
//! scroll position of its own: when the selected row or the rows change, the list scrolls to bring
//! the selected one into view with the least movement.

pub mod actions;
pub mod columns;
pub mod fold;
pub mod row;
pub mod state;

use std::cell::Cell;
use std::rc::Rc;

use argus_desktop_core::checkouts::partition::partition;
use argus_desktop_core::checkouts::table::Checkouts;
use gpui_kit::component::{h_flex, v_flex, v_virtual_list, VirtualListScrollHandle};
use gpui_kit::{canvas, div, prelude::*, px, size, Context, Entity, Pixels, ScrollStrategy, SharedString, Size, TestSupportExt as _, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::style::Typed as _;
use crate::theme::scale::{css, rem, zoom};
use crate::theme::Tokens;
use columns::widths;
use row::Setting;

/// What the rows send through and ask of.
#[derive(Clone)]
pub struct Ctx {
    pub heard: Entity<Heard>,
    pub caller: Caller,
}

/// One line of the list: a row of the table's `rows`, by its place there, or the fold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Line {
    Row(usize),
    Fold,
}

/// The lines in the order the table shows them: the checkouts, the fold when there are branches
/// without a checkout, then the branch rows.
fn lines(table: &Checkouts) -> Vec<Line> {
    let rows = table.rows.iter().enumerate();
    let (checkouts, branches): (Vec<_>, Vec<_>) = rows.partition(|(_, row)| row.checkout.is_some());
    checkouts.into_iter().map(|(i, _)| Line::Row(i)).chain((table.branches > 0).then_some(Line::Fold)).chain(branches.into_iter().map(|(i, _)| Line::Row(i))).collect()
}

pub struct Table {
    ctx: Ctx,
    scroll: VirtualListScrollHandle,
    /// The width the table was last laid out at, in real pixels, which says whether the Last
    /// commit column has room (`@container (max-width: 620px)`).
    width: Rc<Cell<Pixels>>,
    /// The selected row and how many lines there were when the list was last scrolled to it.
    followed: (Option<String>, usize),
}

impl Table {
    pub fn new(heard: Entity<Heard>, caller: Caller) -> Self {
        Self { ctx: Ctx { heard, caller }, scroll: VirtualListScrollHandle::new(), width: Rc::default(), followed: (None, 0) }
    }

    /// Scrolls to the selected line once per change of the selection or of the rows.
    fn follow(&mut self, table: &Checkouts, lines: &[Line]) {
        let now = (table.selected.clone(), lines.len());
        if now == self.followed {
            return;
        }
        let at = lines.iter().position(|line| matches!(line, Line::Row(i) if Some(&table.rows[*i].key) == table.selected.as_ref()));
        if let Some(at) = at {
            self.scroll.scroll_to_item(at, ScrollStrategy::Top);
        }
        self.followed = now;
    }
}

/// The heads of the columns: the page's `th`, small capitals in the faint colour under a hairline.
fn head(widths: columns::Widths, last: bool, tokens: &Tokens) -> impl IntoElement {
    let label = |words: &'static str| div().flex_shrink_0().px(rem(tokens.sp_12)).py(rem(tokens.sp_8)).typed(tokens.type_label).text_color(tokens.fg_faint).child(words);
    h_flex()
        .id("checkouts-head")
        .test_support()
        .flex_shrink_0()
        .w_full()
        .items_center()
        .border_b_1()
        .border_color(tokens.line)
        .child(label("CHECKOUT").w(rem(widths.name)))
        .child(label("STATE").w(rem(widths.state)))
        .child(if last { label("LAST COMMIT").flex_1().into_any_element() } else { div().flex_1().into_any_element() })
        .child(div().flex_shrink_0().w(rem(widths.actions)))
}

impl Render for Table {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let Some(table) = self.ctx.heard.read(cx).checkouts.clone() else { return div().into_any_element() };
        let listed = lines(&table);
        self.follow(&table, &listed);
        let parts = partition(&table);
        let all: Vec<_> = parts.checkouts.iter().chain(parts.branches.iter()).copied().collect();
        let widths = widths(&all, &tokens, &columns::shaped(window, &tokens));
        let hides = css(self.width.get(), window.rem_size()) > 0.0 && css(self.width.get(), window.rem_size()) <= tokens.ck_last_hides_below;
        let sizes: Rc<Vec<Size<Pixels>>> = Rc::new(
            listed.iter().map(|line| size(px(0.0), px(if *line == Line::Fold { tokens.ck_fold_h } else { tokens.ck_row_h } * zoom(window.rem_size())))).collect(),
        );
        let measured = self.width.clone();
        let this = cx.entity();
        let ctx = self.ctx.clone();
        let rows = table.clone();
        v_flex()
            .id("checkouts-table")
            .test_support()
            .aria_label(SharedString::from(format!("Checkouts in {}", table.repository.clone().unwrap_or_default())))
            .relative()
            .size_full()
            .min_h_0()
            .px(rem(tokens.sp_16))
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
            .child(head(widths, !hides, &tokens))
            .child(
                div().flex_1().min_h_0().relative().child(
                    v_virtual_list(cx.entity(), "checkouts-rows", sizes, move |_, range, _, cx| {
                        let tokens = *Tokens::global(cx);
                        let listed = lines(&rows);
                        range
                            .filter_map(|ix| listed.get(ix).copied())
                            .map(|line| match line {
                                Line::Fold => fold::fold(&partition(&rows).fold.expect("a fold line has a fold"), &ctx, &tokens).into_any_element(),
                                Line::Row(i) => {
                                    let row = &rows.rows[i];
                                    let setting = Setting { selected: rows.selected.as_deref() == Some(row.key.as_str()), widths, last: !hides };
                                    row::row(row, &setting, &ctx, &tokens, cx).into_any_element()
                                }
                            })
                            .collect::<Vec<_>>()
                    })
                    .track_scroll(&self.scroll),
                ),
            )
            .into_any_element()
    }
}

