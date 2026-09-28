//! Clicks, drags, and the wheel over the workspace terminal. The rail's
//! and each stage's rows are clicked in `ui/tests/command_center`, where
//! the frame that drew them is at hand.

use super::*;

fn accepts_copy(_: &str) -> bool {
    true
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
fn a_wheel_over_the_terminal_never_scrolls_the_rail_beside_it() {
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

#[test]
fn dragging_in_a_shell_copies_from_its_visible_grid() {
    let mut h = Harness::new();
    laid_out(&mut h);
    let pane = h.app.column_pane().unwrap();
    h.app.clipboard_write = accepts_copy;
    h.app.grids.insert(
        pane,
        crate::grid::Grid::new(vec!["shell output"
            .chars()
            .map(|ch| Cell {
                ch: ch.to_string().into(),
                ..Default::default()
            })
            .collect()]),
    );

    h.app.on_mouse(click(49, 1));
    h.app.on_mouse(drag(53, 1));
    h.app.on_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 53,
        row: 1,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(h.app.status, "copied selection");
    assert_eq!(
        h.app
            .selection
            .as_ref()
            .unwrap()
            .text(h.app.grids[&pane].view()),
        "shell"
    );
    assert!(!h
        .sent()
        .iter()
        .any(|m| matches!(m, ClientMsg::Input { .. })));
}

#[test]
fn shift_drag_copies_locally_instead_of_reaching_a_mouse_aware_child() {
    let mut h = Harness::new();
    laid_out(&mut h);
    let pane = h.app.column_pane().unwrap();
    wants_mouse(&mut h, pane);
    h.app.clipboard_write = accepts_copy;
    h.app.grids.get_mut(&pane).unwrap().cells = vec!["abcdefgh"
        .chars()
        .map(|ch| Cell {
            ch: ch.to_string().into(),
            ..Default::default()
        })
        .collect()];
    h.sent();
    let shifted = |kind| MouseEvent {
        kind,
        column: 50,
        row: 1,
        modifiers: KeyModifiers::SHIFT,
    };

    h.app
        .on_mouse(shifted(MouseEventKind::Down(MouseButton::Left)));
    h.app
        .on_mouse(shifted(MouseEventKind::Drag(MouseButton::Left)));
    h.app.on_mouse(MouseEvent {
        column: 52,
        ..shifted(MouseEventKind::Up(MouseButton::Left))
    });

    assert_eq!(h.app.status, "copied selection");
    assert!(!h
        .sent()
        .iter()
        .any(|m| matches!(m, ClientMsg::Input { .. })));
}

#[test]
fn a_shell_drag_keeps_its_release_after_leaving_the_pane() {
    let mut h = Harness::new();
    laid_out(&mut h);
    let pane = h.app.column_pane().unwrap();
    h.app.clipboard_write = accepts_copy;
    h.app.grids.insert(
        pane,
        crate::grid::Grid::new(vec![
            vec![
                Cell {
                    ch: "x".into(),
                    ..Default::default()
                };
                18
            ];
            6
        ]),
    );

    h.app.on_mouse(click(50, 2));
    h.app.on_mouse(drag(99, 20));
    h.app.on_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 99,
        row: 20,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(h.app.status, "copied selection");
    assert!(h.app.selection.is_some());
}
