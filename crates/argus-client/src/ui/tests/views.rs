//! The tab strip, and what opening a view does to the screen.

use super::*;

use crate::app::View;

fn press(app: &mut App, c: char) {
    app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
}

#[test]
fn the_strip_names_every_view_and_marks_the_open_one() {
    let mut app = app_with_tree();
    let buf = draw_at(&mut app, 100, 30);
    let strip = lines(&buf)[app.layout.views.outer.y as usize].clone();

    for view in View::ALL {
        assert!(
            strip.contains(view.label()),
            "the strip must name {}: {strip:?}",
            view.label()
        );
        assert!(
            strip.contains(view.digit()),
            "and say which key opens it: {strip:?}"
        );
    }
}

#[test]
fn a_digit_opens_its_view_over_the_whole_content_area() {
    let mut app = app_with_tree();
    draw_at(&mut app, 100, 30);
    assert!(app.layout.checkouts.outer.width > 0, "the spine is drawn");

    press(&mut app, View::Decisions.digit());
    let buf = draw_at(&mut app, 100, 30);
    let out = lines(&buf).join("
");

    assert_eq!(app.view, View::Decisions);
    assert!(
        out.contains("no decisions recorded yet"),
        "a tab somebody pressed must say what it is for:
{out}"
    );
    assert_eq!(
        app.layout.checkouts.outer.width, 0,
        "no column is drawn, so no click may resolve against one"
    );
    assert!(
        app.layout.content.outer.width > 80,
        "the view has the content area rather than a column of it"
    );
}

#[test]
fn coming_back_lands_on_the_column_you_left() {
    let mut app = app_with_tree();
    app.focus = Focus::Checkouts;

    press(&mut app, View::Decisions.digit());
    assert_eq!(app.focus, Focus::View, "the view owns the keyboard");
    // j would otherwise move a selection in a column that is not drawn.
    press(&mut app, 'j');
    assert_eq!(app.sel_checkout, 0);

    press(&mut app, View::Spine.digit());
    assert_eq!(app.view, View::Spine);
    assert_eq!(app.focus, Focus::Checkouts);
}

#[test]
fn a_view_does_not_stop_the_panes_running_behind_it() {
    let mut app = app_with_tree();
    app.focus = Focus::PaneContent;
    let subscribed = app.grids.len();

    // From inside a pane every key belongs to the child, so a view is
    // reached the way review and history are: through the leader.
    app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::CONTROL));
    press(&mut app, View::Decisions.digit());

    assert_eq!(
        app.grids.len(),
        subscribed,
        "switching views is a change of surface, not of what is running"
    );
    assert_eq!(app.focus, Focus::View, "but the keys stop reaching the pane");
}

#[test]
fn clicking_a_tab_opens_it() {
    let mut app = app_with_tree();
    draw_at(&mut app, 100, 30);
    let strip = app.layout.views.outer;
    // The second tab's first cell, found the way the renderer draws it.
    let x = (0..strip.width)
        .find(|x| crate::ui::tab_at(strip, strip.x + x, strip.y) == Some(View::Decisions))
        .expect("the decisions tab is on screen");

    app.on_mouse(crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
        column: strip.x + x,
        row: strip.y,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(app.view, View::Decisions);
}

#[test]
fn a_click_before_the_first_frame_lands_on_no_tab() {
    assert_eq!(crate::ui::tab_at(Rect::default(), 0, 0), None);
}

#[test]
fn a_terminal_too_short_for_a_strip_still_draws_the_spine() {
    let mut app = app_with_tree();
    draw_at(&mut app, 100, 2);
    assert_eq!(app.layout.views.outer.height, 0);
    assert!(app.layout.checkouts.outer.width > 0);
}
