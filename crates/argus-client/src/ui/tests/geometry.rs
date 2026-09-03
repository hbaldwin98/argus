//! Centering and stacking, without a frame around them.

use super::*;

// --- layout helpers -----------------------------------------------------

#[test]
fn a_modal_is_centered_in_its_area() {
    let r = centered_rect(10, 4, Rect::new(0, 0, 30, 20));
    assert_eq!((r.x, r.y, r.width, r.height), (10, 8, 10, 4));
}

#[test]
fn a_modal_larger_than_the_screen_is_pinned_not_wrapped_negative() {
    let r = centered_rect(50, 40, Rect::new(0, 0, 30, 20));
    assert_eq!((r.x, r.y), (0, 0), "saturating, never underflowing");
}

#[test]
fn rows_stack_down_the_panel_and_stop_at_its_bottom() {
    // Two lines per item, and a half-drawn item is worse than none.
    let inner = Rect::new(1, 1, 10, 5);
    assert_eq!(row_rect_of(inner, 0, ROW_HEIGHT).unwrap().y, 1);
    assert_eq!(row_rect_of(inner, 0, ROW_HEIGHT).unwrap().height, ROW_HEIGHT);
    assert_eq!(row_rect_of(inner, 1, ROW_HEIGHT).unwrap().y, 3);
    assert!(row_rect_of(inner, 2, ROW_HEIGHT).is_none(), "no room for both its lines");
}

// --- overflow -----------------------------------------------------------

/// A repository with `n` worktrees, which is more than any card can show.
fn app_with_many_checkouts(n: u64) -> App {
    let mut app = app_with_tree();
    let r = &mut app.tree[0].repositories[0];
    for i in 0..n {
        let mut c = r.checkouts[1].clone();
        c.id = CheckoutId(100 + i);
        c.name = format!("feature-{i}");
        r.checkouts.push(c);
    }
    app.focus = Focus::Checkouts;
    app
}

/// Which rows of the frame carry a scroll thumb, so a test can say where
/// it sat rather than only that it existed.
fn thumb_rows(buf: &ratatui::buffer::Buffer) -> Vec<usize> {
    lines(buf)
        .iter()
        .enumerate()
        .filter(|(_, line)| line.contains(SCROLL_THUMB))
        .map(|(y, _)| y)
        .collect()
}

#[test]
fn a_column_showing_less_than_it_holds_says_so() {
    let mut short = app_with_tree();
    assert!(
        thumb_rows(&draw(&mut short)).is_empty(),
        "a list that fits has nothing to scroll to"
    );

    let mut long = app_with_many_checkouts(20);
    assert!(
        !thumb_rows(&draw(&mut long)).is_empty(),
        "twenty checkouts in a card that holds eight must not scroll silently"
    );
}

#[test]
fn the_thumb_tracks_how_far_down_the_list_has_gone() {
    let mut app = app_with_many_checkouts(20);
    let top = thumb_rows(&draw(&mut app));

    app.sel_checkout = app.checkout_row_count() - 1;
    let bottom = thumb_rows(&draw(&mut app));

    assert!(top.first() < bottom.first(), "{top:?} then {bottom:?}");
    assert_eq!(top.len(), bottom.len(), "the thumb keeps its size");
}

#[test]
fn the_thumb_stays_inside_its_card() {
    let mut app = app_with_many_checkouts(20);
    for (w, h) in [(120u16, 20u16), (150, 40), (80, 10)] {
        let buf = draw_at(&mut app, w, h);
        let card = app.layout.checkouts;
        for y in thumb_rows(&buf) {
            let y = y as u16;
            assert!(
                y >= card.inner.y && y < card.inner.bottom(),
                "the thumb left the card at {w}x{h}: row {y} against {:?}",
                card.inner
            );
        }
    }
}
