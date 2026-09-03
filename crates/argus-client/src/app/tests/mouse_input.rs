//! Clicks, drags, and the wheel.

use super::*;
#[test]
fn dragging_a_gutter_resizes_the_two_adjacent_columns() {
    let mut h = Harness::new();
    let panel = |x: u16, w: u16| Panel {
        outer: Rect::new(x, 0, w, 8),
        inner: Rect::new(x + 1, 1, w.saturating_sub(2), 6),
        first: 0,
    };
    h.app.layout = Layout {
        width: 100,
        row_height: crate::ui::ROW_HEIGHT,
        views: Panel::default(),
        projects: panel(0, 20),
        repositories: panel(21, 20),
        checkouts: panel(42, 20),
        panes: panel(63, 20),
        content: panel(84, 30),
        overlay: Panel::default(),
        help: Panel::default(),
        cursor: None,
    };

    h.app.on_mouse(click(20, 3));
    h.app.on_mouse(drag(16, 3));
    h.app.on_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 16,
        row: 3,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(h.app.column_widths, Some(vec![16, 24, 20, 20, 30]));
    assert_eq!(h.app.settings.column_widths, h.app.column_widths);
}

#[test]
fn old_four_column_widths_fall_back_to_the_five_column_layout() {
    let (tx, _rx) = unbounded_channel();
    let settings = crate::settings::Settings {
        column_widths: Some(vec![10, 20, 30, 40]),
        ..crate::settings::Settings::default()
    };

    let app = App::build(tx, settings, false);

    assert_eq!(app.column_widths, None);
}

#[test]
fn saved_five_column_widths_are_restored_at_startup() {
    let (tx, _rx) = unbounded_channel();
    let settings = crate::settings::Settings {
        column_widths: Some(vec![10, 15, 20, 25, 30]),
        ..crate::settings::Settings::default()
    };

    let app = App::build(tx, settings, false);

    assert_eq!(app.column_widths, Some(vec![10, 15, 20, 25, 30]));
}

#[test]
fn dragging_a_gutter_cannot_collapse_either_column() {
    let mut h = Harness::new();
    let panel = |x: u16, w: u16| Panel {
        outer: Rect::new(x, 0, w, 8),
        inner: Rect::new(x + 1, 1, w.saturating_sub(2), 6),
        first: 0,
    };
    h.app.layout = Layout {
        width: 100,
        row_height: crate::ui::ROW_HEIGHT,
        views: Panel::default(),
        projects: panel(0, 20),
        repositories: panel(21, 20),
        checkouts: panel(42, 20),
        panes: panel(63, 20),
        content: panel(84, 30),
        overlay: Panel::default(),
        help: Panel::default(),
        cursor: None,
    };

    h.app.on_mouse(click(20, 3));
    h.app.on_mouse(drag(90, 3));

    assert_eq!(
        h.app.column_widths,
        Some(vec![26, crate::ui::MIN_COLUMN_WIDTH, 20, 20, 30]),
        "the column being squeezed stops at its floor"
    );
}

#[test]
fn clicking_a_row_selects_it_and_focuses_that_column() {
    let mut h = Harness::new();
    laid_out(&mut h);
    h.app.on_mouse(click(26, 3)); // the checkouts card, second row
    assert_eq!(h.app.focus, Focus::Checkouts);
    assert_eq!(h.app.sel_checkout, 1);
}

#[test]
fn clicking_a_child_row_selects_the_pane_it_runs_in() {
    // A child is drawn as its own row but is not somewhere to go: the
    // only thing to select there is the pane it is running inside.
    let mut h = Harness::new();
    laid_out(&mut h);
    if let Some(p) = h.app.tree[0].repositories[0].checkouts[0].panes.get_mut(0) {
        p.children = vec![argus_protocol::ChildAgentInfo {
            label: "running the tests".to_string(),
            status: PaneStatus::Working,
            note: None,
        }];
    }

    // Row 1 of the panes column is the first pane's child; row 2 is the
    // second pane, which without children would have been row 1.
    h.app.on_mouse(click(38, 3));
    assert_eq!(h.app.focus, Focus::Panes);
    assert_eq!(h.app.sel_pane, 0, "a child row means its parent");

    h.app.on_mouse(click(38, 5));
    assert_eq!(h.app.sel_pane, 1, "and the rows below it have shifted down");
}

#[test]
fn clicking_a_flat_row_selects_its_checkout_and_pane() {
    let mut h = Harness::new();
    laid_out(&mut h);
    h.app.tree[0].repositories[0].checkouts[1]
        .panes
        .push(pane(102, "feature agent"));
    h.app.settings.pane_view = crate::settings::PaneView::Flat;

    // The first checkout owns rows 0 and 1; row 2 belongs to `feat`.
    h.app.on_mouse(click(38, 5));

    assert_eq!(h.app.focus, Focus::Panes);
    assert_eq!(h.app.column_pane(), Some(PaneId(102)));
    assert_eq!(
        h.app.current_checkout().map(|checkout| checkout.id),
        Some(CheckoutId(11))
    );
}

#[test]
fn clicking_an_already_selected_row_descends() {
    let mut h = Harness::new();
    laid_out(&mut h);
    // Row 1 isn't the current selection, so the first click only selects.
    h.app.on_mouse(click(2, 3));
    assert_eq!(h.app.focus, Focus::Projects);
    assert_eq!(h.app.sel_project, 1);
    // Clicking the now-selected row again opens it.
    h.app.on_mouse(click(2, 3));
    assert_eq!(h.app.focus, Focus::Repositories, "second click opens it");
}

#[test]
fn clicking_past_the_last_row_keeps_the_selection() {
    let mut h = Harness::new();
    laid_out(&mut h);
    h.keys("lll"); // focus is off in the panes column
    h.sent();

    h.app.on_mouse(click(2, 6)); // empty space below the project rows

    assert_eq!(h.app.focus, Focus::Projects, "the click still moves focus");
    assert_eq!(h.app.sel_project, 0, "but selects nothing new");
}

#[test]
fn clicking_a_cards_frame_moves_focus_without_touching_the_selection() {
    // "Go there" and "pick that" are different gestures; only the
    // second should move a cursor.
    let mut h = Harness::new();
    laid_out(&mut h);
    h.keys("l");
    h.app.sel_checkout = 1;
    h.sent();

    h.app.on_mouse(click(0, 0)); // the projects card's top-left corner

    assert_eq!(h.app.focus, Focus::Projects);
    assert_eq!(h.app.sel_checkout, 1, "the other column keeps its place");
}

#[test]
fn clicking_the_border_of_the_column_you_are_in_does_not_descend() {
    // Only a click on the selected *row* opens it.
    let mut h = Harness::new();
    laid_out(&mut h);
    h.app.on_mouse(click(0, 0));
    h.app.on_mouse(click(0, 0));
    assert_eq!(h.app.focus, Focus::Projects);
}

#[test]
fn either_line_of_a_two_line_row_selects_that_row() {
    // The detail line is part of the item, not a gap between items.
    let mut h = Harness::new();
    laid_out(&mut h);
    h.app.on_mouse(click(2, 3)); // name line of row 1
    assert_eq!(h.app.sel_project, 1);

    h.app.sel_project = 0;
    h.app.on_mouse(click(2, 4)); // detail line of the same row
    assert_eq!(h.app.sel_project, 1);
}

#[test]
fn scrolling_a_background_column_does_not_steal_focus() {
    let mut h = Harness::new();
    laid_out(&mut h);
    h.keys("llll"); // typing into a pane
    h.sent();
    h.app.on_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 2,
        row: 0,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(h.app.sel_project, 1, "the scroll still moved the list");
    assert_eq!(h.app.focus, Focus::PaneContent, "but focus stayed put");
}

#[test]
fn clicking_the_live_view_switches_to_typing_and_forwards_the_click() {
    let mut h = Harness::new();
    laid_out(&mut h);
    let pane = h.app.column_pane().unwrap();
    wants_mouse(&mut h, pane);
    h.app.on_mouse(click(54, 3));
    assert_eq!(h.app.focus, Focus::PaneContent);
    assert!(
        h.sent()
            .iter()
            .any(|m| matches!(m, ClientMsg::Input { .. })),
        "the child gets the click too"
    );
}

#[test]
fn releasing_in_the_live_view_is_forwarded_when_not_resizing() {
    let mut h = Harness::new();
    laid_out(&mut h);
    let pane = h.app.column_pane().unwrap();
    wants_mouse(&mut h, pane);
    h.app.on_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 54,
        row: 3,
        modifiers: KeyModifiers::NONE,
    });

    assert!(
        h.sent().iter().any(|message| matches!(
            message,
            ClientMsg::Input { bytes, .. } if bytes.ends_with(b"m")
        )),
        "the child gets an ordinary release"
    );
}

#[test]
fn nothing_is_forwarded_to_a_child_that_never_asked_for_the_mouse() {
    // The bug: an agent that does no mouse reporting was still sent
    // `ESC [ < ... M` for every click and wheel turn, and typed it into
    // its prompt.
    let mut h = Harness::new();
    laid_out(&mut h);
    h.sent();

    h.app.on_mouse(click(54, 3));
    h.app.on_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 54,
        row: 3,
        modifiers: KeyModifiers::NONE,
    });

    assert!(
        !h.sent()
            .iter()
            .any(|m| matches!(m, ClientMsg::Input { .. })),
        "no mouse bytes reach a child that reports no mouse"
    );
    assert_eq!(
        h.app.focus,
        Focus::PaneContent,
        "the click still selects the live view"
    );
}

#[test]
fn a_wheel_over_an_alt_screen_tui_arrives_as_arrows() {
    // Codex enables DECSET 1007 rather than mouse tracking; Claude and
    // Cursor Agent take the alternate screen the same way. A swallowed
    // wheel is a conversation that cannot scroll.
    let mut h = Harness::new();
    laid_out(&mut h);
    let pane = h.app.column_pane().unwrap();
    on_alt_screen(&mut h, pane);
    h.sent();

    h.app.on_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 54,
        row: 3,
        modifiers: KeyModifiers::NONE,
    });
    h.app.on_mouse(MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: 54,
        row: 3,
        modifiers: KeyModifiers::NONE,
    });

    let bytes: Vec<Vec<u8>> = h
        .sent()
        .into_iter()
        .filter_map(|m| match m {
            ClientMsg::Input { bytes, .. } => Some(bytes),
            _ => None,
        })
        .collect();
    assert_eq!(bytes, [b"\x1b[B".to_vec(), b"\x1b[A".to_vec()]);
}

#[test]
fn a_mouse_tracking_child_still_gets_wheel_reports_not_arrows() {
    // OpenCode enables SGR mouse reporting (and the alternate screen).
    // Those reports must win over the cursor-key fallback.
    let mut h = Harness::new();
    laid_out(&mut h);
    let pane = h.app.column_pane().unwrap();
    wants_mouse(&mut h, pane);
    on_alt_screen(&mut h, pane);
    h.sent();

    h.app.on_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 54,
        row: 3,
        modifiers: KeyModifiers::NONE,
    });

    let bytes = h.sent().iter().find_map(|m| match m {
        ClientMsg::Input { bytes, .. } => Some(bytes.clone()),
        _ => None,
    });
    let bytes = bytes.expect("the child gets a mouse report");
    assert!(
        bytes.starts_with(b"\x1b[<65;"),
        "SGR wheel down, not a cursor key: {bytes:?}"
    );
}

#[test]
fn a_wheel_over_the_live_view_never_scrolls_the_columns_behind_it() {
    let mut h = Harness::new();
    laid_out(&mut h);
    let before = h.app.sel_project;

    h.app.on_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 54,
        row: 3,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(h.app.sel_project, before);
}

#[test]
fn a_release_is_dropped_for_a_child_that_only_reports_presses() {
    let mut h = Harness::new();
    laid_out(&mut h);
    let pane = h.app.column_pane().unwrap();
    wants_mouse(&mut h, pane);
    h.app.grids.get_mut(&pane).unwrap().mouse.mode = argus_protocol::MouseMode::Press;
    h.sent();

    h.app.on_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 54,
        row: 3,
        modifiers: KeyModifiers::NONE,
    });

    assert!(!h
        .sent()
        .iter()
        .any(|m| matches!(m, ClientMsg::Input { .. })));
}
