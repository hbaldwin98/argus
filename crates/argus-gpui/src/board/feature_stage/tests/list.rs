//! The list and the peek: the rows as Rust sends them, and what a press on a row or on the panel
//! sends (BOARD-LIST-1 to 11, BOARD-PEEK-1 to 7).

use argus_desktop_core::connection::page::Features;
use gpui_kit::TestAppContext;

use super::*;
use crate::keys::root::tests::hear;

/// BOARD-LIST-1, 11: a label is drawn whenever the group changes, in the order Rust sends; the view
/// never sorts, so a list sent in another order is drawn in it.
#[gpui_kit::test]
fn the_rows_are_drawn_in_rusts_order_under_a_label_where_the_group_changes(cx: &mut TestAppContext) {
    let opened = on(cx, list());
    for group in ["active", "queued", "held", "archived"] {
        assert!(present(cx, &opened, format!("group-{group}")), "{group}");
    }
    assert!(!present(cx, &opened, "group-done"));
    let first = |cx: &mut TestAppContext, id: &str| cx.update_window(opened.handle, |_, window, _| window.find(id.to_owned()).bounds().origin.y).unwrap();
    assert!(first(cx, "feature-row-trim") < first(cx, "feature-row-mouse") && first(cx, "feature-row-mouse") < first(cx, "feature-row-bump"));
    // The same rows in the other order, as Rust may send them: queued before active.
    let mut swapped = list();
    swapped.rows.swap(0, 1);
    hear(cx, &opened, vec![heard(swapped)]);
    assert!(first(cx, "feature-row-mouse") < first(cx, "feature-row-trim"), "not sorted by the view");
    assert!(present(cx, &opened, "group-queued") && present(cx, &opened, "group-active"));
}

/// BOARD-LIST-2, 8: a row names its number and title, and the peeked one says it is current.
#[gpui_kit::test]
fn a_row_names_its_feature_and_the_peeked_one_is_current(cx: &mut TestAppContext) {
    let opened = on(cx, list());
    assert_eq!(label(cx, &opened, "feature-row-trim").as_deref(), Some("#41 Trim context injection"));
    assert_eq!(selected(cx, &opened, "feature-row-trim"), Some(true));
    assert_eq!(selected(cx, &opened, "feature-row-mouse"), Some(false));
}

/// BOARD-LIST-7: a click peeks, a double click opens, a right click peeks and opens the Move menu.
#[gpui_kit::test]
fn a_click_peeks_a_double_click_opens_and_a_right_click_peeks_and_opens_the_menu(cx: &mut TestAppContext) {
    let opened = on(cx, list());
    click(cx, &opened, "feature-row-mouse");
    assert_eq!(sent(&opened), ["Peek { slug: \"mouse\" }"]);
    assert_eq!(selected(cx, &opened, "feature-row-mouse"), Some(false), "the cursor moves when the session says so");
    double_click(cx, &opened, "feature-row-mouse");
    assert!(sent(&opened).contains(&"OpenFeature { slug: Some(\"mouse\") }".to_owned()));
    right_click(cx, &opened, "feature-row-mouse");
    assert_eq!(sent(&opened), ["Peek { slug: \"mouse\" }"]);
    assert_eq!(label(cx, &opened, "feature-menu").as_deref(), Some("#42 Mouse selection"));
}

/// BOARD-LIST-9: an empty list says why by the filter and the fold.
#[gpui_kit::test]
fn an_empty_list_says_why(cx: &mut TestAppContext) {
    let nothing = Features { rows: Vec::new(), peek: None, ..list() };
    let opened = on(cx, nothing.clone());
    assert_eq!(label(cx, &opened, "feature-list-empty").as_deref(), Some("Every feature here is held or archived."));
    hear(cx, &opened, vec![heard(Features { filter: "zz".into(), folded: 0, ..nothing.clone() })]);
    assert_eq!(label(cx, &opened, "feature-list-empty").as_deref(), Some("Nothing matches \u{201c}zz\u{201d}."));
    hear(cx, &opened, vec![heard(Features { filter: "zz".into(), folded: 3, show_folded: false, ..nothing })]);
    assert_eq!(label(cx, &opened, "feature-list-empty").as_deref(), Some("Only held or archived features match \u{201c}zz\u{201d}."));
}

/// BOARD-PEEK-1: with nothing peeked the panel says so.
#[gpui_kit::test]
fn with_nothing_peeked_the_panel_says_to_choose(cx: &mut TestAppContext) {
    let opened = on(cx, Features { peek: None, ..list() });
    assert_eq!(label(cx, &opened, "feature-peek").as_deref(), Some("Peek"));
    assert!(!present(cx, &opened, "peek-open"));
}

/// BOARD-PEEK-2 to 5: the panel names the feature and has the sections Rust filled, only the ones
/// that have lines.
#[gpui_kit::test]
fn the_panel_names_the_feature_and_draws_only_the_sections_with_lines(cx: &mut TestAppContext) {
    let opened = on(cx, list());
    assert_eq!(label(cx, &opened, "feature-peek").as_deref(), Some("Peek at #41"));
    assert!(present(cx, &opened, "peek-needs-you") && present(cx, &opened, "peek-next-up"));
    assert!(!present(cx, &opened, "peek-in-flight") && !present(cx, &opened, "peek-last-decided"));
}

/// BOARD-PEEK-7: `Open` opens the peeked feature and `Go to its pane` goes to its agent's.
#[gpui_kit::test]
fn the_panels_buttons_open_the_feature_and_go_to_its_pane(cx: &mut TestAppContext) {
    let opened = on(cx, list());
    click(cx, &opened, "peek-open");
    assert_eq!(sent(&opened), ["OpenFeature { slug: Some(\"trim\") }"]);
    click(cx, &opened, "peek-go");
    assert_eq!(sent(&opened), ["GoToPane { pane: 1 }"]);
}

/// BOARD-PEEK-7: with no pane the button is off and sends nothing.
#[gpui_kit::test]
fn without_a_pane_go_to_its_pane_is_off(cx: &mut TestAppContext) {
    let opened = on(cx, peeking(list(), "mouse"));
    click(cx, &opened, "peek-go");
    assert!(sent(&opened).is_empty());
}

/// BOARD-PEEK-7: the `...` opens the Move menu with `Comes after...`, only when the feature is among the rows.
#[gpui_kit::test]
fn the_more_button_opens_the_menu_with_comes_after_only_for_a_listed_feature(cx: &mut TestAppContext) {
    let opened = on(cx, list());
    click(cx, &opened, "peek-more");
    assert!(present(cx, &opened, "feature-menu"));
    assert_eq!(label(cx, &opened, ("feature-menu-item", 10usize)).as_deref(), Some("Comes after\u{2026}"));
    let mut hidden = list();
    hidden.peek = Some(peek_of(&row(99, "gone", "Folded away", "archived")));
    hear(cx, &opened, vec![heard(hidden)]);
    assert!(!present(cx, &opened, "peek-more"), "the peeked feature is not among the rows");
}
