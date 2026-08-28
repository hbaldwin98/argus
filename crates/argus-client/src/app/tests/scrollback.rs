//! Parking a pane's view above its live screen.

use super::*;
#[test]
fn a_wheel_over_a_shell_asks_for_the_lines_above_it() {
    // The normal screen is the case with history behind it, and nothing
    // used to happen at all: a shell's output was gone once it passed
    // the top of the pane.
    let mut h = Harness::new();
    live_pane(&mut h);

    h.app.on_mouse(wheel(MouseEventKind::ScrollUp));

    let sent = h.sent();
    let asked: Vec<u32> = sent
        .iter()
        .filter_map(|m| match m {
            ClientMsg::Scrollback { offset, .. } => Some(*offset),
            _ => None,
        })
        .collect();
    assert_eq!(asked, [3], "one notch is three lines");
    assert!(
        !sent.iter().any(|m| matches!(m, ClientMsg::Input { .. })),
        "and no bytes reach the child"
    );
}

#[test]
fn scrolling_back_draws_the_rows_the_daemon_answers_with() {
    let mut h = Harness::new();
    let pane = live_pane(&mut h);
    assert_eq!(drawn_mark(&h, pane), " ", "live to begin with");

    h.app.on_mouse(wheel(MouseEventKind::ScrollUp));
    answer_scrollback(&mut h, pane, 3, 500, 'H');

    assert_eq!(drawn_mark(&h, pane), "H");
    assert!(h.app.grids[&pane].is_scrolled());
}

#[test]
fn scrolling_back_down_to_the_bottom_returns_to_the_live_screen() {
    let mut h = Harness::new();
    let pane = live_pane(&mut h);
    h.app.on_mouse(wheel(MouseEventKind::ScrollUp));
    answer_scrollback(&mut h, pane, 3, 500, 'H');
    h.sent();

    h.app.on_mouse(wheel(MouseEventKind::ScrollDown));

    assert!(!h.app.grids[&pane].is_scrolled());
    assert_eq!(drawn_mark(&h, pane), " ", "the live grid, still current");
    assert!(
        h.sent().is_empty(),
        "the live screen is already streaming; asking for it again is waste"
    );
}

#[test]
fn scrolling_stops_at_the_depth_the_daemon_reported() {
    let mut h = Harness::new();
    let pane = live_pane(&mut h);
    h.app.on_mouse(wheel(MouseEventKind::ScrollUp));
    answer_scrollback(&mut h, pane, 3, 4, 'H');
    h.sent();

    h.app.on_mouse(wheel(MouseEventKind::ScrollUp));
    assert_eq!(scrollback_asks(&mut h), [4]);

    answer_scrollback(&mut h, pane, 4, 4, 'T');
    h.app.on_mouse(wheel(MouseEventKind::ScrollUp));
    assert!(
        h.sent().is_empty(),
        "already at the top: no point asking for rows that do not exist"
    );
}

#[test]
fn a_daemon_with_nothing_behind_the_screen_leaves_the_pane_live() {
    // What a freshly started shell answers, and what any child on the
    // alternate screen answers: an offset of zero means there is no
    // history to park in.
    let mut h = Harness::new();
    let pane = live_pane(&mut h);
    h.app.on_mouse(wheel(MouseEventKind::ScrollUp));

    answer_scrollback(&mut h, pane, 0, 0, 'X');

    assert!(!h.app.grids[&pane].is_scrolled());
    assert_eq!(drawn_mark(&h, pane), " ");
}

#[test]
fn typing_snaps_a_scrolled_pane_back_to_the_live_screen() {
    // The child's echo lands on the live screen. Leaving the view parked
    // would type into somewhere the operator cannot see.
    let mut h = Harness::new();
    let pane = live_pane(&mut h);
    h.app.on_mouse(wheel(MouseEventKind::ScrollUp));
    answer_scrollback(&mut h, pane, 3, 500, 'H');
    h.sent();

    h.key(KeyCode::Char('x'));

    assert!(!h.app.grids[&pane].is_scrolled());
    assert!(
        h.sent()
            .iter()
            .any(|m| matches!(m, ClientMsg::Input { .. })),
        "and the keystroke still reaches the child"
    );
}

#[test]
fn shift_page_up_pages_by_a_screen_and_leaves_the_child_its_own_keys() {
    let mut h = Harness::new();
    live_pane(&mut h);

    h.app
        .on_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::SHIFT));
    assert_eq!(
        scrollback_asks(&mut h),
        [5],
        "the content panel is six rows, paged with one line of overlap"
    );

    h.app
        .on_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));
    assert!(
        h.sent()
            .iter()
            .any(|m| matches!(m, ClientMsg::Input { .. })),
        "unshifted paging is the child's"
    );
}

#[test]
fn a_scrolled_pane_says_so_in_its_title() {
    let mut h = Harness::new();
    let pane = live_pane(&mut h);
    assert_eq!(h.app.scroll_indicator(), None);

    h.app.on_mouse(wheel(MouseEventKind::ScrollUp));
    answer_scrollback(&mut h, pane, 3, 500, 'H');

    assert_eq!(h.app.scroll_indicator().as_deref(), Some("\u{2191} 3/500"));
}

#[test]
fn a_stale_answer_cannot_pull_a_pane_that_went_live_back_up() {
    let mut h = Harness::new();
    let pane = live_pane(&mut h);
    h.app.on_mouse(wheel(MouseEventKind::ScrollUp));
    h.app.on_mouse(wheel(MouseEventKind::ScrollDown));
    assert!(!h.app.grids[&pane].is_scrolled());

    answer_scrollback(&mut h, pane, 3, 500, 'H');

    assert!(
        !h.app.grids[&pane].is_scrolled(),
        "the request was abandoned before its answer arrived"
    );
}

#[test]
fn a_resize_re_reads_the_parked_rows_at_the_new_width() {
    // A snapshot is how a resize reaches the client. The parked rows are
    // the old width, and only a fresh read can replace them.
    let mut h = Harness::new();
    let pane = live_pane(&mut h);
    h.app.on_mouse(wheel(MouseEventKind::ScrollUp));
    answer_scrollback(&mut h, pane, 3, 500, 'H');
    h.sent();

    h.app.on_server_msg(ServerMsg::PaneSnapshot {
        pane,
        rows: 3,
        cols: 8,
        cells: vec![vec![Cell::default(); 8]; 3],
        cursor: argus_protocol::Cursor::default(),
        mouse: Default::default(),
        alternate_screen: false,
    });

    assert_eq!(scrollback_asks(&mut h), [3]);
    assert!(h.app.grids[&pane].is_scrolled(), "and stays where it was");
}

#[test]
fn the_mouse_is_ignored_while_a_modal_is_open() {
    let mut h = Harness::new();
    laid_out(&mut h);
    h.key(KeyCode::Char('n'));
    h.app.on_mouse(click(14, 3));
    assert_eq!(
        h.app.sel_checkout, 0,
        "click must not navigate behind the modal"
    );
    assert!(h.app.dir_picker.is_some());
}

#[test]
fn resize_is_forwarded_for_the_named_pane() {
    let mut h = Harness::new();
    h.app.resize_pane(PaneId(100), 30, 100);
    match &h.sent()[0] {
        ClientMsg::Resize { pane, rows, cols } => {
            assert_eq!(*pane, PaneId(100));
            assert_eq!((*rows, *cols), (30, 100));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn every_pane_on_screen_is_sized_from_its_own_area() {
    // A floating editor and the column behind it are different widths;
    // sizing both from one of them wraps the other wrongly.
    let mut h = Harness::new();
    laid_out(&mut h);
    h.app
        .open_overlay_pane(PaneId(700), "vim".to_string(), false);
    h.app.layout.overlay = Panel {
        outer: Rect::new(2, 1, 60, 20),
        inner: Rect::new(3, 2, 58, 18),
        first: 0,
    };

    let live = h.app.live_panes();
    assert_eq!(live.len(), 2, "the column's pane and the floating one");
    assert_eq!(live[0].1, h.app.layout.content.inner);
    assert_eq!(live[1].1, h.app.layout.overlay.inner);
}
