//! The agent's table block drawn and pressed: sorting by a header, the filter, ticking rows and
//! pressing the agent's actions on them, opening a row to blocks, Copy as TSV, and the columns that
//! give way when narrow. Each rule is cited by its parity id (TABLE-n).

use argus_desktop_core::connection::page::Call;
use argus_protocol::ViewPress;
use gpui_kit::TestAppContext;
use serde_json::{json, Value};

use super::*;

const T: &str = "7/v.0-table";

fn agent() -> ViewCtx {
    ViewCtx { can_press: true, ..ViewCtx::agent(7, "v") }
}

fn number(n: u32) -> Value {
    json!({"kind": "number", "number": n})
}

fn text(t: &str) -> Value {
    json!({"kind": "text", "text": t})
}

/// A table of `rows` (each a name and a figure), with whatever else a test adds to the block.
fn table(rows: &[(&str, u32)], extra: Value) -> Value {
    let mut block = json!({"kind": "table", "columns": [{"title": "Name"}, {"title": "Took", "numeric": true}], "rows": rows.iter().map(|(n, t)| json!([text(n), number(*t)])).collect::<Vec<_>>()});
    for (k, v) in extra.as_object().unwrap() {
        block[k] = v.clone();
    }
    json!([block])
}

fn top(cx: &mut TestAppContext, rig: &Rig, id: &str) -> f32 {
    bounds(cx, rig, id).unwrap_or_else(|| panic!("{id} is not drawn")).origin.y.into()
}

/// The agent's row numbers in the order they are drawn, top to bottom.
fn order(cx: &mut TestAppContext, rig: &Rig, count: usize) -> Vec<usize> {
    let present: Vec<usize> = (0..count).filter(|r| exists(cx, rig, &format!("{T}-row{r}"))).collect();
    let mut drawn: Vec<(f32, usize)> = present.into_iter().map(|r| (top(cx, rig, &format!("{T}-row{r}")), r)).collect();
    drawn.sort_by(|a, b| a.0.total_cmp(&b.0));
    drawn.into_iter().map(|(_, r)| r).collect()
}

fn ten() -> Vec<(&'static str, u32)> {
    vec![("alpha", 5), ("bravo", 3), ("charlie", 9), ("delta", 1), ("echo", 7), ("foxtrot", 2), ("golf", 8), ("hotel", 4), ("india", 6), ("juliet", 10)]
}

fn press(action: &str, rows: &[&str]) -> String {
    said(Call::PressView { pane: 7, press: ViewPress { view: "v".into(), action: action.into(), rows: rows.iter().map(|r| (*r).to_owned()).collect(), form: None, answers: vec![] } })
}

// TABLE-1
#[gpui_kit::test]
fn a_table_with_no_rows_says_so_and_a_cut_one_says_how_many_it_was(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, json!([{"kind": "table", "columns": [{"title": "A"}], "rows": []}]));
    assert_eq!(label(cx, &rig, T).as_deref(), Some("No rows"));
    let rig = open(cx, 700.0, table(&[("a", 1), ("b", 2)], json!({"total": 40})));
    assert_eq!(label(cx, &rig, "7/v.0-table-more").as_deref(), Some("Showing 2 of 40 rows"));
    let whole = open(cx, 700.0, table(&[("a", 1), ("b", 2)], json!({"total": 2})));
    assert!(!exists(cx, &whole, "7/v.0-table-more"), "a table that was not cut says nothing");
}

// TABLE-2
#[gpui_kit::test]
fn a_header_sorts_ascending_then_descending_and_another_header_starts_ascending(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, table(&[("b", 10), ("a", 9), ("c", 100)], json!({})));
    assert_eq!(order(cx, &rig, 3), [0, 1, 2], "the agent's order until a header is pressed");
    click(cx, &rig, &format!("{T}-head-sort1"));
    assert_eq!(order(cx, &rig, 3), [1, 0, 2], "9, 10, 100: by magnitude, not by digits");
    click(cx, &rig, &format!("{T}-head-sort1"));
    assert_eq!(order(cx, &rig, 3), [2, 0, 1]);
    click(cx, &rig, &format!("{T}-head-sort0"));
    assert_eq!(order(cx, &rig, 3), [1, 0, 2], "another column starts ascending: a, b, c");
}

#[gpui_kit::test]
fn the_column_the_agent_sorted_by_sorts_ascending_on_its_first_press(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, table(&[("a", 3), ("b", 1), ("c", 2)], json!({"sorted_by": 1})));
    assert_eq!(order(cx, &rig, 3), [0, 1, 2], "its direction is not guessed: the agent's order stands");
    click(cx, &rig, &format!("{T}-head-sort1"));
    assert_eq!(order(cx, &rig, 3), [1, 2, 0], "the first press is ascending, not a flip of a direction nobody said");
}

#[gpui_kit::test]
fn a_sort_outlives_a_repaint_and_is_let_go_when_the_view_comes_again(cx: &mut TestAppContext) {
    let rows = [("a", 3), ("b", 1), ("c", 2)];
    let rig = open(cx, 700.0, table(&rows, json!({"row_keys": ["a", "b", "c"], "select": "many"})));
    click(cx, &rig, &format!("{T}-head-sort1"));
    click(cx, &rig, &format!("{T}-row0-pick"));
    assert_eq!(order(cx, &rig, 3), [1, 2, 0], "ticking a row repaints the table and leaves its order alone");
    show(cx, &rig, table(&rows, json!({"row_keys": ["a", "b", "c"], "select": "many"})), agent());
    assert_eq!(order(cx, &rig, 3), [0, 1, 2], "the table resets to the agent's order when the view comes again");
}

// TABLE-3
#[gpui_kit::test]
fn ten_rows_offer_a_filter_that_hides_rows_and_is_kept_across_the_view_coming_again(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, table(&ten(), json!({})));
    assert!(exists(cx, &rig, &format!("{T}-bar-filter-box")));
    click(cx, &rig, &format!("{T}-bar-filter-box"));
    cx.update_window(rig.handle, |_, window, cx| window.input("O", cx)).unwrap();
    settle(cx, &rig);
    // Case is ignored: the names holding an o are bravo, echo, foxtrot, golf and hotel.
    assert_eq!(order(cx, &rig, 10), [1, 4, 5, 6, 7]);
    assert_eq!(label(cx, &rig, &format!("{T}-bar-shown")).as_deref(), Some("5 of 10 rows"));
    assert_eq!(rig.memory.borrow().filter("7/v.0"), "O");
    show(cx, &rig, table(&ten(), json!({})), agent());
    assert_eq!(order(cx, &rig, 10), [1, 4, 5, 6, 7], "what was typed is kept per block, not lost to a republish");
}

#[gpui_kit::test]
fn eight_rows_offer_no_filter_and_a_filter_matches_the_text_of_any_cell(cx: &mut TestAppContext) {
    let few = open(cx, 700.0, table(&ten()[..8], json!({})));
    assert!(!exists(cx, &few, &format!("{T}-bar-filter-box")));
    let rig = open(cx, 700.0, table(&ten(), json!({})));
    rig.memory.borrow_mut().set_filter("7/v.0", "9");
    show(cx, &rig, table(&ten(), json!({})), agent());
    assert_eq!(order(cx, &rig, 10), [2], "9 is the figure of charlie alone");
}

// TABLE-4, TABLE-5
#[gpui_kit::test]
fn ticked_rows_are_counted_and_an_action_presses_with_their_keys_in_the_order_shown(cx: &mut TestAppContext) {
    let keys = json!(["ka", "kb", "kc"]);
    let extra = json!({"row_keys": keys, "select": "many", "actions": [{"id": "rerun", "label": "Re-run", "tone": "good"}]});
    let rig = open(cx, 700.0, table(&[("a", 3), ("b", 1), ("c", 2)], extra));
    assert_eq!(label(cx, &rig, &format!("{T}-bar-picked")).as_deref(), Some("0 selected"));
    click(cx, &rig, "7/v.0|rerun");
    assert!(calls(&rig).is_empty(), "an action waits for a row to act on");
    click(cx, &rig, &format!("{T}-head-sort1"));
    click(cx, &rig, &format!("{T}-row0-pick"));
    click(cx, &rig, &format!("{T}-row2-pick"));
    assert_eq!(label(cx, &rig, &format!("{T}-bar-picked")).as_deref(), Some("2 selected"));
    click(cx, &rig, "7/v.0|rerun");
    assert_eq!(calls(&rig), [press("rerun", &["kc", "ka"])], "the keys go in the order the person sees: 2, then 3");
}

#[gpui_kit::test]
fn select_all_ticks_the_rows_shown_clears_them_and_leaves_the_rows_a_filter_hides(cx: &mut TestAppContext) {
    let names: Vec<String> = (0..10).map(|n| format!("k{n}")).collect();
    let rig = open(cx, 700.0, table(&ten(), json!({"row_keys": names, "select": "many"})));
    rig.memory.borrow_mut().set_selection("7/v.0", ["k0".to_owned()]);
    rig.memory.borrow_mut().set_filter("7/v.0", "o");
    show(cx, &rig, table(&ten(), json!({"row_keys": names, "select": "many"})), agent());
    click(cx, &rig, &format!("{T}-head-all"));
    let picked = rig.memory.borrow_mut().selection("7/v.0", &names);
    assert_eq!(picked.iter().map(String::as_str).collect::<Vec<_>>(), ["k0", "k1", "k4", "k5", "k6", "k7"], "the five that hold an o, and the hidden one that was picked already");
    click(cx, &rig, &format!("{T}-head-all"));
    let left = rig.memory.borrow_mut().selection("7/v.0", &names);
    assert_eq!(left.iter().map(String::as_str).collect::<Vec<_>>(), ["k0"], "clearing takes only what is shown");
}

#[gpui_kit::test]
fn a_table_that_picks_one_row_keeps_one_and_one_without_keys_cannot_be_picked_from(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, table(&[("a", 1), ("b", 2)], json!({"row_keys": ["ka", "kb"], "select": "one"})));
    click(cx, &rig, &format!("{T}-row0-pick"));
    click(cx, &rig, &format!("{T}-row1-pick"));
    let keys = ["ka".to_owned(), "kb".to_owned()];
    let picked = rig.memory.borrow_mut().selection("7/v.0", &keys);
    assert_eq!(picked.into_iter().collect::<Vec<_>>(), ["kb"]);
    assert!(!exists(cx, &rig, &format!("{T}-head-all")), "one row has no select-all");
    let unkeyed = open(cx, 700.0, table(&[("a", 1), ("b", 2)], json!({"select": "many"})));
    assert!(!exists(cx, &unkeyed, &format!("{T}-row0-pick")), "the agent gave no key per row");
}

#[gpui_kit::test]
fn the_rows_ticked_outlast_the_view_coming_again_and_a_key_that_is_gone_is_let_go(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, table(&[("a", 1), ("b", 2)], json!({"row_keys": ["ka", "kb"], "select": "many"})));
    click(cx, &rig, &format!("{T}-row0-pick"));
    click(cx, &rig, &format!("{T}-row1-pick"));
    show(cx, &rig, table(&[("b", 2)], json!({"row_keys": ["kb"], "select": "many"})), agent());
    assert_eq!(label(cx, &rig, &format!("{T}-bar-picked")).as_deref(), Some("1 selected"), "ka is no row now");
}

// TABLE-5
#[gpui_kit::test]
fn copy_as_tsv_writes_the_order_and_the_rows_the_person_sees(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, table(&[("a\tb", 3), ("c", 1), ("d", 2)], json!({})));
    click(cx, &rig, &format!("{T}-head-sort1"));
    click(cx, &rig, &format!("{T}-bar|copy"));
    assert_eq!(calls(&rig), [said(Call::CopyText { text: "Name\tTook\nc\t1\nd\t2\na b\t3".into() })]);
}

#[gpui_kit::test]
fn copy_as_tsv_leaves_out_the_rows_a_filter_hides(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, table(&ten(), json!({})));
    rig.memory.borrow_mut().set_filter("7/v.0", "golf");
    show(cx, &rig, table(&ten(), json!({})), agent());
    click(cx, &rig, &format!("{T}-bar|copy"));
    assert_eq!(calls(&rig), [said(Call::CopyText { text: "Name\tTook\ngolf\t8".into() })]);
}

// TABLE-6
#[gpui_kit::test]
fn a_row_with_details_opens_to_its_blocks_and_stays_open_when_the_view_comes_again(cx: &mut TestAppContext) {
    let extra = json!({"details": [[{"kind": "callout", "tone": "info", "text": "why"}], null]});
    let rig = open(cx, 700.0, table(&[("a", 1), ("b", 2)], extra.clone()));
    assert!(exists(cx, &rig, &format!("{T}-row0-chevron")) && !exists(cx, &rig, &format!("{T}-row1-chevron")), "a row with nothing to open has no chevron");
    assert!(!exists(cx, &rig, "7/v.0.d0.0-callout"));
    click(cx, &rig, &format!("{T}-row0-chevron"));
    assert_eq!(label(cx, &rig, "7/v.0.d0.0-callout").as_deref(), Some("why"), "its blocks are drawn at `.d<row>`");
    assert!(top(cx, &rig, &format!("{T}-row0-detail")) > top(cx, &rig, &format!("{T}-row0")) && top(cx, &rig, &format!("{T}-row0-detail")) < top(cx, &rig, &format!("{T}-row1")), "the detail is the row under it");
    show(cx, &rig, table(&[("a", 1), ("b", 2)], extra), agent());
    assert!(exists(cx, &rig, "7/v.0.d0.0-callout"));
    click(cx, &rig, &format!("{T}-row0-chevron"));
    assert!(!exists(cx, &rig, "7/v.0.d0.0-callout"));
}

#[gpui_kit::test]
fn a_detail_belongs_to_its_row_not_to_where_the_row_is_sorted_to(cx: &mut TestAppContext) {
    let extra = json!({"details": [null, [{"kind": "callout", "tone": "info", "text": "b's"}]]});
    let rig = open(cx, 700.0, table(&[("a", 9), ("b", 1)], extra));
    click(cx, &rig, &format!("{T}-head-sort1"));
    click(cx, &rig, &format!("{T}-row1-chevron"));
    assert!(exists(cx, &rig, "7/v.0.d1.0-callout"), "the path names the row the agent sent, so what is kept stays with it");
}

// TABLE-1: optional columns
#[gpui_kit::test]
fn an_optional_column_gives_way_when_the_box_is_narrow(cx: &mut TestAppContext) {
    let block = json!([{"kind": "table", "columns": [{"title": "Name"}, {"title": "Note", "optional": true}], "rows": [[text("a"), text("b")]]}]);
    let rig = open(cx, 700.0, block);
    assert!(exists(cx, &rig, &format!("{T}-head-sort1")));
    set_width(cx, &rig, 400.0);
    assert!(!exists(cx, &rig, &format!("{T}-head-sort1")), "an optional column goes under 560");
    assert!(exists(cx, &rig, &format!("{T}-head-sort0")));
}

#[gpui_kit::test]
fn the_filter_input_is_not_written_by_a_redraw_while_someone_types(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, table(&ten(), json!({})));
    click(cx, &rig, &format!("{T}-bar-filter-box"));
    cx.update_window(rig.handle, |_, window, cx| window.input("ab", cx)).unwrap();
    settle(cx, &rig);
    show(cx, &rig, table(&ten(), json!({})), agent());
    cx.update_window(rig.handle, |_, window, cx| window.input("c", cx)).unwrap();
    settle(cx, &rig);
    assert_eq!(rig.memory.borrow().filter("7/v.0"), "abc", "a republish in the middle of typing keeps what was typed");
}
