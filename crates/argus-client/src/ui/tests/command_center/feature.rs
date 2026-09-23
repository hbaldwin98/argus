//! The Feature stage: wrapped rows, tree guides and right-aligned ids.

use super::*;

fn feature_document_with_long_rows() -> App {
    let mut app = command_center();
    app.on_server_msg(argus_protocol::ServerMsg::Decisions(Box::new(
        argus_protocol::DecisionBoard {
            project: None,
            name: "argus".into(),
            features: vec![argus_protocol::Feature {
                slug: "focus".into(),
                title: "Focused content".into(),
                body: "A short brief.".into(),
                origin_checkout: None,
                origin_branch: Some("main".into()),
                at: 0,
                session: None,
                state: argus_protocol::FeatureState::Open,
                checkouts: Vec::new(),
                tasks: Default::default(),
            }],
            decisions: vec![
                argus_protocol::Decision {
                    id: 1,
                    parent: None,
                    at: 0,
                    session: None,
                    checkout: None,
                    feature: Some("focus".into()),
                    chose: "Use the focused decision layout".into(),
                    over: Some("the permanently verbose layout".into()),
                    because: Some(
                        "The full decision reasoning appears only while this decision is focused and "
                            .to_string()
                            + "decision rationale remains visible after wrapping.",
                    ),
                    superseded_by: None,
                },
                argus_protocol::Decision {
                    id: 2,
                    parent: None,
                    at: 0,
                    session: None,
                    checkout: None,
                    feature: Some("focus".into()),
                    chose: "Keep the second decision compact".into(),
                    over: None,
                    because: None,
                    superseded_by: None,
                },
            ],
        },
    )));
    app.open_view(View::Feature);
    app.on_server_msg(argus_protocol::ServerMsg::Tasks(Box::new(
        argus_protocol::TaskList {
            project_name: "argus".into(),
            feature: Some("focus".into()),
            tasks: vec![
                argus_protocol::Task {
                    id: 1,
                    feature: "focus".into(),
                    parent: None,
                    title: "Carry the parser through the complete resize boundary".into(),
                    body: Some(
                        "The full task description appears only while this task is focused and "
                            .to_string()
                            + "task prose is visible after wrapping.",
                    ),
                    state: argus_protocol::TaskState::Todo,
                    claimed_by: None,
                    external: None,
                    position: 0,
                    at: 0,
                    session: None,
                },
                argus_protocol::Task {
                    id: 2,
                    feature: "focus".into(),
                    parent: None,
                    title: "second".into(),
                    body: None,
                    state: argus_protocol::TaskState::Todo,
                    claimed_by: None,
                    external: None,
                    position: 1,
                    at: 0,
                    session: None,
                },
            ],
        },
    )));
    app
}

#[test]
fn focused_feature_rows_show_wrapped_text_without_expanding_every_row() {
    let mut app = feature_document_with_long_rows();
    let compact = lines(&draw_at(&mut app, 80, 96)).join("\n");
    assert!(!compact.contains("full task description"), "{compact}");
    assert!(!compact.contains("full decision reasoning"), "{compact}");
    assert!(!compact.contains("permanently verbose"), "{compact}");

    let tasks = app.layout.feature_tasks.inner;
    click(&mut app, tasks.x + 1, tasks.y);
    let focused_task_lines = lines(&draw_at(&mut app, 80, 96));
    let focused_task = focused_task_lines.join("\n");
    assert!(focused_task.contains("boundary"), "{focused_task}");
    assert!(
        focused_task.contains("task prose")
            && focused_task.contains("visible after")
            && focused_task.contains("wrapping"),
        "{focused_task}"
    );

    let second_task_y = focused_task_lines
        .iter()
        .position(|line| line.contains("second"))
        .expect("the compact task remains visible") as u16;
    click(&mut app, tasks.x + 1, second_task_y);
    assert_eq!(app.task_sel, 1, "clicking past an expanded task selects the row");

    draw_at(&mut app, 80, 96);
    app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    app.decision_sel = 0;
    let focused_decision_lines = lines(&draw_at(&mut app, 80, 96));
    let focused_decision = focused_decision_lines.join("\n");
    assert!(
        focused_decision.contains("rationale")
            && focused_decision.contains("remains")
            && focused_decision.contains("visible")
            && focused_decision.contains("wrapping"),
        "{focused_decision}"
    );
    assert_eq!(app.panel, FeaturePanel::Decisions);
    app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(
        app.decision_sel, 1,
        "moving past an expanded decision selects the next row"
    );
}

#[test]
fn feature_decisions_show_tree_guides_and_right_aligned_ids() {
    let mut app = command_center();
    app.on_server_msg(argus_protocol::ServerMsg::Decisions(Box::new(
        argus_protocol::DecisionBoard {
            project: None,
            name: "argus".into(),
            features: vec![argus_protocol::Feature {
                slug: "tree".into(),
                title: "Tree".into(),
                body: String::new(),
                origin_checkout: None,
                origin_branch: Some("main".into()),
                at: 0,
                session: None,
                state: argus_protocol::FeatureState::Open,
                checkouts: Vec::new(),
                tasks: Default::default(),
            }],
            decisions: vec![
                argus_protocol::Decision {
                    id: 10,
                    parent: None,
                    at: 0,
                    session: None,
                    checkout: None,
                    feature: Some("tree".into()),
                    chose: "root decision".into(),
                    over: None,
                    because: None,
                    superseded_by: None,
                },
                argus_protocol::Decision {
                    id: 11,
                    parent: Some(10),
                    at: 0,
                    session: None,
                    checkout: None,
                    feature: Some("tree".into()),
                    chose: "child decision".into(),
                    over: None,
                    because: None,
                    superseded_by: None,
                },
            ],
        },
    )));
    app.open_view(View::Feature);
    let text = lines(&draw_at(&mut app, 80, 40)).join("\n");
    assert!(
        text.contains('◇') && text.contains("root dec"),
        "root decisions should show the root mark:\n{text}"
    );
    assert!(
        text.contains('└') && text.contains("child dec"),
        "nested decisions should render branch guides:\n{text}"
    );
    for line in text.lines().filter(|line| line.contains("decision")) {
        let row = line.split('│').next_back().unwrap_or(line).trim();
        let Some(hash) = row.rfind('#') else {
            continue;
        };
        let id_tail = row[hash + 1..].trim();
        assert!(
            !id_tail.is_empty() && id_tail.chars().all(|c| c.is_ascii_digit()),
            "decision id should sit on the right: {row:?}"
        );
    }
}

#[test]
fn focused_decision_keeps_id_on_the_right() {
    let mut app = feature_document_with_long_rows();
    let decisions = app.layout.feature_decisions.inner;
    click(&mut app, decisions.x + 1, decisions.y);
    let width = decisions.width as usize;
    for line in lines(&draw_at(&mut app, 80, 60)) {
        if !line.contains('#') {
            continue;
        }
        let visible: String = line.chars().take(width).collect();
        if visible.contains("no alternative") {
            let trimmed = visible.trim_end();
            assert!(
                trimmed.ends_with('1') || trimmed.ends_with("#    1"),
                "decision id should stay right-aligned on the detail row: {visible:?}"
            );
        }
    }
}

#[test]
fn focused_task_keeps_id_on_the_right() {
    let mut app = feature_document_with_long_rows();
    let tasks = app.layout.feature_tasks.inner;
    click(&mut app, tasks.x + 1, tasks.y);
    let width = tasks.width as usize;
    for line in lines(&draw_at(&mut app, 80, 60)) {
        if !line.contains('#') {
            continue;
        }
        let visible: String = line.chars().take(width).collect();
        if visible.contains("#") && visible.contains("1") {
            let trimmed = visible.trim_end();
            assert!(
                trimmed.ends_with('1') || trimmed.ends_with("#1"),
                "task id should stay right-aligned when expanded: {visible:?}"
            );
        }
    }
}
