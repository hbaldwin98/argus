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

    press(&mut app, View::Feature.digit());
    let buf = draw_at(&mut app, 100, 30);
    let out = lines(&buf).join("
");

    assert_eq!(app.view, View::Feature);
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
        app.layout.features.outer.width + app.layout.feature_decisions.outer.width > 80,
        "the view has the content area rather than a column of it"
    );
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
    press(&mut app, View::Feature.digit());

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
        .find(|x| crate::ui::tab_at(strip, strip.x + x, strip.y) == Some(View::Feature))
        .expect("the decisions tab is on screen");

    app.on_mouse(crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
        column: strip.x + x,
        row: strip.y,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(app.view, View::Feature);
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
    press(&mut app, View::Feature.digit());
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
    let top = app.layout.feature_decisions.inner.y as usize;

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
    let top = app.layout.feature_decisions.inner.y as usize;

    assert!(out[top + 2].contains("├─ #2 first child"), "{:?}", out[top + 2]);
    assert!(out[top + 4].contains("│  └─ #3 grandchild"), "{:?}", out[top + 4]);
    assert!(out[top + 6].contains("└─ #4 last child"), "{:?}", out[top + 6]);
}

#[test]
fn a_decision_with_neither_an_alternative_nor_a_reason_says_so() {
    let mut app = app_with_a_board(vec![decision(1, None, "sqlite")]);
    let buf = draw_at(&mut app, 100, 30);
    let out = lines(&buf);
    let top = app.layout.feature_decisions.inner.y as usize;
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

    // The keys start on the feature column; `l` crosses into the tasks
    // and then into the tree.
    app.on_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
    app.on_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
    app.on_key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE));
    let buf = draw_at(&mut app, 100, 30);
    let out = lines(&buf).join("
");

    assert_eq!(app.decision_sel, 39);
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
        state: argus_protocol::FeatureState::Open,
        checkouts: Vec::new(),
        tasks: Default::default(),
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
    press(&mut app, View::Feature.digit());
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
    press(&mut app, View::Feature.digit());
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
    app.open_view(View::Feature);
    draw_at(&mut app, 100, 30);
    let inner = app.layout.feature_decisions.inner;

    click(&mut app, inner.x + 2, inner.y + 2 * crate::ui::ROW_HEIGHT);

    assert_eq!(
        app.focus,
        Focus::View,
        "the board is not the pane whose column used to be there"
    );
    assert_eq!(app.decision_sel, 2, "and the row clicked is the row selected");
}

#[test]
fn a_click_past_the_last_row_selects_nothing_new() {
    let mut app = app_with_a_board(vec![decision(1, None, "sqlite")]);
    app.open_view(View::Feature);
    draw_at(&mut app, 100, 30);
    let inner = app.layout.feature_decisions.inner;

    click(&mut app, inner.x + 2, inner.y + inner.height - 1);

    assert_eq!(app.focus, Focus::View);
    assert_eq!(app.decision_sel, 0);
}

#[test]
fn a_board_opened_before_the_tree_arrived_is_asked_for_when_it_does() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut app = App::new(tx);
    // The view is reachable before the first tree lands, and asking then
    // means asking about a project the client does not have yet.
    app.open_view(View::Feature);
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
    app.open_view(View::Feature);
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

#[test]
fn a_feature_row_says_what_is_happening_to_it_rather_than_what_state_it_was_dragged_to() {
    let mut app = app_with_features(
        vec![
            feature("notes", "Notes storage"),
            argus_protocol::Feature {
                tasks: argus_protocol::TaskCounts {
                    todo: 2,
                    doing: 1,
                    done: 4,
                },
                ..feature("pty", "Streaming the pty")
            },
        ],
        Vec::new(),
    );

    let out = lines(&draw_at(&mut app, 120, 30)).join("\n");
    assert!(out.contains("4/7 tasks"), "how far along it is: {out}");
    assert!(
        out.contains("main"),
        "and where to look when nothing is happening yet: {out}"
    );
    for column in ["proposed", "blocked", "submitted"] {
        assert!(
            !out.contains(column),
            "the drag-maintained columns are gone: {out}"
        );
    }
}

#[test]
fn a_feature_says_which_of_its_agents_has_stopped_for_somebody() {
    // The checkout `super::tree()` builds, which is where the panes are.
    let path = crate::app::App::new(tokio::sync::mpsc::unbounded_channel().0);
    let _ = path;
    let mut app = app_with_tree();
    let checkout = app
        .tree
        .iter()
        .flat_map(|p| p.repositories.iter())
        .flat_map(|r| r.checkouts.iter())
        .next()
        .unwrap()
        .path
        .clone();
    let name = app.current_project().unwrap().name.clone();
    app.on_server_msg(argus_protocol::ServerMsg::Decisions(Box::new(
        argus_protocol::DecisionBoard {
            project: None,
            name,
            features: vec![argus_protocol::Feature {
                checkouts: vec![checkout],
                ..feature("pty", "Streaming the pty")
            }],
            decisions: Vec::new(),
        },
    )));
    press(&mut app, View::Feature.digit());

    let row = app.feature_rows().into_iter().next().unwrap();
    assert!(
        row.detail.contains("working") || row.detail.contains("idle"),
        "a feature says what its agents are doing: {}",
        row.detail
    );
}

#[test]
fn tab_crosses_the_three_panels_and_h_comes_back_to_the_list() {
    use crate::app::FeaturePanel;
    let mut app = app_with_features(vec![feature("notes", "Notes storage")], Vec::new());
    assert_eq!(app.panel, FeaturePanel::Features);

    app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(app.panel, FeaturePanel::Tasks);
    app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(app.panel, FeaturePanel::Decisions);
    app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(app.panel, FeaturePanel::Features, "and wraps");

    // `l` is a direction rather than a cycle: it stops at the last panel.
    for _ in 0..4 {
        app.on_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
    }
    assert_eq!(app.panel, FeaturePanel::Decisions);
    app.on_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
    assert_eq!(app.panel, FeaturePanel::Features);
}

#[test]
fn the_brief_the_tasks_and_the_decisions_are_all_the_feature_you_selected() {
    let mut notes = decision(1, None, "one row per note");
    notes.feature = Some("notes".into());
    let mut pty = decision(2, None, "one reader thread");
    pty.feature = Some("pty".into());
    let (mut app, mut rx) = feature_view_watching(vec![
        briefed("notes", "Notes storage", "keyed by path"),
        briefed("pty", "Streaming the pty", "one reader thread owns it"),
    ]);
    app.on_server_msg(argus_protocol::ServerMsg::Decisions(Box::new(
        argus_protocol::DecisionBoard {
            project: app.board.as_ref().and_then(|b| b.project),
            name: app.board.as_ref().unwrap().name.clone(),
            features: app.board.as_ref().unwrap().features.clone(),
            decisions: vec![notes, pty],
        },
    )));

    // Moving down the list re-asks for that feature's tasks, so the three
    // panels cannot end up describing different features — which is what
    // three views with a selection each used to do.
    while rx.try_recv().is_ok() {}
    app.on_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    let asked: Vec<String> = std::iter::from_fn(|| rx.try_recv().ok())
        .filter_map(|msg| match msg {
            argus_protocol::ClientMsg::GetTasks { feature, .. } => Some(feature),
            _ => None,
        })
        .collect();
    assert_eq!(asked, vec!["pty".to_string()]);

    let out = lines(&draw_at(&mut app, 120, 30)).join("\n");
    assert!(out.contains("one reader thread owns it"), "the brief: {out}");
    assert!(out.contains("one reader thread"), "the decisions: {out}");
    assert!(
        !out.contains("one row per note"),
        "and nothing from the feature above it: {out}"
    );
}

/// A feature view whose outgoing messages can be read back, which
/// `app_with_tree` deliberately throws away.
fn feature_view_watching(
    features: Vec<argus_protocol::Feature>,
) -> (App, tokio::sync::mpsc::UnboundedReceiver<argus_protocol::ClientMsg>) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let mut app = App::new(tx);
    app.on_server_msg(argus_protocol::ServerMsg::Tree(super::tree()));
    let project = app.current_project().unwrap();
    let (id, name) = (project.id, project.name.clone());
    app.on_server_msg(argus_protocol::ServerMsg::Decisions(Box::new(
        argus_protocol::DecisionBoard {
            project: Some(id),
            name,
            features,
            decisions: Vec::new(),
        },
    )));
    press(&mut app, View::Feature.digit());
    (app, rx)
}

fn moves(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<argus_protocol::ClientMsg>,
) -> Vec<(String, argus_protocol::FeatureState)> {
    let mut out = Vec::new();
    while let Ok(msg) = rx.try_recv() {
        if let argus_protocol::ClientMsg::MoveFeature { slug, state, .. } = msg {
            out.push((slug, state));
        }
    }
    out
}

#[test]
fn accepting_a_feature_is_one_key_and_it_goes_back() {
    use argus_protocol::FeatureState::*;
    let (mut app, mut rx) = feature_view_watching(vec![feature("notes", "Notes storage")]);
    let _ = moves(&mut rx);

    app.on_key(KeyEvent::new(KeyCode::Char('.'), KeyModifiers::NONE));
    assert_eq!(moves(&mut rx), vec![("notes".to_string(), Done)]);
    assert_eq!(
        app.selected_feature().map(|f| f.state),
        Some(Done),
        "applied here so the row answers the key; the push is what makes it true"
    );

    app.on_key(KeyEvent::new(KeyCode::Char('.'), KeyModifiers::NONE));
    assert_eq!(
        moves(&mut rx),
        vec![("notes".to_string(), Open)],
        "an acceptance made in error must not need a new feature"
    );
}

#[test]
fn there_is_nothing_to_accept_when_no_feature_is_selected() {
    let (mut app, mut rx) = feature_view_watching(Vec::new());
    let _ = moves(&mut rx);
    app.on_key(KeyEvent::new(KeyCode::Char('.'), KeyModifiers::NONE));
    assert!(moves(&mut rx).is_empty());
}

fn task(id: i64, title: &str, state: argus_protocol::TaskState) -> argus_protocol::Task {
    argus_protocol::Task {
        id,
        feature: "notes".into(),
        title: title.to_string(),
        state,
        claimed_by: None,
        external: None,
        position: id,
        at: 0,
        session: None,
    }
}

/// The feature view with the keys in its tasks, and its messages readable.
fn tasks_watching(
    tasks: Vec<argus_protocol::Task>,
) -> (App, tokio::sync::mpsc::UnboundedReceiver<argus_protocol::ClientMsg>) {
    let (mut app, rx) = feature_view_watching(vec![carded(
        "notes",
        "Notes storage",
        argus_protocol::FeatureState::Open,
    )]);
    app.on_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
    app.on_server_msg(argus_protocol::ServerMsg::Tasks(Box::new(
        argus_protocol::TaskList {
            project_name: "argus".into(),
            feature: Some("notes".into()),
            tasks,
        },
    )));
    (app, rx)
}

#[test]
fn a_task_is_one_row_with_its_state_marked_on_it() {
    use argus_protocol::TaskState::*;
    let (mut app, _rx) = tasks_watching(vec![
        task(1, "port the parser", Done),
        task(2, "wire the resize path", Doing),
        task(3, "backpressure", Todo),
    ]);

    let drawn = lines(&draw_at(&mut app, 120, 30));
    let out = drawn.join("\n");
    for title in ["port the parser", "wire the resize path", "backpressure"] {
        assert!(out.contains(title), "{out}");
    }
    for column in ["todo", "doing"] {
        assert!(
            !out.contains(&format!("{column} ·")),
            "the three columns are one list now: {out}"
        );
    }
    let row = |needle: &str| drawn.iter().position(|l| l.contains(needle)).unwrap();
    assert!(
        row("port the parser") < row("backpressure"),
        "the order a person put them in survives, which is what it is for"
    );
    assert!(out.contains("tasks · 1/3"), "how far along they are: {out}");
}

#[test]
fn moving_a_task_asks_the_daemon_and_follows_it_there() {
    use argus_protocol::TaskState::*;
    let (mut app, mut rx) = tasks_watching(vec![task(1, "port the parser", Todo)]);
    let _ = rx.try_recv();
    while rx.try_recv().is_ok() {}

    app.on_key(KeyEvent::new(KeyCode::Char('L'), KeyModifiers::NONE));
    let sent: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
    assert!(
        matches!(
            sent.as_slice(),
            [argus_protocol::ClientMsg::MoveTask { id: 1, state: Doing, .. }]
        ),
        "{sent:?}"
    );
    assert_eq!(
        app.selected_task().map(|t| (t.id, t.state)),
        Some((1, Doing)),
        "the mark under the cursor changes at once; the push is what makes it true"
    );
}

#[test]
fn a_task_can_be_pushed_up_the_list_and_dropped() {
    use argus_protocol::TaskState::*;
    let (mut app, mut rx) = tasks_watching(vec![
        task(1, "port the parser", Todo),
        task(2, "backpressure", Todo),
    ]);
    while rx.try_recv().is_ok() {}

    app.on_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    app.on_key(KeyEvent::new(KeyCode::Char('K'), KeyModifiers::NONE));
    let sent: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
    assert!(
        matches!(
            sent.as_slice(),
            [argus_protocol::ClientMsg::ReorderTask { id: 2, to: 1, .. }]
        ),
        "the human says what to do first: {sent:?}"
    );

    app.on_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
    let sent: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
    assert!(
        matches!(
            sent.as_slice(),
            [argus_protocol::ClientMsg::RemoveTask { id: 2, .. }]
        ),
        "{sent:?}"
    );
}

#[test]
fn another_features_task_list_is_dropped_rather_than_drawn() {
    use argus_protocol::TaskState::*;
    let (mut app, _rx) = tasks_watching(vec![task(1, "port the parser", Todo)]);
    app.on_server_msg(argus_protocol::ServerMsg::Tasks(Box::new(
        argus_protocol::TaskList {
            project_name: "argus".into(),
            feature: Some("the-pty".into()),
            tasks: vec![task(9, "one reader thread", Todo)],
        },
    )));
    let out = lines(&draw_at(&mut app, 120, 20)).join("\n");
    assert!(out.contains("port the parser"), "{out}");
    assert!(!out.contains("one reader thread"), "{out}");
}

#[test]
fn a_task_is_typed_in_on_a_line_of_its_own() {
    use argus_protocol::TaskState::*;
    let (mut app, mut rx) = tasks_watching(vec![task(1, "port the parser", Todo)]);
    while rx.try_recv().is_ok() {}

    app.on_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    for c in "xq back".chars() {
        app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    let out = lines(&draw_at(&mut app, 120, 20)).join("\n");
    assert!(out.contains("new task"), "{out}");
    assert!(out.contains("xq back"), "{out}");
    assert!(
        out.contains("port the parser"),
        "what is already there stays readable: {out}"
    );
    assert!(
        rx.try_recv().is_err(),
        "x and q were typed, not treated as drop and quit"
    );

    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let sent: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
    assert!(
        matches!(
            sent.as_slice(),
            [argus_protocol::ClientMsg::AddTask { write, .. }] if write.title == "xq back"
        ),
        "{sent:?}"
    );
    assert!(app.line.is_none(), "the line goes away once it is sent");
}

#[test]
fn rewriting_a_task_starts_from_what_it_says() {
    use argus_protocol::TaskState::*;
    let (mut app, mut rx) = tasks_watching(vec![task(1, "port the parser", Todo)]);
    while rx.try_recv().is_ok() {}

    app.on_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
    assert_eq!(
        app.line.as_ref().map(|i| i.text.as_str()),
        Some("port the parser"),
        "a correction is a few words off an existing line"
    );
    for c in " and its tests".chars() {
        app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let sent: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
    assert!(
        matches!(
            sent.as_slice(),
            [argus_protocol::ClientMsg::RetitleTask { id: 1, title, .. }]
                if title == "port the parser and its tests"
        ),
        "{sent:?}"
    );
}

#[test]
fn escape_abandons_the_line_rather_than_the_view() {
    use argus_protocol::TaskState::*;
    let (mut app, mut rx) = tasks_watching(vec![task(1, "port the parser", Todo)]);
    while rx.try_recv().is_ok() {}

    app.on_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    app.on_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE));
    app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.line.is_none());
    assert_eq!(app.view, View::Feature, "the first escape only put the line away");
    assert!(rx.try_recv().is_err(), "an abandoned line writes nothing");

    app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(app.view, View::Spine);
}

#[test]
fn each_panel_advertises_its_own_keys_rather_than_the_spines() {
    let mut app = app_with_features(
        vec![carded(
            "notes",
            "Notes storage",
            argus_protocol::FeatureState::Open,
        )],
        Vec::new(),
    );
    // The spine's own bar, for something to be different from.
    press(&mut app, '1');
    let spine = bar(&draw_at(&mut app, 130, 20));
    assert!(spine.contains("n add"), "the spine offers its own keys: {spine}");

    press(&mut app, View::Feature.digit());
    let features = bar(&draw_at(&mut app, 130, 20));
    assert!(features.contains("accept"), "{features}");

    app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    let tasks = bar(&draw_at(&mut app, 130, 20));
    assert!(tasks.contains("drop"), "{tasks}");

    app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    let decisions = bar(&draw_at(&mut app, 130, 20));
    assert!(decisions.contains("agents write this"), "{decisions}");

    for advertised in [&features, &tasks, &decisions] {
        assert!(
            !advertised.contains("n add"),
            "and none of them offers the spine's: {advertised}"
        );
    }
}

#[test]
fn the_bar_says_you_are_typing_while_a_task_is_being_written() {
    use argus_protocol::TaskState::*;
    let (mut app, _rx) = tasks_watching(vec![task(1, "port the parser", Todo)]);
    app.on_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    let typing = bar(&draw_at(&mut app, 120, 20));
    assert!(typing.contains("typing"), "{typing}");
    assert!(
        !typing.contains("x drop"),
        "a key that is being typed is not a key that is offered: {typing}"
    );
}

fn briefed(slug: &str, title: &str, body: &str) -> argus_protocol::Feature {
    argus_protocol::Feature {
        body: body.to_string(),
        ..feature(slug, title)
    }
}

#[test]
fn the_decision_view_reads_the_brief_above_the_reasoning() {
    let mut notes = decision(1, None, "one row per note");
    notes.feature = Some("notes".into());
    let mut app = app_with_features(
        vec![briefed(
            "notes",
            "Notes storage",
            "The key has to outlive the ids.",
        )],
        vec![notes],
    );
    press(&mut app, View::Feature.digit());

    let drawn = lines(&draw_at(&mut app, 100, 30));
    let out = drawn.join("\n");
    assert!(
        out.contains("The key has to outlive the ids"),
        "the brief is finally readable without an agent: {out}"
    );
    let row = |needle: &str| drawn.iter().position(|l| l.contains(needle)).unwrap();
    assert!(
        row("The key has to outlive") < row("one row per note"),
        "the brief comes first: a decision without it explains half of itself"
    );
}

#[test]
fn e_opens_the_brief_in_the_editor_and_saving_replaces_it() {
    let (mut app, mut rx) = feature_view_watching(vec![argus_protocol::Feature {
        body: "The reader thread owns the handle.".into(),
        ..carded("pty", "The pty", argus_protocol::FeatureState::Open)
    }]);
    while rx.try_recv().is_ok() {}

    app.on_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
    let view = app.notes.as_ref().expect("the brief is open");
    assert_eq!(view.brief.as_ref().map(|(_, slug)| slug.as_str()), Some("pty"));
    assert!(
        view.body().contains("The reader thread owns the handle"),
        "it opens on what is already written"
    );

    // Type a correction and close, which is what saves.
    app.on_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
    for c in " Not the writer.".chars() {
        app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    let sent: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
    assert!(
        sent.iter().any(|m| matches!(
            m,
            argus_protocol::ClientMsg::SetFeatureBody { slug, body, .. }
                if slug == "pty" && body.contains("Not the writer.")
        )),
        "a brief is replaced whole rather than appended to: {sent:?}"
    );
    assert!(
        !sent
            .iter()
            .any(|m| matches!(m, argus_protocol::ClientMsg::SetNote { .. })),
        "and never as a note: {sent:?}"
    );
}

#[test]
fn a_brief_is_not_a_note_and_says_so() {
    let (mut app, mut rx) = feature_view_watching(vec![argus_protocol::Feature {
        body: "- [ ] not a checkbox here".into(),
        ..carded("pty", "The pty", argus_protocol::FeatureState::Open)
    }]);
    while rx.try_recv().is_ok() {}
    app.on_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));

    app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    let sent: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
    assert!(
        !sent
            .iter()
            .any(|m| matches!(m, argus_protocol::ClientMsg::SetTodo { .. })),
        "ticking a brief must not write into the project's note: {sent:?}"
    );
}

#[test]
fn a_feature_can_be_written_down_from_the_board() {
    let (mut app, mut rx) = feature_view_watching(Vec::new());
    while rx.try_recv().is_ok() {}

    app.on_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    for c in "streaming the pty".chars() {
        app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    let out = lines(&draw_at(&mut app, 140, 20)).join("\n");
    assert!(out.contains("new feature"), "{out}");
    assert!(out.contains("streaming the pty"), "{out}");

    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let sent: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
    assert!(
        matches!(
            sent.as_slice(),
            [argus_protocol::ClientMsg::OpenFeature { write, .. }]
                if write.title == "streaming the pty"
        ),
        "a person can start a feature without an agent: {sent:?}"
    );
}

#[test]
fn renaming_a_feature_says_nothing_about_its_slug() {
    let (mut app, mut rx) = feature_view_watching(vec![carded(
        "notes",
        "Notes storage",
        argus_protocol::FeatureState::Open,
    )]);
    while rx.try_recv().is_ok() {}

    app.on_key(KeyEvent::new(KeyCode::Char('R'), KeyModifiers::NONE));
    assert_eq!(
        app.line.as_ref().map(|i| i.text.as_str()),
        Some("Notes storage"),
        "a rename starts from what it is called"
    );
    for c in " and context".chars() {
        app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let sent: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
    assert!(
        matches!(
            sent.as_slice(),
            [argus_protocol::ClientMsg::RenameFeature { slug, title, .. }]
                if slug == "notes" && title == "Notes storage and context"
        ),
        "the slug travels unchanged, since the work points at it: {sent:?}"
    );
}

#[test]
fn a_feature_can_be_removed_from_the_board() {
    let (mut app, mut rx) = feature_view_watching(vec![carded(
        "notes",
        "Notes storage",
        argus_protocol::FeatureState::Open,
    )]);
    while rx.try_recv().is_ok() {}

    app.on_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
    let sent: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
    assert!(
        matches!(
            sent.as_slice(),
            [argus_protocol::ClientMsg::RemoveFeature { slug, .. }] if slug == "notes"
        ),
        "{sent:?}"
    );
}

#[test]
fn typing_a_feature_name_does_not_work_the_board_underneath() {
    let (mut app, mut rx) = feature_view_watching(vec![carded(
        "notes",
        "Notes storage",
        argus_protocol::FeatureState::Open,
    )]);
    while rx.try_recv().is_ok() {}

    app.on_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    for c in "xqsL".chars() {
        app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    assert!(
        rx.try_recv().is_err(),
        "x, q, s and L were typed, not treated as drop, quit, send back and move"
    );
    assert_eq!(app.line.as_ref().map(|i| i.text.as_str()), Some("xqsL"));

    app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(app.view, View::Feature, "the first escape only put the line away");
}

#[test]
fn a_working_pane_turns_and_a_settled_one_does_not() {
    let mut app = app_with_tree();
    let epoch = app.epoch();

    let glyphs: Vec<String> = (0..4)
        .map(|frame| {
            app.set_frame_now(epoch + crate::motion::SPINNER_FRAME * frame);
            let text = lines(&draw(&mut app)).join("\n");
            // The pane's own row, reduced to its status cell. Not the live
            // view's title, which spells the same name out across the
            // breadcrumb and carries no glyph of its own.
            text.lines()
                .find(|l| l.contains("claude") && !l.contains('\u{203a}'))
                .expect("the working pane has a row")
                .chars()
                .find(|c| "⠋⠙⠹⠸⠼⠴⠦⠧●○".contains(*c))
                .expect("the row carries a status glyph")
                .to_string()
        })
        .collect();

    let distinct: std::collections::HashSet<&String> = glyphs.iter().collect();
    assert_eq!(
        distinct.len(),
        4,
        "four consecutive frames should show four glyphs, not {glyphs:?}"
    );

    // The idle pane beside it is unmoved by any of that: motion marks the
    // one state that is ongoing, and would say nothing if everything had it.
    let idle: Vec<String> = (0..4)
        .map(|frame| {
            app.set_frame_now(epoch + crate::motion::SPINNER_FRAME * frame);
            let text = lines(&draw(&mut app)).join("\n");
            text.lines()
                .find(|l| l.contains("shell"))
                .expect("the idle pane has a row")
                .to_string()
        })
        .collect();
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

/// A decision whose reasoning is far wider than any column.
fn a_long_decision(id: i64) -> argus_protocol::Decision {
    argus_protocol::Decision {
        over: Some("the obvious alternative that somebody would otherwise reach for".into()),
        because: Some(
            "the key has to outlive the ids, which are handed out fresh on every start, \
             and a row that renumbers underneath a board is the one thing a board cannot take"
                .into(),
        ),
        ..decision(id, None, "a choice whose title is itself long enough to run past the card edge")
    }
}

#[test]
fn the_decision_you_are_on_shows_all_of_itself() {
    let mut app = app_with_a_board(vec![a_long_decision(1), decision(2, None, "short")]);
    let out = lines(&draw_at(&mut app, 100, 30));
    let body = out.join("\n");

    // Nothing is clipped away: the tail of the reasoning is on screen.
    assert!(
        body.contains("cannot take"),
        "the selected decision should show its whole reason:\n{body}"
    );
    // And it wrapped rather than overflowing the card.
    assert!(
        out.iter().all(|l| l.chars().count() <= 100),
        "no line may run past the terminal"
    );
}

#[test]
fn the_decisions_you_are_not_on_stay_one_row_each() {
    // Two long decisions, the cursor on the first. The second keeps its
    // two lines, or the list stops being something you can scan.
    let mut app = app_with_a_board(vec![a_long_decision(1), a_long_decision(2)]);
    let out = lines(&draw_at(&mut app, 100, 30));
    let second = out
        .iter()
        .position(|l| l.contains("#2"))
        .expect("the second decision is drawn");

    assert!(
        !out[second].contains("cannot take"),
        "an unselected row is not expanded: {:?}",
        out[second]
    );
    assert_eq!(
        out[second + 1..]
            .iter()
            .filter(|l| l.contains("cannot take"))
            .count(),
        0,
        "and nothing of its detail wrapped either"
    );
}

#[test]
fn moving_the_cursor_moves_which_decision_is_expanded() {
    let mut app = app_with_a_board(vec![a_long_decision(1), a_long_decision(2)]);
    let before = lines(&draw_at(&mut app, 100, 30));
    let expanded_before = before.iter().filter(|l| l.contains("cannot take")).count();

    press(&mut app, 'j');
    let after = lines(&draw_at(&mut app, 100, 30));
    let expanded_after = after.iter().filter(|l| l.contains("cannot take")).count();

    assert_eq!(expanded_before, 1, "exactly one row is expanded at a time");
    assert_eq!(expanded_after, 1);
    assert_ne!(before, after, "and it is a different one after moving");
}

#[test]
fn an_expanded_row_does_not_push_itself_off_the_bottom() {
    // Enough decisions that the list scrolls, with the cursor at the end:
    // the row being read has to be wholly on screen, not just started.
    let mut many: Vec<_> = (1..=20).map(|id| decision(id, None, "a choice")).collect();
    many.push(a_long_decision(21));
    let mut app = app_with_a_board(many);
    // The view opens on the features column beside the tree; `l` is what
    // moves onto the decisions themselves.
    press(&mut app, 'l');
    // Into the tasks, then into the tree, which is what `j` then walks.
    press(&mut app, 'l');
    press(&mut app, 'l');
    for _ in 0..20 {
        press(&mut app, 'j');
    }
    let out = lines(&draw_at(&mut app, 100, 30));
    let body = out.join("\n");

    assert!(
        body.contains("#21"),
        "the selected row is on screen:\n{body}"
    );
    assert!(
        body.contains("cannot take"),
        "and so is the end of what it says:\n{body}"
    );
}

const LONG_TITLE: &str =
    "rewrite the checkout picker so it remembers the directory you came from";

#[test]
fn the_task_you_are_on_shows_its_whole_title() {
    use argus_protocol::TaskState::*;
    let (mut app, _rx) = tasks_watching(vec![
        task(1, LONG_TITLE, Todo),
        task(2, LONG_TITLE, Todo),
    ]);

    // Narrow enough that the title has to wrap; at a width where it fits
    // on one row there is nothing for expansion to do.
    let out = lines(&draw_at(&mut app, 80, 24));
    let body = out.join("\n");

    assert!(
        body.contains("came from"),
        "the selected task should show all of its title:\n{body}"
    );
    assert_eq!(
        out.iter().filter(|l| l.contains("came from")).count(),
        1,
        "and only the selected one:\n{body}"
    );
    assert!(
        out.iter().all(|l| l.chars().count() <= 80),
        "nothing runs past the terminal"
    );
}

#[test]
fn the_card_you_are_on_shows_its_whole_title() {
    let (mut app, _rx) = feature_view_watching(vec![carded(
        "notes",
        LONG_TITLE,
        argus_protocol::FeatureState::Open,
    )]);

    let out = lines(&draw_at(&mut app, 120, 20));
    let body = out.join("\n");

    assert!(
        body.contains("came from"),
        "the selected card should show all of its title:\n{body}"
    );
    assert!(out.iter().all(|l| l.chars().count() <= 120));
}

/// Not an assertion — a way to look at the feature view while working on
/// it: `cargo test -p argus dump_feature -- --ignored --nocapture`.
#[test]
#[ignore = "prints a frame for eyeballing; asserts nothing"]
fn dump_feature() {
    let mut auth = briefed(
        "auth",
        "Auth rewrite",
        "Replace the session cookie with a signed token. Rotation is out of scope; \
         the refresh path stays where it is.",
    );
    auth.tasks = argus_protocol::TaskCounts {
        todo: 3,
        doing: 1,
        done: 3,
    };
    let mut billing = feature("billing", "Billing retry");
    billing.state = argus_protocol::FeatureState::Done;
    billing.tasks = argus_protocol::TaskCounts {
        todo: 0,
        doing: 0,
        done: 7,
    };

    let mut chose = decision(1, None, "sign with ed25519");
    chose.over = Some("HMAC".into());
    chose.because = Some("key rotation".into());
    chose.feature = Some("auth".into());
    let mut under = decision(2, Some(1), "store the key id with the pane");
    under.feature = Some("auth".into());

    let mut app = app_with_features(
        vec![auth, billing, feature("tls", "TLS expiry")],
        vec![chose, under],
    );
    app.on_server_msg(argus_protocol::ServerMsg::Tasks(Box::new(
        argus_protocol::TaskList {
            project_name: "argus".into(),
            feature: Some("auth".into()),
            tasks: vec![
                argus_protocol::Task {
                    feature: "auth".into(),
                    claimed_by: Some("sess-1".into()),
                    ..task(1, "carry the token through the pump", argus_protocol::TaskState::Doing)
                },
                argus_protocol::Task {
                    feature: "auth".into(),
                    external: Some("ORION-412".into()),
                    ..task(2, "test reconnect after a daemon restart", argus_protocol::TaskState::Todo)
                },
                argus_protocol::Task {
                    feature: "auth".into(),
                    ..task(3, "pick a signing algorithm", argus_protocol::TaskState::Done)
                },
            ],
        },
    )));
    for line in lines(&draw_at(&mut app, 110, 30)) {
        println!("|{line}");
    }
}
