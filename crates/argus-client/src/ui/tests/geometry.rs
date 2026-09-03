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

// --- content-sized columns ----------------------------------------------

fn row(name: &str, detail: &str) -> Item<'static> {
    Item::new(
        vec![Span::raw("● "), Span::raw(name.to_string())],
        vec![Span::raw(detail.to_string())],
    )
}

#[test]
fn a_column_asks_for_the_width_its_widest_row_needs() {
    let narrow = natural_width(&[row("a", "b")], "panes", "");
    let wide = natural_width(&[row("a-much-longer-pane-name", "b")], "panes", "");
    assert!(wide > narrow, "{wide} should beat {narrow}");
    assert!(narrow >= MIN_COLUMN_WIDTH && wide <= MAX_COLUMN_WIDTH);
}

#[test]
fn the_detail_line_and_the_title_are_measured_too() {
    // Both can be the longest thing in the card, and either being cut is
    // the truncation this exists to stop.
    let by_detail = natural_width(&[row("a", "twenty-four chars of it!")], "x", "");
    let by_name = natural_width(&[row("a", "b")], "x", "");
    assert!(by_detail > by_name);

    let by_title = natural_width(&[row("a", "b")], "projects · some-workspace", "");
    assert!(by_title > by_name, "a title is not allowed to be ellipsized");
}

#[test]
fn an_empty_column_is_sized_by_what_it_says_instead() {
    // A first run whose only instruction is cut off has nowhere to go.
    let hint = natural_width(&[], "projects", "no projects yet

n  add one");
    assert!(hint as usize >= "no projects yet".len() + CARD_CHROME, "{hint}");
}

#[test]
fn widths_move_in_steps_so_a_renamed_pane_does_not_shift_the_spine() {
    // An agent renaming its pane, or changing what its note says, must not
    // drag every column sideways.
    let widths: Vec<u16> = ["claude", "claude!", "claude!!", "claude!!!"]
        .iter()
        .map(|name| natural_width(&[row(name, "working")], "panes", ""))
        .collect();
    assert_eq!(widths[0], widths[1]);
    assert!(widths.iter().all(|w| w % WIDTH_STEP as u16 == 0));
}

#[test]
fn a_column_never_hoards_more_than_a_list_of_names_is_worth() {
    let huge = natural_width(&[row(&"x".repeat(200), "y")], "panes", "");
    assert_eq!(huge, MAX_COLUMN_WIDTH, "the live view can always use it better");
}

#[test]
fn the_live_view_gets_what_the_nav_columns_did_not_want() {
    let mut app = app_with_tree();
    let wide = draw_at(&mut app, 200, 24);
    let _ = wide;
    let nav: u16 = [
        app.layout.projects.outer.width,
        app.layout.repositories.outer.width,
        app.layout.checkouts.outer.width,
        app.layout.panes.outer.width,
    ]
    .iter()
    .sum();
    assert!(
        app.layout.content.outer.width > nav,
        "a wide terminal should go to the pty, not be shared out four ways:          {} vs {nav}",
        app.layout.content.outer.width
    );
}

