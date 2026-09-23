//! The contextual rail: its badges, repository ordering and expansion,
//! and what a click on each of its rows does.

use super::*;

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
fn the_rail_border_continues_the_feature_tabs_left_edge() {
    let mut app = command_center();
    let rows = lines(&draw_at(&mut app, 140, 40));
    let tabs: Vec<char> = rows[1].chars().collect();
    let feature = rows[1].find("FEATURE").unwrap();
    let feature_left = rows[1][..feature].chars().count() - 2;
    let border = app.layout.projects.outer.right() as usize - 1;
    assert_eq!(feature_left, border, "{}\n{:?}", rows.join("\n"), tabs);
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
