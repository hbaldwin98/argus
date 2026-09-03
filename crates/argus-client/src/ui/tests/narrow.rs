//! What the spine does when the terminal is too small to hold it: folds a
//! column away rather than squeezing every column past legibility, drops
//! the detail line rather than the items, and shortens the keymap rather
//! than letting it be cut mid-word.

use super::*;

// --- folding rather than squeezing --------------------------------------

#[test]
fn the_default_width_is_exactly_what_the_whole_spine_needs() {
    // The breakpoints are derived from the floors, so this is the one place
    // the two are checked against each other.
    assert_eq!(Fold::required(120), Fold::None);
    assert_eq!(Fold::required(spine_min_width(5) + GUTTER_COLS * 2), Fold::None);
    assert_eq!(
        Fold::required(spine_min_width(5) + GUTTER_COLS * 2 - 1),
        Fold::Projects,
        "one cell short of five columns folds one away"
    );
    assert_eq!(Fold::required(80), Fold::Repositories);
    assert_eq!(Fold::required(20), Fold::Repositories, "and stops there");
}

#[test]
fn a_narrow_terminal_folds_columns_away_instead_of_crushing_them() {
    let mut app = app_with_tree();
    let buf = draw_at(&mut app, 80, 24);

    assert_eq!(app.fold, Fold::Repositories);
    let text = lines(&buf).join("\n");
    assert!(!text.contains("repositories"), "no repositories card:\n{text}");
    assert!(text.contains("checkouts"), "checkouts survives:\n{text}");
    assert!(text.contains("panes"), "panes survives:\n{text}");

    // Nothing became unreachable: the live view's title still names the
    // whole path, tabs mark what was folded, and focus is off them.
    assert!(text.contains("argus \u{203a} orion"), "breadcrumb intact:\n{text}");
    assert_eq!(app.layout.projects.outer.width, GUTTER_COLS, "a tab, not a card");
    assert_eq!(app.layout.repositories.outer.width, GUTTER_COLS);
    assert_eq!(app.focus, Focus::Checkouts);
}

#[test]
fn the_live_view_keeps_its_floor_while_the_nav_columns_give_way() {
    let mut app = app_with_tree();
    for width in [80u16, 90, 100, 120, 200] {
        draw_at(&mut app, width, 24);
        assert!(
            app.layout.content.outer.width >= MIN_CONTENT_WIDTH,
            "the pty was squeezed under its floor at {width}: {}",
            app.layout.content.outer.width
        );
    }
}

#[test]
fn widening_the_terminal_does_not_undo_a_fold_the_user_chose() {
    let mut app = app_with_tree();
    draw_at(&mut app, 200, 24);
    assert_eq!(app.fold, Fold::None);

    app.cycle_fold();
    assert_eq!(app.fold, Fold::Projects);
    draw_at(&mut app, 220, 24);
    assert_eq!(app.fold, Fold::Projects, "a resize only ever folds further");

    // And `p` still works at a width that could not fit the column, because
    // being unable to reach a repository at all would be worse.
    draw_at(&mut app, 80, 24);
    assert_eq!(app.fold, Fold::Repositories);
    app.cycle_fold();
    assert_eq!(app.fold, Fold::None);
    let text = lines(&draw_at(&mut app, 80, 24)).join("\n");
    assert!(
        text.contains("repos"),
        "brought back anyway, squeezed rather than refused:\n{text}"
    );
}

// --- one-line rows on a short terminal -----------------------------------

#[test]
fn a_short_terminal_drops_the_detail_line_rather_than_the_items() {
    let mut app = app_with_a_long_checkout_column();
    let tall = draw_at(&mut app, 120, 24);
    assert_eq!(app.layout.row_height, ROW_HEIGHT);
    let tall_rows = app.layout.checkouts.inner.height / ROW_HEIGHT;

    let short = draw_at(&mut app, 120, 16);
    assert_eq!(app.layout.row_height, COMPACT_ROW_HEIGHT);
    let short_rows = app.layout.checkouts.inner.height;
    assert!(
        short_rows > tall_rows,
        "the shorter card shows more items, not fewer: {short_rows} vs {tall_rows}"
    );

    let named = |buf: &ratatui::buffer::Buffer, n: &str| lines(buf).iter().any(|l| l.contains(n));
    assert!(named(&tall, "primary"), "the detail line is there when it fits");
    assert!(!named(&short, "primary"), "and gone when it does not");
    assert!(named(&short, "wt-0") && named(&short, "wt-4"), "items remain");
}

#[test]
fn a_click_lands_on_the_row_it_looks_like_with_compact_rows() {
    let mut app = app_with_a_long_checkout_column();
    draw_at(&mut app, 120, 16);
    assert_eq!(app.layout.row_height, COMPACT_ROW_HEIGHT);

    let inner = app.layout.checkouts.inner;
    app.on_mouse(crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
        column: inner.x + 1,
        row: inner.y + 3,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(app.sel_checkout, 3, "one line per row, so row 3 is item 3");
}

// --- what a squeezed row keeps -------------------------------------------

#[test]
fn a_badge_outlives_the_tail_of_a_name_it_cannot_fit_beside() {
    let mut app = app_with_tree();
    app.tree[0].name = "a-project-with-a-very-long-name".to_string();
    app.column_widths = Some(vec![20, 18, 18, 18, 46]);
    let text = lines(&draw_at(&mut app, 140, 24)).join("\n");

    let row = text
        .lines()
        // Not the live view's title, which carries the same name whole.
        .find(|l| l.contains("a-proj") && !l.contains('\u{203a}'))
        .expect("the project row");
    assert!(row.contains('\u{2026}'), "the name gives way: {row:?}");
    assert!(row.contains("2 \u{25a3}"), "the count does not: {row:?}");
}

#[test]
fn a_badge_gives_way_once_the_name_has_nothing_left() {
    let mut app = app_with_tree();
    app.column_widths = Some(vec![MIN_COLUMN_WIDTH, 18, 18, 18, 46]);
    let text = lines(&draw_at(&mut app, 140, 24)).join("\n");

    let row = text
        .lines()
        .find(|l| l.contains("argus"))
        .expect("the project row");
    assert!(
        !row.contains("2 \u{25a3}"),
        "a badge that leaves no room to name the thing is not worth keeping: {row:?}"
    );
}

// --- the status bar ------------------------------------------------------

#[test]
fn a_narrow_bar_shortens_the_keymap_rather_than_cutting_it() {
    let mut app = app_with_tree();
    app.focus = Focus::Checkouts;

    let wide = bar(&draw_at(&mut app, 200, 24));
    assert!(wide.contains("F fetch") && wide.contains("H history"), "{wide:?}");

    for width in [60u16, 70, 80, 100, 120] {
        let bar = bar(&draw_at(&mut app, width, 24));
        assert!(
            bar.chars().count() <= width as usize,
            "the bar overran {width}: {bar:?}"
        );
        // Whatever tier survived, it ends on a whole word rather than
        // wherever the width happened to land -- and the way to the rest
        // of the keys is the one thing no width drops.
        assert!(
            bar.trim_end().ends_with("? keys"),
            "the way to the full keymap went missing at {width}: {bar:?}"
        );
    }
}
