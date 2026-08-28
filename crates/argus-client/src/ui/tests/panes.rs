//! A pane parked above its live screen.

use super::*;

#[test]
fn a_parked_pane_draws_its_history_rather_than_the_live_screen() {
    let mut app = app_scrolled_back(120, 4000, 'H');
    let text = lines(&draw(&mut app)).join("\n");
    assert!(text.contains("HHH"), "the rows read out of the scrollback");
    assert!(
        !text.contains("LLL"),
        "and not the live screen underneath them"
    );
}

#[test]
fn a_parked_pane_shows_how_far_back_it_is_in_its_title() {
    // A pane parked in history looks exactly like a quiet one. Without
    // this the operator has no way to tell that what is on screen is
    // not what the child is doing now.
    let mut app = app_scrolled_back(120, 4000, 'H');
    let text = lines(&draw(&mut app)).join("\n");
    assert!(text.contains("\u{2191} 120/4000"), "got:\n{text}");
}

#[test]
fn a_parked_pane_says_how_to_get_back_to_the_live_screen() {
    let mut app = app_scrolled_back(120, 4000, 'H');
    assert!(bar(&draw(&mut app)).contains("scrolled back"));

    let mut live = app_with_tree();
    live.focus = Focus::PaneContent;
    assert!(bar(&draw(&mut live)).contains("shift-pgup scroll"));
}

#[test]
fn a_parked_pane_does_not_take_the_hardware_cursor() {
    // The child's cursor belongs to the live screen, which is not the
    // one being drawn. Placing it here points at unrelated history.
    let mut app = app_scrolled_back(120, 4000, 'H');
    let pane = app.column_pane().unwrap();
    app.grids.get_mut(&pane).unwrap().cursor = argus_protocol::Cursor {
        row: 1,
        col: 1,
        visible: true,
        ..Default::default()
    };
    draw(&mut app);
    assert_eq!(app.layout.cursor, None);
}
