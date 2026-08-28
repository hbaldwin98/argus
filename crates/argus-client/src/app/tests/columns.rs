//! Column widths, and folding the projects column away.

use super::*;
// --- collapse projects pane ----------------------------------------------

#[test]
fn p_collapses_and_restores_the_projects_column() {
    let mut h = Harness::new();
    assert!(!h.app.projects_collapsed, "starts expanded");
    assert_eq!(h.app.focus, Focus::Projects);

    h.key(KeyCode::Char('p'));
    assert!(h.app.projects_collapsed, "p collapses");
    assert_eq!(h.app.focus, Focus::Repositories, "focus leaves the tab");
    assert!(
        h.app.status.contains("collapsed"),
        "reports collapse: {}",
        h.app.status
    );
    assert!(h.app.settings.projects_collapsed, "persisted to settings");

    h.key(KeyCode::Char('p'));
    assert!(!h.app.projects_collapsed, "p restores");
    assert_eq!(
        h.app.focus,
        Focus::Repositories,
        "focus stays put on restore"
    );
    assert!(
        h.app.status.contains("expanded"),
        "reports expand: {}",
        h.app.status
    );
    assert!(!h.app.settings.projects_collapsed, "cleared in settings");
}

#[test]
fn collapsing_moves_focus_off_projects() {
    let mut h = Harness::new();
    h.app.focus = Focus::Projects;
    h.key(KeyCode::Char('p'));
    assert!(h.app.projects_collapsed);
    assert_eq!(h.app.focus, Focus::Repositories);
}

#[test]
fn ascending_into_a_collapsed_projects_column_stays_put() {
    let mut h = Harness::new();
    h.key(KeyCode::Char('p')); // collapse, focus -> Repositories
    h.key(KeyCode::Char('h')); // ascend from Repositories
    assert_eq!(
        h.app.focus,
        Focus::Repositories,
        "blocked by the folded-away tab"
    );
    // Expand it; now ascend works.
    h.key(KeyCode::Char('p'));
    h.key(KeyCode::Char('h'));
    assert_eq!(h.app.focus, Focus::Projects);
}

#[test]
fn starting_collapsed_lands_on_repositories() {
    let (tx, _rx) = unbounded_channel();
    let settings = crate::settings::Settings {
        projects_collapsed: true,
        ..crate::settings::Settings::default()
    };
    let app = App::build(tx, settings, false);
    assert!(app.projects_collapsed);
    assert_eq!(
        app.focus,
        Focus::Repositories,
        "never lands on the hidden column"
    );
}

#[test]
fn clicking_the_collapsed_tab_expands_it() {
    let mut h = Harness::new();
    h.app.projects_collapsed = true;
    h.app.layout.projects = Panel {
        outer: Rect::new(0, 1, 1, 16),
        inner: Rect::new(0, 1, 1, 1),
        first: 0,
    };
    h.app.on_mouse(click(0, 1));
    assert!(!h.app.projects_collapsed, "click expands");
}

#[test]
fn the_gutter_next_to_a_collapsed_tab_is_not_draggable() {
    let mut h = Harness::new();
    let panel = |x: u16, w: u16| Panel {
        outer: Rect::new(x, 0, w, 8),
        inner: Rect::new(x + 1, 1, w.saturating_sub(2), 6),
        first: 0,
    };
    // Tab at 0..1, a one-cell gap, repositories at 2..14. The gap would
    // otherwise be gutter 0; collapsed layout suppresses it.
    h.app.layout = Layout {
        projects: panel(0, 1),
        repositories: panel(2, 12),
        checkouts: panel(15, 12),
        panes: panel(28, 12),
        content: panel(41, 20),
        overlay: Panel::default(),
        cursor: None,
    };
    h.app.projects_collapsed = true;

    h.app.on_mouse(click(1, 3)); // the gap
    assert!(h.app.resizing_gutter.is_none(), "gutter suppressed");

    // Drag does nothing.
    h.app.on_mouse(drag(5, 3));
    h.app.on_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 5,
        row: 3,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(h.app.column_widths, None);
}
