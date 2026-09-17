//! Regression coverage for the HTML command-center composition.

use super::*;

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
    assert_eq!(app.layout.projects.outer.width, 35);
    assert!(app.layout.content.outer.x >= 35);
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
    let text = lines(&draw_at(&mut app, 120, 30)).join("\n");

    assert!(text.contains("checkouts & worktrees"), "{text}");
    for heading in ["BRANCH", "STATE", "PANES", "PATH"] {
        assert!(text.contains(heading), "missing {heading}:\n{text}");
    }
    assert!(text.contains("m CHECKOUT · n WORKTREE"), "{text}");
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
