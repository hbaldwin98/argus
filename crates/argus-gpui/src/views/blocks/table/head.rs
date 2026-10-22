//! The header of an agent's table: a select-all box over the ticks, a blank over the chevrons, and
//! each column's title as a button that sorts by it (TABLE-2, TABLE-4).
//!
//! Also the one place a column's width becomes a flex track, so a header cell and every cell under
//! it are the same width: a basis of its estimated width, never shrinking, and a share of what is
//! left in proportion to it, as an auto-sized table shares its room.

use argus_desktop_core::views::blocks::table::pick::{all_state, toggle_all, Tri};
use argus_desktop_core::views::blocks::table::sort::click;
use argus_protocol::RowSelect;
use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, Div, Role, SharedString};

use super::Plan;
use crate::kit::mixed::{mixed_box, Mark};
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;
use crate::views::blocks::pen::Pen;

/// The width of the cell that holds a tick or a chevron: the control, and the room round it.
pub fn lead_w(tokens: &Tokens) -> f32 {
    tokens.icon_sm + (tokens.sp_8 + tokens.sp_4) * 2.0
}

/// A cell `width` wide at least that takes its share of any room the row has over.
pub fn track(width: f32) -> Div {
    let mut cell = div().flex().items_center().min_w_0().flex_shrink_0().flex_basis(rem(width));
    cell.style().flex_grow = Some(width / 100.0);
    cell
}

/// The empty cell of a lead column.
pub fn lead(tokens: &Tokens) -> Div {
    div().flex().items_center().justify_center().flex_shrink_0().w(rem(lead_w(tokens)))
}

pub fn head(plan: &Plan, pen: &mut Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let t = plan.table;
    let key = format!("{}-head", plan.key);
    let (marked, ascending) = (plan.marked(), plan.sort.is_some_and(|(_, up)| up));

    let pick = (plan.mode != RowSelect::None).then(|| {
        let cell = lead(tokens);
        if plan.mode == RowSelect::One {
            return cell;
        }
        let shown = plan.shown_keys();
        let state = all_state(&shown, &plan.chosen);
        let (memory, path, repaint) = (pen.memory.clone(), plan.path.clone(), pen.repaint);
        let (chosen, every) = (plan.chosen.clone(), shown.iter().map(|key| (*key).to_owned()).collect::<Vec<_>>());
        let mark = match state {
            Tri::Clear => Mark::Clear,
            Tri::Mixed => Mark::Mixed,
            Tri::All => Mark::Checked,
        };
        cell.child(mixed_box(SharedString::from(format!("{key}-all")), "Select all rows", mark, tokens).on_click(move |_, _, cx| {
            // A click on a mixed box ticks, as the browser's does: only a full one clears.
            let every: Vec<&str> = every.iter().map(String::as_str).collect();
            memory.borrow_mut().set_selection(&path, toggle_all(&chosen, &every, state != Tri::All));
            cx.notify(repaint);
        }))
    });

    let cells: Vec<AnyElement> = t
        .columns
        .iter()
        .enumerate()
        .filter(|(c, _)| plan.visible[*c])
        .map(|(c, column)| {
            let on = marked == Some(c);
            let arrow = if !on { "" } else if plan.sort.is_some() && ascending { "\u{2191}" } else { "\u{2193}" };
            let (next, direction) = click(marked, plan.sort.is_some() && ascending, c);
            let (sorts, path, repaint) = (pen.kept.sorts.clone(), plan.path.clone(), pen.repaint);
            let (fg, fg_hot) = (tokens.fg_faint, tokens.fg);
            track(plan.widths[c])
                .child(
                    h_flex()
                        .id(SharedString::from(format!("{key}-sort{c}")))
                        .role(Role::Button)
                        .test_support()
                        .aria_label(column.title.clone())
                        .w_full()
                        .gap(rem(tokens.sp_4))
                        .px(rem(tokens.sp_8))
                        .py(rem(tokens.sp_6))
                        .typed(tokens.type_label)
                        .text_color(if on { fg_hot } else { fg })
                        .cursor_pointer()
                        .whitespace_nowrap()
                        .when(column.numeric, |button| button.justify_end())
                        .hover(move |style| style.text_color(fg_hot))
                        .on_click(move |_, _, cx| {
                            sorts.set(&path, next, direction == argus_desktop_core::views::blocks::table::sort::Direction::Ascending);
                            cx.notify(repaint);
                        })
                        .child(SharedString::from(column.title.to_uppercase()))
                        .child(div().min_w(rem(tokens.sp_8)).child(arrow)),
                )
                .into_any_element()
        })
        .collect();

    h_flex()
        .id(SharedString::from(key))
        .border_b_1()
        .border_color(tokens.line)
        .children(pick)
        .children(plan.has_details.then(|| lead(tokens)))
        .children(cells)
        .into_any_element()
}
