//! The board: its columns, its cards, the drag of a card and the cue that a column is cut off
//! (BOARD-COLS-1 to 14, BOARD-DRAG-1 to 13).

use gpui_kit::{point, px, MouseButton, Pixels, Point, TestAppContext};

use super::*;
use crate::keys::root::tests::{hear, open_sized, overlays, press};
use crate::kit::overlay::kind::Overlay;

fn centre(cx: &mut TestAppContext, opened: &Opened, id: &str) -> Point<Pixels> {
    let id = id.to_owned();
    cx.update_window(opened.handle, |_, window, _| window.find(id).bounds().center()).unwrap()
}

fn frame(cx: &mut TestAppContext, opened: &Opened) {
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.run_until_parked();
}

/// Presses on the card and takes it `to` in a few moves, leaving the button down.
fn carry(cx: &mut TestAppContext, opened: &Opened, id: &str, to: Point<Pixels>) {
    let from = centre(cx, opened, id);
    cx.update_window(opened.handle, |_, window, cx| window.pointer_down(from, MouseButton::Left, cx)).unwrap();
    for step in 1..=4 {
        let at = point(from.x + (to.x - from.x) * (step as f32 / 4.0), from.y + (to.y - from.y) * (step as f32 / 4.0));
        cx.update_window(opened.handle, |_, window, cx| window.pointer_move(at, Some(MouseButton::Left), cx)).unwrap();
        frame(cx, opened);
    }
}

fn release(cx: &mut TestAppContext, opened: &Opened, at: Point<Pixels>) {
    cx.update_window(opened.handle, |_, window, cx| window.pointer_up(at, MouseButton::Left, cx)).unwrap();
    frame(cx, opened);
}

/// Drags the card `id` and drops it on `column`.
fn drop_on(cx: &mut TestAppContext, opened: &Opened, id: &str, column: &str) {
    let to = centre(cx, opened, column);
    carry(cx, opened, id, to);
    release(cx, opened, to);
}

/// BOARD-COLS-1, 2: Queued, Active and Done as lanes and Held over Archived, each named with its
/// count, and a folded Archived counting what it folds.
#[gpui_kit::test]
fn the_columns_are_named_with_their_counts_and_a_folded_archived_counts_what_it_folds(cx: &mut TestAppContext) {
    let opened = on(cx, board());
    for (column, said) in [("queued", "Queued, 1 features"), ("active", "Active, 1 features"), ("done", "Done, 0 features"), ("held", "Held, 1 features"), ("archived", "Archived, 1 features")] {
        assert_eq!(label(cx, &opened, format!("feature-column-{column}")).as_deref(), Some(said), "{column}");
    }
    assert!(present(cx, &opened, "feature-card-trim") && !present(cx, &opened, "feature-card-old"), "a folded column lists no cards");
}

/// BOARD-COLS-3: an empty column says its two lines.
#[gpui_kit::test]
fn an_empty_column_says_what_lands_in_it(cx: &mut TestAppContext) {
    let opened = on(cx, board());
    assert_eq!(label(cx, &opened, "empty-done").as_deref(), Some("Nothing done yet"));
    assert!(!present(cx, &opened, "empty-active"));
}

/// BOARD-COLS-5, 6: a folded Archived is one row that unfolds it, and unfolded ends with a button
/// that folds it.
#[gpui_kit::test]
fn archived_unfolds_from_its_row_and_folds_from_its_button(cx: &mut TestAppContext) {
    let opened = on(cx, board());
    click(cx, &opened, "unfold-archived");
    assert_eq!(sent(&opened), ["FeatureAction { action: Folded }"]);
    let mut open = list();
    open.layout = "board";
    hear(cx, &opened, vec![heard(open)]);
    assert!(present(cx, &opened, "feature-card-old") && !present(cx, &opened, "unfold-archived"));
    click(cx, &opened, "fold-archived");
    assert_eq!(sent(&opened), ["FeatureAction { action: Folded }"]);
}

/// BOARD-COLS-7, 10: a card names its number and title, and says how it came to stand there.
#[gpui_kit::test]
fn a_card_names_its_feature_and_says_how_it_came_to_stand_there(cx: &mut TestAppContext) {
    let opened = on(cx, board());
    assert_eq!(label(cx, &opened, "feature-card-trim").as_deref(), Some("#41 Trim context injection"));
    assert_eq!(label(cx, &opened, "card-footer-bump").as_deref(), Some("written down \u{b7} 1d ago"));
    assert_eq!(selected(cx, &opened, "feature-card-trim"), Some(true));
    assert_eq!(selected(cx, &opened, "feature-card-mouse"), Some(false));
}

/// BOARD-COLS-11: a click peeks, a double click opens, a right click peeks and opens the menu.
#[gpui_kit::test]
fn a_card_click_peeks_a_double_click_opens_and_a_right_click_opens_the_menu(cx: &mut TestAppContext) {
    let opened = on(cx, board());
    click(cx, &opened, "feature-card-mouse");
    assert_eq!(sent(&opened), ["Peek { slug: \"mouse\" }"]);
    double_click(cx, &opened, "feature-card-mouse");
    assert!(sent(&opened).contains(&"OpenFeature { slug: Some(\"mouse\") }".to_owned()));
    right_click(cx, &opened, "feature-card-mouse");
    assert_eq!(sent(&opened), ["Peek { slug: \"mouse\" }"]);
    assert!(present(cx, &opened, "feature-menu"));
}

/// BOARD-DRAG-3, 5: a dragged card leaves a placeholder saying where it is going, and only a column
/// that takes it lights up with its banner.
#[gpui_kit::test]
fn a_dragged_card_leaves_a_placeholder_and_only_held_lights_up_with_its_banner(cx: &mut TestAppContext) {
    let opened = on(cx, board());
    let held = centre(cx, &opened, "feature-column-held");
    carry(cx, &opened, "feature-card-trim", held);
    assert_eq!(label(cx, &opened, "feature-placeholder-trim").as_deref(), Some("#41 is being dragged to Held"));
    assert_eq!(label(cx, &opened, "dropzone-held").as_deref(), Some("Drop to hold: you say why #41 waits"));
    let done = centre(cx, &opened, "feature-column-done");
    cx.update_window(opened.handle, |_, window, cx| window.pointer_move(done, Some(MouseButton::Left), cx)).unwrap();
    frame(cx, &opened);
    assert_eq!(label(cx, &opened, "feature-placeholder-trim").as_deref(), Some("#41 is being dragged"), "Done follows the work and does not light up");
    assert!(!present(cx, &opened, "dropzone-done") && !present(cx, &opened, "dropzone-held"));
    release(cx, &opened, done);
    assert!(!present(cx, &opened, "feature-placeholder-trim"), "the card is back where Rust last put it");
}

/// BOARD-DRAG-7: a drop on Held asks why, a drop on Archived sends the drop, and nothing is sent
/// until the person says why.
#[gpui_kit::test]
fn a_drop_on_held_asks_why_and_a_drop_on_archived_is_sent(cx: &mut TestAppContext) {
    let opened = on(cx, board());
    drop_on(cx, &opened, "feature-card-trim", "feature-column-held");
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    assert_eq!(overlays(cx), [Overlay::Hold]);
    assert!(sent(&opened).is_empty(), "no peek, no open, no hold until the person says why");
    press(cx, &opened, "escape");
    drop_on(cx, &opened, "feature-card-trim", "unfold-archived");
    assert_eq!(sent(&opened), ["DropFeature { slug: \"trim\", on: Archived }"], "the folded row counts as the Archived column");
}

/// BOARD-DRAG-9: a drop on a column that follows the work is sent for Rust to answer with its notice.
#[gpui_kit::test]
fn a_drop_on_a_column_that_follows_the_work_is_sent_for_rust_to_answer(cx: &mut TestAppContext) {
    let opened = on(cx, board());
    drop_on(cx, &opened, "feature-card-mouse", "feature-column-done");
    assert_eq!(sent(&opened), ["DropFeature { slug: \"mouse\", on: Done }"]);
}

/// BOARD-DRAG-7, 11: a drop on the card's own column, or outside every column, sends nothing.
#[gpui_kit::test]
fn a_drop_on_its_own_column_or_on_no_column_sends_nothing(cx: &mut TestAppContext) {
    let opened = on(cx, board());
    drop_on(cx, &opened, "feature-card-trim", "feature-column-active");
    assert!(sent(&opened).is_empty());
    let nowhere = point(px(2.0), px(2.0));
    carry(cx, &opened, "feature-card-trim", nowhere);
    release(cx, &opened, nowhere);
    assert!(sent(&opened).is_empty());
    assert!(overlays(cx).is_empty());
}

/// BOARD-DRAG-13: Escape cancels a drag: the ghost goes, and nothing is sent even when the button is
/// let go over a column that takes the card.
#[gpui_kit::test]
fn escape_cancels_a_drag_and_nothing_is_sent(cx: &mut TestAppContext) {
    let opened = on(cx, board());
    let archived = centre(cx, &opened, "unfold-archived");
    carry(cx, &opened, "feature-card-trim", archived);
    assert!(present(cx, &opened, "feature-placeholder-trim"));
    press(cx, &opened, "escape");
    assert!(!present(cx, &opened, "feature-placeholder-trim"));
    release(cx, &opened, archived);
    assert!(sent(&opened).is_empty());
    assert!(overlays(cx).is_empty());
}

/// BOARD-DRAG-8: the release that ends a drag is no click: it peeks and opens nothing.
#[gpui_kit::test]
fn the_release_that_ends_a_drag_is_not_a_click(cx: &mut TestAppContext) {
    let opened = on(cx, board());
    let back = centre(cx, &opened, "feature-card-mouse");
    let away = point(back.x + px(40.0), back.y + px(40.0));
    carry(cx, &opened, "feature-card-mouse", away);
    release(cx, &opened, away);
    assert!(sent(&opened).is_empty(), "{:?}", sent(&opened));
}

/// BOARD-DRAG-2: a press that never goes the threshold is a click.
#[gpui_kit::test]
fn a_press_that_goes_nowhere_is_a_click_not_a_drag(cx: &mut TestAppContext) {
    let opened = on(cx, board());
    click(cx, &opened, "feature-card-mouse");
    assert!(!present(cx, &opened, "feature-placeholder-mouse"));
    assert_eq!(sent(&opened), ["Peek { slug: \"mouse\" }"]);
}

/// BOARD-COLS-12: a wide window has no chip.
#[gpui_kit::test]
fn a_wide_window_has_no_chip_for_columns_cut_off(cx: &mut TestAppContext) {
    let wide = on(cx, board());
    assert!(!present(cx, &wide, "feature-board-more"));
    assert!(present(cx, &wide, "feature-board-fade"), "the fade is there and clear");
}

/// BOARD-COLS-12, 13: a window too narrow for the columns scrolls them sideways, with a chip naming
/// those cut off.
#[gpui_kit::test]
fn a_narrow_window_names_the_columns_cut_off(cx: &mut TestAppContext) {
    let narrow = open_sized(cx, 900.0);
    let rail = argus_desktop_core::connection::page::Rail { stage: Stage::Feature, ..crate::checkouts::stage::tests::rail() };
    hear(cx, &narrow, vec![connected(), Event::Rail(Box::new(rail)), heard(board())]);
    frame(cx, &narrow);
    frame(cx, &narrow);
    let said = label(cx, &narrow, "feature-board-more").expect("a column is cut off");
    assert!(said.contains(" more") && said.contains("Held"), "{said}");
}

/// BOARD-COLS-14: the lanes keep where they were scrolled across the snapshots a push brings.
#[gpui_kit::test]
fn the_lanes_keep_their_scroll_across_a_pushed_board(cx: &mut TestAppContext) {
    let narrow = open_sized(cx, 900.0);
    let rail = argus_desktop_core::connection::page::Rail { stage: Stage::Feature, ..crate::checkouts::stage::tests::rail() };
    hear(cx, &narrow, vec![connected(), Event::Rail(Box::new(rail)), heard(board())]);
    frame(cx, &narrow);
    let at = |cx: &mut TestAppContext| centre(cx, &narrow, "feature-column-active").x;
    let before = at(cx);
    cx.update_window(narrow.handle, |_, window, cx| window.scroll("feature-lanes", gpui_kit::ScrollDelta::Pixels(point(px(-120.0), px(0.0))), cx)).unwrap();
    frame(cx, &narrow);
    let scrolled = at(cx);
    assert!(scrolled < before, "the lanes went sideways: {before:?} to {scrolled:?}");
    hear(cx, &narrow, vec![heard(Features { filter: String::new(), ..board() })]);
    assert_eq!(at(cx), scrolled, "a push did not move them");
}
