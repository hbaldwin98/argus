//! What is drawn: a leaf for each stretch of text core numbers, in step with core's numbering.

use argus_desktop_core::views::markdown::blocks::parse;
use argus_desktop_core::views::markdown::leaves::leaves;
use gpui_kit::TestAppContext;

use super::*;

const DOC: &str = "# Title\n\nA paragraph with **bold**, `code` and a [link](https://a.b/c).\n\n- one\n  - nested\n- two\n\n1. first\n2. second\n\n> quoted\n\n```\nlet x = 1;\n```\n\n---\n\n| a | b |\n|---|---|\n| 1 | 2 |\n";

#[gpui_kit::test]
fn every_leaf_core_numbers_is_drawn_once_under_its_number(cx: &mut TestAppContext) {
    let opened = open(cx, DOC);
    let expected = leaves(&parse(DOC)).len();
    assert_eq!(expected, 13);
    let drawn: Vec<usize> = cx.update(|cx| {
        let mut keys: Vec<usize> = opened.view.read(cx).layouts.borrow().keys().copied().collect();
        keys.sort_unstable();
        keys
    });
    assert_eq!(drawn, (0..expected).collect::<Vec<_>>(), "a place in the document is read against core's numbering");
    assert_eq!(cx.update(|cx| opened.view.read(cx).leaf_count()), expected);
}

#[gpui_kit::test]
fn the_leaves_are_drawn_in_reading_order_down_the_page(cx: &mut TestAppContext) {
    let opened = open(cx, "first\n\n- second\n\n> third\n\nfourth");
    let tops: Vec<f32> = (0..4).map(|leaf| at(cx, &opened, leaf, 0).y.as_f32()).collect();
    assert!(tops.windows(2).all(|pair| pair[0] < pair[1]), "{tops:?}");
}

#[gpui_kit::test]
fn the_cells_of_a_row_are_side_by_side_and_the_rows_stack(cx: &mut TestAppContext) {
    let opened = open(cx, "| a | b |\n|---|---|\n| 1 | 2 |\n");
    let (a, b, one) = (at(cx, &opened, 0, 0), at(cx, &opened, 1, 0), at(cx, &opened, 2, 0));
    assert_eq!(a.y, b.y, "one row");
    assert!(b.x > a.x);
    assert!(one.y > a.y && one.x == a.x, "the next row, in the same column");
}

#[gpui_kit::test]
fn a_view_that_shows_the_same_text_again_keeps_its_selection_and_other_text_puts_it_away(cx: &mut TestAppContext) {
    let opened = open(cx, "hello world");
    let (from, to) = (at(cx, &opened, 0, 0), at(cx, &opened, 0, 5));
    drag(cx, &opened, from, to);
    assert_eq!(selected(cx, &opened).as_deref(), Some("hello"));
    cx.update(|cx| opened.view.update(cx, |view, cx| view.set_text("hello world", cx)));
    assert_eq!(selected(cx, &opened).as_deref(), Some("hello"), "an agent showing the view again changed nothing here");
    cx.update(|cx| opened.view.update(cx, |view, cx| view.set_text("something else", cx)));
    settle(cx, &opened);
    assert_eq!(selected(cx, &opened), None);
    assert_eq!(cx.update(|cx| opened.view.read(cx).leaf_count()), 1);
}

#[gpui_kit::test]
fn raw_html_and_an_unsafe_link_are_drawn_as_the_text_they_are(cx: &mut TestAppContext) {
    let opened = open(cx, "<script>alert(1)</script> [x](javascript:alert(1)) ![i](data:image/png;base64,AAAA)");
    let text = cx.update(|cx| opened.view.read(cx).leaves.iter().map(|leaf| leaf.styled.text.clone()).collect::<Vec<_>>());
    assert_eq!(text, ["<script>alert(1)</script> x i"]);
    assert!(cx.update(|cx| opened.view.read(cx).leaves[0].styled.links.is_empty()));
}

#[gpui_kit::test]
fn a_tables_columns_are_as_wide_as_their_cells_and_not_stretched_to_the_view(cx: &mut TestAppContext) {
    let opened = open(cx, "| a | b | c |
|---|---|---|
| 1 | 2 | 3 |
");
    let (a, b, c) = (at(cx, &opened, 0, 0), at(cx, &opened, 1, 0), at(cx, &opened, 2, 0));
    // The view is 600 px wide: equal thirds would put the second column near 200 px.
    assert!((c.x - a.x).as_f32() < 150.0 && b.x > a.x && c.x > b.x, "{:?} {:?} {:?}", a.x, b.x, c.x);
}

#[gpui_kit::test]
fn a_wide_cell_widens_its_column_alone(cx: &mut TestAppContext) {
    let opened = open(cx, "| a | b |
|---|---|
| a much longer cell | 2 |
");
    let (a, b) = (at(cx, &opened, 0, 0), at(cx, &opened, 1, 0));
    assert!((b.x - a.x).as_f32() > 100.0, "the column is as wide as its longest cell: {:?}", b.x - a.x);
}
