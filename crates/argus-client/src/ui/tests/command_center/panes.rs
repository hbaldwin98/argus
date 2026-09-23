//! The Panes stage: the card overview and clicking through to a pane.

use super::*;

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
