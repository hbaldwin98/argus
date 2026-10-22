//! The bar over an agent's table: the filter box, how many rows it lets through, how many are
//! ticked, the agent's actions on the ticked rows, and Copy as TSV (TABLE-3 to TABLE-5).

use std::rc::Rc;

use argus_desktop_core::views::blocks::table::filter::{count_line, FILTERABLE};
use argus_desktop_core::views::blocks::table::pick::{actions_off, count_line as picked_line};
use argus_desktop_core::views::blocks::table::tsv::tsv;
use argus_desktop_core::views::values::cell_text;
use gpui_kit::component::input::Input;
use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, SharedString};

use super::Plan;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::views::actions::{action_button, PressWith};
use crate::views::blocks::pen::Pen;
use crate::views::ctx::ViewCtx;

/// `.vtable-bar`. `typed` is what the filter says.
pub fn bar(plan: &Plan, typed: &str, ctx: &ViewCtx, pen: &mut Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let t = plan.table;
    let key = format!("{}-bar", plan.key);
    let shown = plan.shown.iter().filter(|shown| **shown).count();

    let search = (t.rows.len() > FILTERABLE).then(|| {
        let (memory, path, repaint) = (pen.memory.clone(), plan.path.clone(), pen.repaint);
        let input = pen.field(&format!("{key}-filter"), "Filter rows", typed, move |text, cx| {
            memory.borrow_mut().set_filter(&path, text);
            cx.notify(repaint);
        });
        div().id(SharedString::from(format!("{key}-filter-box"))).test_support().min_w(rem(tokens.table_filter_min_w)).child(Input::new(&input).aria_label("Filter rows").h(rem(tokens.control_h_sm)))
    });

    let count = |id: &str, words: String| {
        div().id(SharedString::from(format!("{key}-{id}"))).test_support().aria_label(words.clone()).whitespace_nowrap().child(SharedString::from(words))
    };

    // The agent's actions press with the keys ticked, in the order the person sees, read at the press.
    let ordered: Vec<String> = plan.order.iter().filter_map(|r| t.key(*r).map(str::to_owned)).collect();
    let keys = t.row_keys.to_vec();
    let off = actions_off(plan.mode, plan.chosen.len());
    let buttons: Vec<AnyElement> = t
        .actions
        .iter()
        .map(|action| {
            let (memory, path, ordered, keys) = (pen.memory.clone(), plan.path.clone(), ordered.clone(), keys.clone());
            let with = Rc::new(move || {
                let chosen = memory.borrow_mut().selection(&path, &keys);
                PressWith { rows: ordered.iter().filter(|key| chosen.contains(*key)).cloned().collect(), ..PressWith::default() }
            });
            action_button(action, ctx, with, off, pen)
        })
        .collect();

    // A copy writes the order the person sees, and the rows the filter lets through.
    let titles: Vec<String> = t.columns.iter().map(|c| c.title.clone()).collect();
    let lines: Vec<Vec<String>> = plan
        .order
        .iter()
        .filter(|r| plan.shown[**r])
        .map(|r| t.rows[*r].iter().map(cell_text).collect())
        .collect();
    let copy = pen.copy_as(&format!("{key}|copy"), "Copy as TSV", &tsv(&titles, &lines));

    h_flex()
        .id(SharedString::from(key.clone()))
        .test_support()
        .flex_wrap()
        .items_center()
        .gap(rem(tokens.sp_8))
        .pt(rem(tokens.sp_4))
        .pb(rem(tokens.sp_8))
        .typed(tokens.type_small)
        .text_color(tokens.fg_muted)
        .children(search)
        .child(count("shown", count_line(shown, t.rows.len(), typed)))
        .child(count("picked", picked_line(plan.mode, plan.chosen.len())))
        .child(div().flex_1())
        .children(buttons)
        .child(copy)
        .into_any_element()
}
