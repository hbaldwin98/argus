//! Column widths, and folding the leading columns away.

use super::*;
// --- folding columns away ------------------------------------------------

#[test]
fn p_cycles_through_the_fold_levels() {
    let mut h = Harness::new();
    assert_eq!(h.app.fold, Fold::None, "starts expanded");
    assert_eq!(h.app.focus, Focus::Projects);

    h.key(KeyCode::Char('p'));
    assert_eq!(h.app.fold, Fold::Projects, "p folds projects away");
    assert_eq!(h.app.focus, Focus::Repositories, "focus leaves the tab");
    assert!(
        h.app.status.contains("folded"),
        "reports the fold: {}",
        h.app.status
    );
    assert_eq!(h.app.settings.folded_columns, 1, "persisted to settings");

    h.key(KeyCode::Char('p'));
    assert_eq!(h.app.fold, Fold::Repositories, "p folds repositories too");
    assert_eq!(h.app.focus, Focus::Checkouts, "focus leaves that tab as well");
    assert_eq!(h.app.settings.folded_columns, 2);

    h.key(KeyCode::Char('p'));
    assert_eq!(h.app.fold, Fold::None, "and wraps back to none");
    assert_eq!(
        h.app.focus,
        Focus::Checkouts,
        "focus stays put on expand"
    );
    assert!(
        h.app.status.contains("expanded"),
        "reports expand: {}",
        h.app.status
    );
    assert_eq!(h.app.settings.folded_columns, 0, "cleared in settings");
}

#[test]
fn folding_moves_focus_off_the_hidden_column() {
    let mut h = Harness::new();
    h.app.focus = Focus::Projects;
    h.key(KeyCode::Char('p'));
    assert_eq!(h.app.fold, Fold::Projects);
    assert_eq!(h.app.focus, Focus::Repositories);
}

#[test]
fn ascending_into_a_folded_away_column_stays_put() {
    let mut h = Harness::new();
    h.key(KeyCode::Char('p')); // fold projects, focus -> Repositories
    h.key(KeyCode::Char('h')); // ascend from Repositories
    assert_eq!(
        h.app.focus,
        Focus::Repositories,
        "blocked by the folded-away tab"
    );
    // All the way round to expanded; now ascend works.
    h.key(KeyCode::Char('p'));
    h.key(KeyCode::Char('p'));
    h.key(KeyCode::Char('h')); // checkouts -> repositories
    h.key(KeyCode::Char('h')); // repositories -> projects
    assert_eq!(h.app.focus, Focus::Projects);
}

#[test]
fn starting_folded_lands_on_the_leftmost_column_drawn() {
    let (tx, _rx) = unbounded_channel();
    let settings = crate::settings::Settings {
        folded_columns: 1,
        ..crate::settings::Settings::default()
    };
    let app = App::build(tx, settings, false);
    assert_eq!(app.fold, Fold::Projects);
    assert_eq!(
        app.focus,
        Focus::Repositories,
        "never lands on the hidden column"
    );
}

#[test]
fn clicking_a_fold_tab_brings_that_column_back() {
    let mut h = Harness::new();
    h.app.fold = Fold::Projects;
    h.app.layout.projects = Panel {
        outer: Rect::new(0, 1, 1, 16),
        inner: Rect::new(0, 1, 1, 1),
        first: 0,
    };
    h.app.on_mouse(click(0, 1));
    assert_eq!(h.app.fold, Fold::None, "click expands");
}

#[test]
fn the_gutter_next_to_a_fold_tab_is_not_draggable() {
    let mut h = Harness::new();
    let panel = |x: u16, w: u16| Panel {
        outer: Rect::new(x, 0, w, 8),
        inner: Rect::new(x + 1, 1, w.saturating_sub(2), 6),
        first: 0,
    };
    // Tab at 0..1, a one-cell gap, repositories at 2..14. The gap would
    // otherwise be gutter 0; a folded layout suppresses it.
    h.app.layout = Layout {
        width: 61,
        row_height: crate::ui::ROW_HEIGHT,
        views: Panel::default(),
        projects: panel(0, 1),
        repositories: panel(2, 12),
        checkouts: panel(15, 12),
        panes: panel(28, 12),
        content: panel(41, 20),
        overlay: Panel::default(),
        help: Panel::default(),
        cursor: None,
    };
    h.app.fold = Fold::Projects;

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
