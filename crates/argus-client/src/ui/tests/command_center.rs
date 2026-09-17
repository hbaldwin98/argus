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
    assert!(!text.contains("└ master"), "repositories expand only after a click:\n{text}");
    assert_eq!(app.layout.projects.outer.x, 0);
    assert_eq!(app.layout.projects.outer.width, 32);
    assert!(app.layout.content.outer.x >= 32);
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
fn clicking_a_checkout_row_uses_the_rendered_two_line_geometry() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

    let mut app = command_center();
    app.open_view(View::Checkouts);
    draw_at(&mut app, 120, 30);
    let rows = app.layout.checkouts.inner;

    app.on_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: rows.x + 2,
        row: rows.y + 3,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(app.sel_checkout, 1);
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
    assert!(text.contains("└ ◆ claude"), "{text}");
    assert!(text.contains("└ › shell"), "{text}");
}

#[test]
fn current_pane_is_highlighted_in_an_expanded_repository() {
    let mut app = command_center();
    app.focus = Focus::PaneContent;
    let text = lines(&draw_at(&mut app, 120, 30)).join("\n");

    assert!(text.contains("▌      └ ◆ claude"), "{text}");
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
