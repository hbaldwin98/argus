//! Regression coverage for the HTML command-center composition.

use super::*;
use crate::app::FeaturePanel;

fn command_center() -> App {
    let mut app = app_with_tree();
    app.command_center = true;
    app
}

#[test]
fn production_shell_matches_the_designs_four_regions() {
    let mut app = command_center();
    let text = lines(&draw_at(&mut app, 120, 30)).join("\n");

    for landmark in [
        "ARGUS",
        "WORKSPACE",
        "ACTIVE WORKSPACE",
        "REPOSITORIES",
        "AGENTS",
    ] {
        assert!(text.contains(landmark), "missing {landmark}:\n{text}");
    }
    assert!(
        !text.contains("└ master"),
        "repositories expand only after a click:\n{text}"
    );
    assert_eq!(app.layout.projects.outer.x, 0);
    assert_eq!(
        app.layout.projects.outer.width,
        crate::ui::command_center::SIDEBAR_WIDTH
    );
    assert!(
        app.layout.content.outer.x >= crate::ui::command_center::SIDEBAR_WIDTH
    );
}

#[test]
fn workspace_summary_badges_stay_on_one_row() {
    let mut app = command_center();
    app.tree[0].repositories[0].checkouts[0].panes[1].status = PaneStatus::Waiting;
    app.tree[0].repositories.push(repository(
        9,
        "satellite",
        vec![checkout(20, "main", true, vec![])],
    ));
    for width in [80, 120] {
        let text = lines(&draw_at(&mut app, width, 30));
        assert!(
            text.iter().any(|line| {
                line.contains("REPOS") && line.contains("AGENTS") && line.contains("NEED")
            }),
            "badges wrapped at width {width}:\n{}",
            text.join("\n")
        );
    }
}

#[test]
fn workspace_summary_badges_stay_on_one_row_with_two_digit_counts() {
    let mut app = command_center();
    app.tree[0].repositories[0].checkouts[0].panes[1].status = PaneStatus::Waiting;
    for i in 0..12 {
        let name = format!("repo-{i}");
        app.tree[0].repositories.push(repository(
            20 + i,
            &name,
            vec![checkout(30 + i, "main", false, vec![])],
        ));
    }
    let text = lines(&draw_at(&mut app, 80, 30)).join("\n");
    assert!(
        text.lines().any(|line| {
            line.contains("13 REPOS") && line.contains("AGENTS") && line.contains("NEED")
        }),
        "two-digit badge row wrapped:\n{text}"
    );
}

#[test]
fn pane_overview_is_a_real_top_level_surface() {
    let mut app = command_center();
    app.open_view(View::Panes);
    let text = lines(&draw_at(&mut app, 120, 30)).join("\n");

    assert!(text.contains("repository panes"), "{text}");
    assert!(text.contains("claude"), "{text}");
    assert!(text.contains("shell"), "{text}");
}

#[test]
fn checkout_overview_uses_the_designs_operational_table() {
    let mut app = command_center();
    app.open_view(View::Checkouts);
    // The wide table needs enough stage width once the rail is drawn.
    let text = lines(&draw_at(&mut app, 140, 30)).join("\n");

    assert!(text.contains("checkouts & worktrees"), "{text}");
    for heading in ["BRANCH", "STATE", "PANES", "PATH"] {
        assert!(text.contains(heading), "missing {heading}:\n{text}");
    }
    assert!(text.contains("B BRANCHES · m CHECKOUT · n WORKTREE"), "{text}");
    assert!(text.contains("master"), "{text}");
}

#[test]
fn active_repositories_sort_before_inactive_repositories() {
    let mut app = command_center();
    app.tree[0].repositories.insert(
        0,
        repository(3, "sleeping", vec![checkout(12, "quiet", false, vec![])]),
    );
    app.sel_repository = 1;
    let text = lines(&draw_at(&mut app, 120, 30)).join("\n");

    assert!(
        text.find("orion").unwrap() < text.find("sleeping").unwrap(),
        "{text}"
    );
}

#[test]
fn b_lists_branch_only_rows_in_the_checkouts_table() {
    let mut app = command_center();
    app.tree[0].repositories[0].branches = vec!["spike".into()];
    app.open_view(View::Checkouts);
    let hidden = lines(&draw_at(&mut app, 120, 30)).join("\n");
    assert!(
        !hidden.contains("spike"),
        "branch-only rows stay out until expanded:\n{hidden}"
    );

    app.on_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('B'),
        crossterm::event::KeyModifiers::NONE,
    ));
    let shown = lines(&draw_at(&mut app, 120, 30)).join("\n");
    assert!(shown.contains("spike"), "{shown}");
    assert!(shown.contains("no checkout"), "{shown}");
}

#[test]
fn a_long_checkouts_table_scrolls_with_the_selection() {
    let mut app = command_center();
    {
        let r = &mut app.tree[0].repositories[0];
        r.checkouts = (0..12)
            .map(|i| argus_protocol::CheckoutInfo {
                id: argus_protocol::CheckoutId(20 + i),
                name: format!("branch-{i:02}"),
                path: format!("/repo/wt-{i}"),
                primary: i == 0,
                git: None,
                panes: Vec::new(),
            })
            .collect();
    }
    app.open_view(View::Checkouts);
    app.sel_checkout = app.checkout_table_row_indices().len() - 1;
    draw_at(&mut app, 120, 24);
    assert!(
        app.layout.checkouts.first > 0,
        "the table should scroll when the selection is below the fold"
    );
}

#[test]
fn slash_filters_checkouts_by_branch_name() {
    let mut app = command_center();
    {
        let r = &mut app.tree[0].repositories[0];
        r.checkouts = vec![
            checkout(20, "alpha", true, vec![]),
            checkout(21, "beta", false, vec![]),
            checkout(22, "alphabet", false, vec![]),
        ];
    }
    app.open_view(View::Checkouts);
    app.on_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('/'),
        crossterm::event::KeyModifiers::NONE,
    ));
    app.on_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('a'),
        crossterm::event::KeyModifiers::NONE,
    ));
    app.on_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('l'),
        crossterm::event::KeyModifiers::NONE,
    ));
    app.on_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('p'),
        crossterm::event::KeyModifiers::NONE,
    ));
    app.on_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    ));
    let indices = app.checkout_table_row_indices();
    assert_eq!(indices.len(), 2, "alpha and alphabet match");
    let text = lines(&draw_at(&mut app, 120, 30)).join("\n");
    assert!(text.contains("alpha"), "{text}");
    assert!(text.contains("alphabet"), "{text}");
    assert!(!text.contains("beta"), "{text}");
}

#[test]
fn clicking_a_checkout_row_selects_the_row_under_the_pointer() {
    // A default branch with no directory gets a navigation row of its own
    // that the table does not draw; clicks used to land one row short.
    let mut app = command_center();
    {
        let repo = &mut app.tree[0].repositories[0];
        repo.default_branch = Some("main".into());
        repo.branches = vec!["main".into()];
    }
    app.open_view(View::Checkouts);
    let text = lines(&draw_at(&mut app, 120, 30));
    let rows = app.layout.checkouts.inner;
    for (index, name) in ["master", "feat"].iter().enumerate() {
        let top = rows.y + index as u16 * 3;
        // Every line of the row is part of it, and its text is on the middle one.
        assert!(text[top as usize + 1].contains(name), "{}", text.join("\n"));
        for y in top..top + 3 {
            click(&mut app, rows.x + 4, y);
            assert_eq!(
                app.current_checkout().map(|c| c.name.as_str()),
                Some(*name),
                "click on line {y}"
            );
        }
    }
}

#[test]
fn repository_rail_expands_active_branches_and_panes_after_click() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

    let mut app = command_center();
    draw_at(&mut app, 120, 30);
    let row = app.layout.projects.inner.y;
    app.on_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: app.layout.projects.inner.x + 2,
        row,
        modifiers: KeyModifiers::NONE,
    });
    let text = lines(&draw_at(&mut app, 120, 30)).join("\n");

    assert!(text.contains("└ master"), "{text}");
    // A working agent shows the spinner in place of its still mark.
    assert!(
        text.lines()
            .any(|line| line.contains("└ ") && line.contains("claude") && line.contains("RUNNING")),
        "{text}"
    );
    assert!(text.contains("└ › shell"), "{text}");
}

#[test]
fn current_pane_is_highlighted_in_an_expanded_repository() {
    let mut app = command_center();
    app.focus = Focus::PaneContent;
    let text = lines(&draw_at(&mut app, 120, 30)).join("\n");

    assert!(
        text.lines()
            .any(|line| line.contains("▌      └ ") && line.contains("claude")),
        "{text}"
    );
}

#[test]
fn clicking_a_pane_card_opens_that_exact_pane() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

    let mut app = command_center();
    app.open_view(View::Panes);
    draw_at(&mut app, 120, 30);
    let card = app.layout.panes.inner;

    app.on_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: card.x + 1,
        row: card.y + 1,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(app.view, View::Spine);
    assert_eq!(app.current_pane().map(|pane| pane.id), Some(PaneId(100)));
    assert_eq!(app.focus, Focus::PaneContent);
}

#[test]
fn pane_overview_defaults_to_the_current_repository() {
    let mut app = command_center();
    app.tree[0].repositories.push(repository(
        3,
        "elsewhere",
        vec![checkout(
            12,
            "other",
            false,
            vec![pane_info(
                102,
                PaneKind::Agent,
                "other-agent",
                PaneStatus::Working,
            )],
        )],
    ));
    app.open_view(View::Panes);

    draw_at(&mut app, 120, 30);
    assert_eq!(app.overview_pane_locations().len(), 2);

    app.show_all_panes = true;
    draw_at(&mut app, 120, 30);
    assert_eq!(app.overview_pane_locations().len(), 3);
}

#[test]
fn first_run_replaces_the_shell_when_no_tree_exists() {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    std::mem::forget(rx);
    let mut app = App::new(tx);
    let text = lines(&draw_at(&mut app, 100, 24)).join("\n");

    assert!(text.contains("NO WORKSPACE"), "{text}");
    assert!(text.contains("Point Argus at a directory"), "{text}");
    assert_eq!(app.layout.projects.outer, Rect::default());
}

fn click(app: &mut App, column: u16, row: u16) {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    app.on_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    });
}

#[test]
fn clicking_a_repository_collapses_the_one_open_before_it() {
    let mut app = command_center();
    app.tree[0].repositories.push(repository(
        3,
        "second",
        vec![checkout(
            12,
            "trunk",
            false,
            vec![pane_info(102, PaneKind::Shell, "zsh", PaneStatus::Idle)],
        )],
    ));
    draw_at(&mut app, 120, 30);
    let x = app.layout.projects.inner.x + 2;
    let y = app.layout.projects.inner.y;
    click(&mut app, x, y);
    let text = lines(&draw_at(&mut app, 120, 30)).join("\n");
    assert!(
        text.contains("└ master") && !text.contains("└ trunk"),
        "{text}"
    );

    let second = text
        .lines()
        .position(|line| line.contains("second"))
        .expect("second repository row") as u16;
    click(&mut app, x, second);
    let text = lines(&draw_at(&mut app, 120, 30)).join("\n");
    assert!(
        text.contains("└ trunk") && !text.contains("└ master"),
        "{text}"
    );
}

#[test]
fn clicking_a_pane_in_the_rail_keeps_keys_on_the_rail_so_x_closes_it() {
    let mut app = command_center();
    draw_at(&mut app, 120, 30);
    let x = app.layout.projects.inner.x + 2;
    let y = app.layout.projects.inner.y;
    click(&mut app, x, y);
    let text = lines(&draw_at(&mut app, 120, 30)).join("\n");
    let row = text
        .lines()
        .position(|line| line.contains("└ ") && line.contains("claude"))
        .expect("pane row") as u16;
    click(&mut app, x, row);

    assert_eq!(app.current_pane().map(|pane| pane.id), Some(PaneId(100)));
    assert_eq!(app.focus, Focus::Panes);
}

#[test]
fn the_open_tabs_accent_bar_is_not_cut_by_the_rail_junction() {
    let mut app = command_center();
    app.open_view(View::Feature);
    let rows = lines(&draw_at(&mut app, 120, 30));
    let rule = rows
        .iter()
        .find(|line| line.contains('━'))
        .expect("tab rule");
    let bar: String = rule
        .chars()
        .skip_while(|c| *c != '━')
        .take_while(|c| *c != '─')
        .collect();
    assert!(
        !rule[rule.find('━').unwrap()..]
            .trim_end_matches('─')
            .contains('┬'),
        "{rule}"
    );
    assert!(bar.chars().count() >= 7, "{rule}");
}

#[test]
fn a_long_checkout_path_keeps_its_end() {
    let mut app = command_center();
    app.tree[0].repositories[0].checkouts[0].path =
        "/very/long/prefix/that/will/not/fit/in/the/column/at/all/final-segment".into();
    app.open_view(View::Checkouts);
    let text = lines(&draw_at(&mut app, 120, 30)).join("\n");
    assert!(text.contains("final-segment"), "{text}");
}

#[test]
fn the_shell_leaves_a_row_above_the_tabs_and_below_the_status_band() {
    let mut app = command_center();
    let rows = lines(&draw_at(&mut app, 120, 30));
    assert!(rows[0].trim().is_empty(), "{rows:?}");
    assert!(rows[29].trim().is_empty(), "{rows:?}");
}

#[test]
fn clicking_an_agent_in_the_agents_list_goes_straight_to_it() {
    let mut app = command_center();
    app.focus = Focus::PaneContent;
    draw_at(&mut app, 140, 40);
    let agents = app.layout.agents.inner;
    app.focus = Focus::Repositories;
    click(&mut app, agents.x + 2, agents.y);

    assert_eq!(app.view, View::Spine);
    assert_eq!(app.current_pane().map(|pane| pane.id), Some(PaneId(100)));
    assert_eq!(app.focus, Focus::Panes);
}

#[test]
fn the_rail_border_continues_the_feature_tabs_right_edge() {
    let mut app = command_center();
    let rows = lines(&draw_at(&mut app, 140, 40));
    let tabs: Vec<char> = rows[1].chars().collect();
    let feature = rows[1].find("FEATURE").unwrap();
    let feature = rows[1][..feature].chars().count() + "FEATURE".len() + 2;
    let border = app.layout.projects.outer.right() as usize - 1;
    assert_eq!(feature - 1, border, "{}\n{:?}", rows.join("\n"), tabs);
}

#[test]
fn the_rail_switches_projects_through_the_project_picker() {
    let mut app = command_center();
    app.tree.push(project(
        4,
        "hermes",
        vec![repository(
            5,
            "courier",
            vec![checkout(
                30,
                "main",
                true,
                vec![pane_info(300, PaneKind::Shell, "zsh", PaneStatus::Idle)],
            )],
        )],
    ));
    let text = lines(&draw_at(&mut app, 120, 30)).join("\n");
    assert!(text.contains("argus 1/2"), "{text}");

    // Clicking the project name opens the picker on the current project.
    let outer = app.layout.projects.outer;
    click(&mut app, outer.x + 4, outer.y + 2);
    assert!(app.picker.is_some());
    app.picker = None;

    app.on_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
    app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.sel_project, 1);
    let text = lines(&draw_at(&mut app, 120, 30)).join("\n");
    assert!(
        text.contains("hermes 2/2") && text.contains("courier"),
        "{text}"
    );
}

#[test]
fn the_workspace_header_and_pane_cards_show_agent_telemetry() {
    let mut app = command_center();
    {
        let pane = &mut app.tree[0].repositories[0].checkouts[0].panes[0];
        pane.telemetry = argus_protocol::AgentTelemetry {
            model: Some("gpt-5-codex".into()),
            context_tokens: Some(41_203),
            context_window: Some(272_000),
            cost_usd: Some(0.5),
            tool: Some("Bash".into()),
            tool_calls: Some(3),
            ..Default::default()
        };
    }
    app.select_pane_location(app.flat_pane_locations()[0]);
    let text = lines(&draw_at(&mut app, 160, 30)).join("\n");
    assert!(
        text.contains("▸ Bash · ctx 41k/272k 15% · $0.50 · gpt-5-codex"),
        "{text}"
    );

    app.open_view(View::Panes);
    let text = lines(&draw_at(&mut app, 160, 30)).join("\n");
    assert!(text.contains("▸ Bash · ctx 41k/272k"), "{text}");
}

#[test]
fn token_counts_read_compactly() {
    use crate::ui::command_center::compact_tokens;
    assert_eq!(compact_tokens(980), "980");
    assert_eq!(compact_tokens(4_250), "4.2k");
    assert_eq!(compact_tokens(41_203), "41k");
    assert_eq!(compact_tokens(1_200_000), "1.2M");
}

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
    let compact = lines(&draw_at(&mut app, 80, 60)).join("\n");
    assert!(!compact.contains("full task description"), "{compact}");
    assert!(!compact.contains("full decision reasoning"), "{compact}");
    assert!(!compact.contains("permanently verbose"), "{compact}");

    let tasks = app.layout.feature_tasks.inner;
    click(&mut app, tasks.x + 1, tasks.y);
    let focused_task_lines = lines(&draw_at(&mut app, 80, 60));
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

    let decisions = app.layout.feature_decisions.inner;
    click(&mut app, decisions.x + 1, decisions.y);
    let focused_decision_lines = lines(&draw_at(&mut app, 80, 60));
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
        text.contains('└') && text.contains("child decis"),
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
