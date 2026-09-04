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
        out.contains("nothing decided under this feature yet"),
        "a tab somebody pressed must say what it is for:
{out}"
    );
    assert_eq!(
        app.layout.checkouts.outer.width, 0,
        "no column is drawn, so no click may resolve against one"
    );
    assert!(
        app.layout.features.outer.width + app.layout.content.outer.width > 80,
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
    assert_eq!(app.focus, Focus::View, "clicking a tab hands it the keyboard");
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

fn decision(id: i64, parent: Option<i64>, chose: &str) -> argus_protocol::Decision {
    argus_protocol::Decision {
        id,
        parent,
        at: 0,
        session: None,
        checkout: None,
        feature: None,
        chose: chose.to_string(),
        over: None,
        because: None,
        superseded_by: None,
    }
}

fn app_with_a_board(decisions: Vec<argus_protocol::Decision>) -> App {
    let mut app = app_with_tree();
    let name = app.current_project().unwrap().name.clone();
    app.on_server_msg(argus_protocol::ServerMsg::Decisions(Box::new(
        argus_protocol::DecisionBoard {
            project: None,
            name,
            features: Vec::new(),
            decisions,
        },
    )));
    press(&mut app, View::Decisions.digit());
    app
}

#[test]
fn the_board_draws_a_decision_under_the_one_that_constrained_it() {
    let mut app = app_with_a_board(vec![
        argus_protocol::Decision {
            over: Some("a file per feature".into()),
            because: Some("both need migrations".into()),
            ..decision(1, None, "sqlite")
        },
        decision(2, Some(1), "wal mode"),
    ]);
    let buf = draw_at(&mut app, 100, 30);
    let out = lines(&buf);
    let top = app.layout.content.inner.y as usize;

    assert!(out[top].contains("#1 sqlite"), "{:?}", out[top]);
    assert!(
        out[top + 1].contains("over a file per feature")
            && out[top + 1].contains("because both need migrations"),
        "{:?}",
        out[top + 1]
    );
    let child = out[top + 2].clone();
    assert!(child.contains("└─ #2 wal mode"), "{child:?}");
    assert!(out[top + 1].contains('│'), "the branch crosses the detail row");
}

#[test]
fn sibling_and_nested_decisions_draw_a_connected_tree() {
    let mut app = app_with_a_board(vec![
        decision(1, None, "root"),
        decision(2, Some(1), "first child"),
        decision(3, Some(2), "grandchild"),
        decision(4, Some(1), "last child"),
    ]);
    let buf = draw_at(&mut app, 100, 30);
    let out = lines(&buf);
    let top = app.layout.content.inner.y as usize;

    assert!(out[top + 2].contains("├─ #2 first child"), "{:?}", out[top + 2]);
    assert!(out[top + 4].contains("│  └─ #3 grandchild"), "{:?}", out[top + 4]);
    assert!(out[top + 6].contains("└─ #4 last child"), "{:?}", out[top + 6]);
}

#[test]
fn a_decision_with_neither_an_alternative_nor_a_reason_says_so() {
    let mut app = app_with_a_board(vec![decision(1, None, "sqlite")]);
    let buf = draw_at(&mut app, 100, 30);
    let out = lines(&buf);
    let top = app.layout.content.inner.y as usize;
    assert!(
        out[top + 1].contains("no alternative or reason recorded"),
        "{:?}",
        out[top + 1]
    );
}

#[test]
fn a_superseded_decision_keeps_its_place_and_says_what_replaced_it() {
    let mut app = app_with_a_board(vec![
        argus_protocol::Decision {
            superseded_by: Some(2),
            ..decision(1, None, "key notes by id")
        },
        decision(2, None, "key notes by path"),
    ]);
    let buf = draw_at(&mut app, 100, 30);
    let out = lines(&buf).join("
");

    assert!(out.contains("#1 key notes by id"), "{out}");
    assert!(out.contains("superseded by #2"), "{out}");
}

#[test]
fn the_board_scrolls_to_keep_the_selection_on_screen() {
    let many = (1..=40).map(|id| decision(id, None, "a choice")).collect();
    let mut app = app_with_a_board(many);
    draw_at(&mut app, 100, 30);

    // The keys start on the feature column, and `l` crosses into the tree.
    app.on_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
    app.on_key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE));
    let buf = draw_at(&mut app, 100, 30);
    let out = lines(&buf).join("
");

    assert_eq!(app.board_sel, 39);
    assert!(out.contains("#40"), "the last row is drawn: {out}");
}

fn feature(slug: &str, title: &str) -> argus_protocol::Feature {
    argus_protocol::Feature {
        slug: slug.to_string(),
        title: title.to_string(),
        body: String::new(),
        origin_checkout: None,
        origin_branch: Some("main".into()),
        at: 0,
        session: None,
        state: argus_protocol::FeatureState::Proposed,
        claimed_by: None,
        claimed_at: None,
        blocker: None,
        evidence: None,
    }
}

fn app_with_features(
    features: Vec<argus_protocol::Feature>,
    decisions: Vec<argus_protocol::Decision>,
) -> App {
    let mut app = app_with_tree();
    let name = app.current_project().unwrap().name.clone();
    app.on_server_msg(argus_protocol::ServerMsg::Decisions(Box::new(
        argus_protocol::DecisionBoard {
            project: None,
            name,
            features,
            decisions,
        },
    )));
    press(&mut app, View::Decisions.digit());
    app
}

#[test]
fn the_board_draws_one_features_decisions_and_offers_the_others() {
    let mut notes = decision(1, None, "one row per note");
    notes.feature = Some("notes".into());
    let mut pty = decision(2, None, "one reader thread");
    pty.feature = Some("pty".into());
    let mut app = app_with_features(
        vec![feature("notes", "Notes storage"), feature("pty", "The pty")],
        vec![notes, pty],
    );

    let out = lines(&draw_at(&mut app, 100, 30)).join("\n");
    assert!(out.contains("Notes storage"), "both features are offered: {out}");
    assert!(out.contains("The pty"), "{out}");
    assert!(out.contains("one row per note"), "{out}");
    assert!(
        !out.contains("one reader thread"),
        "another feature's decisions are not on this board: {out}"
    );

    // Moving down the feature column swaps the tree beside it.
    app.on_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    let out = lines(&draw_at(&mut app, 100, 30)).join("\n");
    assert!(out.contains("one reader thread"), "{out}");
    assert!(!out.contains("one row per note"), "{out}");
}

#[test]
fn decisions_from_before_features_are_kept_on_a_row_of_their_own() {
    let mut filed = decision(1, None, "one row per note");
    filed.feature = Some("notes".into());
    let mut app = app_with_features(
        vec![feature("notes", "Notes storage")],
        vec![filed, decision(2, None, "sqlite")],
    );

    let out = lines(&draw_at(&mut app, 100, 30)).join("\n");
    assert!(out.contains("before features"), "nothing is silently lost: {out}");
    app.on_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    let out = lines(&draw_at(&mut app, 100, 30)).join("\n");
    assert!(out.contains("sqlite"), "{out}");
}

#[test]
fn a_board_for_another_project_is_dropped_rather_than_drawn() {
    let mut app = app_with_tree();
    app.on_server_msg(argus_protocol::ServerMsg::Decisions(Box::new(
        argus_protocol::DecisionBoard {
            project: None,
            name: "something else".into(),
            features: Vec::new(),
            decisions: vec![decision(1, None, "not ours")],
        },
    )));
    press(&mut app, View::Decisions.digit());
    let buf = draw_at(&mut app, 100, 30);
    let out = lines(&buf).join("
");

    assert!(!out.contains("not ours"), "{out}");
    assert!(out.contains("nothing decided under this feature yet"), "{out}");
}

#[test]
fn a_click_on_the_board_stays_in_the_view_and_picks_the_row() {
    let mut app = app_with_a_board(vec![
        decision(1, None, "sqlite"),
        decision(2, Some(1), "one row per note"),
        decision(3, Some(1), "key notes by path"),
    ]);
    app.open_view(View::Decisions);
    draw_at(&mut app, 100, 30);
    let inner = app.layout.content.inner;

    click(&mut app, inner.x + 2, inner.y + 2 * crate::ui::ROW_HEIGHT);

    assert_eq!(
        app.focus,
        Focus::View,
        "the board is not the pane whose column used to be there"
    );
    assert_eq!(app.board_sel, 2, "and the row clicked is the row selected");
}

#[test]
fn a_click_past_the_last_row_selects_nothing_new() {
    let mut app = app_with_a_board(vec![decision(1, None, "sqlite")]);
    app.open_view(View::Decisions);
    draw_at(&mut app, 100, 30);
    let inner = app.layout.content.inner;

    click(&mut app, inner.x + 2, inner.y + inner.height - 1);

    assert_eq!(app.focus, Focus::View);
    assert_eq!(app.board_sel, 0);
}

#[test]
fn a_board_opened_before_the_tree_arrived_is_asked_for_when_it_does() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut app = App::new(tx);
    // The view is reachable before the first tree lands, and asking then
    // means asking about a project the client does not have yet.
    app.open_view(View::Decisions);
    assert!(app.board.is_none());

    app.on_server_msg(argus_protocol::ServerMsg::Tree(super::tree()));

    let mut asked = false;
    while let Ok(msg) = rx.try_recv() {
        asked |= matches!(msg, argus_protocol::ClientMsg::GetDecisions { .. });
    }
    assert!(asked, "the board is asked for without anyone pressing r");
}

#[test]
fn a_tree_that_moves_nothing_does_not_ask_for_the_board_again() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut app = App::new(tx);
    app.on_server_msg(argus_protocol::ServerMsg::Tree(super::tree()));
    app.open_view(View::Decisions);
    let project = app.current_project().unwrap();
    app.on_server_msg(argus_protocol::ServerMsg::Decisions(Box::new(
        argus_protocol::DecisionBoard {
            project: Some(project.id),
            name: project.name.clone(),
            features: Vec::new(),
            decisions: vec![decision(1, None, "sqlite")],
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

fn click(app: &mut App, column: u16, row: u16) {
    app.on_mouse(crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    });
}


fn carded(slug: &str, title: &str, state: argus_protocol::FeatureState) -> argus_protocol::Feature {
    argus_protocol::Feature {
        state,
        ..feature(slug, title)
    }
}

/// Opens the board view over a set of features.
fn board_of(features: Vec<argus_protocol::Feature>) -> App {
    let mut app = app_with_features(features, Vec::new());
    press(&mut app, View::Board.digit());
    app
}

#[test]
fn every_feature_is_drawn_under_the_column_it_is_in() {
    use argus_protocol::FeatureState::*;
    let mut app = board_of(vec![
        carded("pty", "Streaming the pty", Active),
        carded("notes", "Notes storage", Proposed),
        carded("review", "Split review", Done),
    ]);

    let out = lines(&draw_at(&mut app, 140, 20)).join("\n");
    for column in ["proposed", "active", "blocked", "submitted", "done"] {
        assert!(out.contains(column), "every column is offered: {out}");
    }
    for title in ["Streaming the pty", "Notes storage", "Split review"] {
        assert!(out.contains(title), "{out}");
    }

    // Each title sits under its own state, not merely somewhere on screen.
    let drawn = lines(&draw_at(&mut app, 140, 20));
    let row = |needle: &str| {
        drawn
            .iter()
            .position(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("{needle} was not drawn"))
    };
    assert!(
        row("Notes storage") > row("proposed"),
        "a card is drawn below its column heading"
    );
}

#[test]
fn the_keys_cross_columns_and_walk_the_cards_in_one() {
    use argus_protocol::FeatureState::*;
    let mut app = board_of(vec![
        carded("notes", "Notes storage", Proposed),
        carded("pty", "Streaming the pty", Active),
        carded("board", "The feature board", Active),
    ]);
    assert_eq!(app.selected_card().map(|f| f.slug.as_str()), Some("notes"));

    app.on_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
    assert_eq!(app.selected_card().map(|f| f.slug.as_str()), Some("pty"));
    app.on_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    assert_eq!(app.selected_card().map(|f| f.slug.as_str()), Some("board"));

    // An empty column selects nothing rather than the card that was there.
    app.on_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
    assert_eq!(app.selected_card(), None, "nothing is blocked");

    app.on_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
    assert_eq!(
        app.selected_card().map(|f| f.slug.as_str()),
        Some("pty"),
        "coming back lands on the first card, not the one you left"
    );
}

#[test]
fn a_card_says_what_its_column_leaves_unsaid() {
    use argus_protocol::FeatureState::*;
    let mut blocked = carded("pty", "Streaming the pty", Blocked);
    blocked.blocker = Some("ConPTY resize".into());
    let mut submitted = carded("notes", "Notes storage", Submitted);
    submitted.evidence = Some("green on cargo test".into());
    let mut app = board_of(vec![blocked, submitted]);

    let out = lines(&draw_at(&mut app, 140, 20)).join("\n");
    assert!(out.contains("ConPTY resize"), "{out}");
    assert!(out.contains("green on cargo test"), "{out}");
}

#[test]
fn enter_on_a_card_opens_the_decisions_under_that_feature() {
    use argus_protocol::FeatureState::*;
    let mut notes = decision(1, None, "one row per note");
    notes.feature = Some("notes".into());
    let mut pty = decision(2, None, "one reader thread");
    pty.feature = Some("pty".into());
    let mut app = app_with_features(
        vec![
            carded("notes", "Notes storage", Proposed),
            carded("pty", "Streaming the pty", Proposed),
        ],
        vec![notes, pty],
    );
    press(&mut app, View::Board.digit());
    app.on_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.view, View::Decisions);
    let out = lines(&draw_at(&mut app, 100, 30)).join("\n");
    assert!(out.contains("one reader thread"), "{out}");
    assert!(
        !out.contains("one row per note"),
        "the card you came from is the feature you land on: {out}"
    );
}
