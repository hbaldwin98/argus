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
            strip.contains(&view.label().to_ascii_uppercase()),
            "the strip must name {}: {strip:?}",
            view.label()
        );
    }
}

#[test]
fn a_digit_opens_its_view_over_the_whole_content_area() {
    let mut app = app_with_tree();
    draw_at(&mut app, 100, 30);
    assert!(app.layout.terminal.outer.width > 0, "the workspace is drawn");

    press(&mut app, View::Feature.digit());
    let out = lines(&draw_at(&mut app, 100, 30)).join("\n");

    assert_eq!(app.view, View::Feature);
    assert!(
        out.contains("No features in orion yet."),
        "a tab somebody pressed must say what it is for:\n{out}"
    );
    assert_eq!(
        app.layout.terminal.outer.width, 0,
        "no terminal is drawn, so no click may resolve against one"
    );
    assert!(app.layout.features.outer.width > 40, "the view has the stage");
}

#[test]
fn the_digit_three_opens_checkouts() {
    let mut app = app_with_tree();

    press(&mut app, '3');
    assert_eq!(app.view, View::Checkouts);

    // The strip ends at three: a fourth digit is not a view.
    press(&mut app, '4');
    assert_eq!(app.view, View::Checkouts);
}

#[test]
fn coming_back_lands_on_the_column_you_left() {
    let mut app = app_with_tree();
    app.focus = Focus::Checkouts;

    press(&mut app, View::Feature.digit());
    assert_eq!(app.focus, Focus::View, "the view owns the keyboard");
    // j would otherwise move a selection in a column that is not drawn.
    press(&mut app, 'j');
    assert_eq!(app.sel_checkout, 0);

    press(&mut app, View::Workspace.digit());
    assert_eq!(app.view, View::Workspace);
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
    press(&mut app, View::Feature.digit());

    assert_eq!(
        app.grids.len(),
        subscribed,
        "switching views is a change of surface, not of what is running"
    );
    assert_eq!(
        app.focus,
        Focus::View,
        "but the keys stop reaching the pane"
    );
}

#[test]
fn clicking_a_tab_opens_it() {
    let mut app = app_with_tree();
    draw_at(&mut app, 100, 30);
    let views = app.layout.views;
    let strip = views.outer;
    // The second tab's first cell, found the way the renderer draws it.
    let x = (0..strip.width)
        .find(|x| crate::ui::tab_at(views, None, strip.x + x, strip.y) == Some(View::Feature))
        .expect("the decisions tab is on screen");

    app.on_mouse(crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
        column: strip.x + x,
        row: strip.y,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(app.view, View::Feature);
    assert_eq!(
        app.focus,
        Focus::View,
        "clicking a tab hands it the keyboard"
    );
}

#[test]
fn clicking_the_workspace_tab_withdraws_pane_focus() {
    let mut app = app_with_tree();
    app.focus = Focus::PaneContent;
    draw_at(&mut app, 100, 30);
    let views = app.layout.views;
    let strip = views.outer;
    let x = (0..strip.width)
        .find(|x| crate::ui::tab_at(views, None, strip.x + x, strip.y) == Some(View::Workspace))
        .expect("the workspace tab is on screen");

    app.on_mouse(crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
        column: strip.x + x,
        row: strip.y,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(app.view, View::Workspace);
    assert_eq!(
        app.focus,
        Focus::Panes,
        "the workspace tab is chrome, not the live pane"
    );
}

#[test]
fn a_click_before_the_first_frame_lands_on_no_tab() {
    assert_eq!(crate::ui::tab_at(Panel::default(), None, 0, 0), None);
}

#[test]
fn a_board_opened_before_the_tree_arrived_is_asked_for_when_it_does() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut app = App::new(tx);
    // The view is reachable before the first tree lands, and asking then
    // means asking about a project the client does not have yet.
    app.open_view(View::Feature);
    assert!(app.board_follower.board().is_none());

    app.on_server_msg(argus_protocol::ServerMsg::Tree(super::tree()));

    let mut asked = false;
    while let Ok(msg) = rx.try_recv() {
        asked |= matches!(msg, argus_protocol::ClientMsg::GetDecisions { .. });
    }
    assert!(asked, "the board is asked for without anyone pressing r");
}

#[test]
fn a_reconnect_asks_for_the_board_again_and_keeps_it_on_screen_meanwhile() {
    // Whatever was pushed while the daemon was away was pushed to nobody.
    let (mut app, _old) = feature_stage();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    app.reconnect(tx);
    assert!(app.board_follower.board().is_some());

    app.on_server_msg(argus_protocol::ServerMsg::Tree(super::tree()));

    let asked: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok())
        .filter(|msg| matches!(msg, argus_protocol::ClientMsg::GetDecisions { .. } | argus_protocol::ClientMsg::Task { .. }))
        .collect();
    assert_eq!(asked.len(), 2, "the board and the feature's tasks: {asked:?}");
    assert_eq!(app.feature_slug().as_deref(), Some("trim"));
}

#[test]
fn a_tree_that_moves_nothing_does_not_ask_for_the_board_again() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut app = App::new(tx);
    app.on_server_msg(argus_protocol::ServerMsg::Tree(super::tree()));
    app.open_view(View::Feature);
    let project = app.current_project().unwrap();
    app.on_server_msg(argus_protocol::ServerMsg::Decisions(Box::new(
        argus_protocol::DecisionBoard {
            repositories: Vec::new(),
            project: Some(project.id),
            tracker_map: Default::default(),
            name: project.name.clone(),
            features: Vec::new(),
            decisions: vec![argus_protocol::Decision {
                id: 1,
                chose: "sqlite".into(),
                ..Default::default()
            }],
        },
    )));
    while rx.try_recv().is_ok() {}

    app.on_server_msg(argus_protocol::ServerMsg::Tree(super::tree()));

    let mut asks = 0;
    while let Ok(msg) = rx.try_recv() {
        if matches!(msg, argus_protocol::ClientMsg::GetDecisions { .. }) {
            asks += 1;
        }
    }
    assert_eq!(asks, 0, "the board on screen is already this project's");
}

#[test]
fn a_working_pane_turns_and_a_settled_one_does_not() {
    let mut app = app_with_tree();
    app.choose_rail_row(crate::app::RailRow::Repository(0));
    let epoch = app.epoch();
    // A pane's own row in the rail, never the workspace heading beside it,
    // which names the same pane and carries no glyph that turns.
    let rail_row = |app: &mut App, frame: u32, name: &str| {
        app.set_frame_now(epoch + crate::motion::SPINNER_FRAME * frame);
        let buf = draw_at(app, 120, 30);
        let rail = app.layout.rail.outer;
        (rail.y..rail.bottom())
            .map(|y| row_text(&buf, y, rail))
            .find(|row| row.contains(name))
            .expect("the pane has a row in the rail")
    };

    let glyphs: Vec<char> = (0..4)
        .map(|frame| {
            rail_row(&mut app, frame, "claude")
                .chars()
                .find(|c| "⠋⠙⠹⠸⠼⠴⠦⠧".contains(*c))
                .expect("the working row carries a spinner")
        })
        .collect();
    let distinct: std::collections::HashSet<&char> = glyphs.iter().collect();
    assert_eq!(
        distinct.len(),
        4,
        "four consecutive frames should show four glyphs, not {glyphs:?}"
    );

    // The idle pane beside it is unmoved by any of that: motion marks the
    // one state that is ongoing, and would say nothing if everything had it.
    let idle: Vec<String> = (0..4).map(|frame| rail_row(&mut app, frame, "shell")).collect();
    assert!(
        idle.windows(2).all(|w| w[0] == w[1]),
        "an idle row should be identical frame to frame: {idle:?}"
    );
}

#[test]
fn nothing_moving_means_no_frame_is_owed() {
    let mut app = app_with_tree();
    // The fixture has a pane mid-turn, so the spinner is asking for frames.
    assert!(app.next_motion_deadline().is_some());

    for project in &mut app.tree {
        for repository in &mut project.repositories {
            for checkout in &mut repository.checkouts {
                for pane in &mut checkout.panes {
                    pane.status = argus_protocol::PaneStatus::Idle;
                }
            }
        }
    }

    assert!(
        app.next_motion_deadline().is_none(),
        "a screen with nothing turning should let the loop sleep"
    );
}

#[test]
fn focus_travels_between_columns_rather_than_snapping() {
    let mut app = app_with_tree();
    let epoch = app.epoch();
    app.focus = Focus::Projects;
    app.set_frame_now(epoch);

    let lit_of = |app: &App| {
        (
            app.focus_lit(Focus::Projects).value(),
            app.focus_lit(Focus::Repositories).value(),
        )
    };
    assert_eq!(lit_of(&app), (1.0, 0.0), "settled on the column it is in");

    // Focus moves. The frame that notices it starts the fade, so that one
    // still draws the old card lit: the move begins from where the eye
    // already is rather than from a state nothing was ever drawn in.
    app.focus = Focus::Repositories;
    app.set_frame_now(epoch);
    assert_eq!(lit_of(&app), (1.0, 0.0));

    // The frames after it are the travel.
    app.set_frame_now(epoch + crate::motion::FOCUS_FADE / 2);
    let (leaving, arriving) = lit_of(&app);
    assert!(
        leaving > 0.0 && leaving < 1.0,
        "the column being left should be dimming, not off: {leaving}"
    );
    assert!(
        arriving > 0.0 && arriving < 1.0,
        "the column being entered should be brightening, not on: {arriving}"
    );
    assert!(
        (leaving + arriving - 1.0).abs() < f32::EPSILON,
        "what one loses the other gains: {leaving} + {arriving}"
    );
    assert!(
        app.next_motion_deadline().is_some(),
        "a fade in flight owes the loop another frame"
    );

    // A column that is neither end of the move is untouched by it.
    assert_eq!(app.focus_lit(Focus::Panes).value(), 0.0);

    app.set_frame_now(epoch + crate::motion::FOCUS_FADE * 2);
    assert_eq!(lit_of(&app), (0.0, 1.0), "and it settles where focus went");
}

#[test]
fn the_first_frame_of_a_session_is_already_settled() {
    // Nothing should fade in from wherever the default happened to sit.
    let mut app = app_with_tree();
    app.set_frame_now(app.epoch());
    assert_eq!(app.focus_lit(app.focus).value(), 1.0);
}

#[test]
fn moving_to_another_checkout_of_the_repository_keeps_its_board_and_asks_nothing() {
    // The board is the repository's whichever checkout names it, so a
    // second ask would only fetch the board already on screen.
    let (mut app, mut rx) = feature_stage();
    while rx.try_recv().is_ok() {}
    app.sel_checkout = 1;
    let other = app.current_checkout().unwrap().id;
    app.on_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));

    let asked = std::iter::from_fn(|| rx.try_recv().ok())
        .any(|msg| matches!(msg, argus_protocol::ClientMsg::GetDecisions { .. }));
    assert!(!asked);
    assert!(app.board_follower.board().is_some());
    assert_eq!(app.board_follower.scope().map(|scope| scope.checkout), Some(other), "what it asks next names it");
}

#[test]
fn a_remote_host_is_named_in_the_product_mark_and_the_tabs_still_click() {
    // Every tab under the mark is that host's, so the mark is what says
    // whose agents these are. It is wider for it, and clicks must still
    // land on the tab drawn where they land.
    let mut app = app_with_tree();
    app.host = Some("devbox".into());
    let buf = draw_at(&mut app, 100, 30);
    assert!(
        row_of(&buf, "ARGUS · devbox").is_some(),
        "{}",
        lines(&buf).join("\n")
    );

    let views = app.layout.views;
    let strip = views.outer;
    let x = (0..strip.width)
        .find(|x| crate::ui::tab_at(views, None, strip.x + x, strip.y) == Some(View::Feature))
        .expect("the feature tab is on screen");
    let under: String = (x..x + 12)
        .filter_map(|dx| buf.cell((strip.x + dx, strip.y)).map(|c| c.symbol().to_string()))
        .collect();
    assert!(under.contains("FEATURE"), "the click lands on {under:?}");
}
