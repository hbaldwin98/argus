//! Whole frames, drawn through a `TestBackend` and read back as text.

use super::*;

#[test]
fn a_click_in_a_scrolled_column_selects_the_row_it_landed_on() {
    let mut app = app_with_a_long_checkout_column();
    app.sel_checkout = 7;
    // Tall enough for a few rows, far short of eight.
    let buf = draw_at(&mut app, 100, 12);
    let top = app.layout.checkouts.inner.y;
    let first_drawn = lines(&buf)[top as usize].clone();

    click_checkout(&mut app, 0);

    let name = app
        .current_checkout()
        .map(|c| c.name.clone())
        .unwrap_or_default();
    assert!(
        first_drawn.contains(&name),
        "clicking the top row must select the checkout drawn there: {first_drawn:?} selected {name:?}"
    );
}

#[test]
fn a_scrolled_column_does_not_slide_when_the_selection_moves_back_up() {
    let mut app = app_with_a_long_checkout_column();
    app.sel_checkout = 7;
    let buf = draw_at(&mut app, 100, 12);
    let top = app.layout.checkouts.inner.y as usize;
    assert!(lines(&buf)[top].contains("wt-3"), "the column is scrolled");

    app.on_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
    let buf = draw_at(&mut app, 100, 12);

    assert!(
        lines(&buf)[top].contains("wt-3"),
        "the selection is still on screen, so the list must not move under it: {:?}",
        lines(&buf)[top]
    );
}

/// The policy itself. Deriving the offset from the selection alone
/// pins the selected row to the bottom of the card, which is what made
/// the columns lurch whenever a row appeared above the cursor.
#[test]
fn a_column_scrolls_the_least_it_can_to_keep_the_selection_visible() {
    // Already on screen: nothing moves.
    assert_eq!(scrolled_to_show(3, Some(4), 5, 20), 3);
    // Above the window, and below it.
    assert_eq!(scrolled_to_show(3, Some(1), 5, 20), 1);
    assert_eq!(scrolled_to_show(3, Some(8), 5, 20), 4);
    // A list that fits needs no offset at all, and one that shrank
    // must not leave blank rows under it.
    assert_eq!(scrolled_to_show(3, Some(1), 5, 4), 0);
    assert_eq!(scrolled_to_show(9, Some(6), 5, 8), 3);
}

#[test]
fn a_checkout_two_agents_are_working_in_says_it_is_shared() {
    let mut app = app_with_tree();
    let panes = &mut app.tree[0].repositories[0].checkouts[0].panes;
    for p in panes.iter_mut() {
        p.kind = argus_protocol::PaneKind::Agent;
    }

    let buf = draw_at(&mut app, 200, 20);
    let rendered = lines(&buf);
    let at = rendered
        .iter()
        .position(|l| l.contains("⌂ master"))
        .expect("the primary checkout has a row");

    assert!(
        rendered[at].contains('⚠'),
        "the glyph is what survives a narrow column: {:?}",
        rendered[at]
    );
    assert!(
        rendered[at + 1].contains("shared by 2"),
        "sharing a checkout is allowed, but not something to find out later: {:?}",
        rendered[at + 1]
    );
}

#[test]
fn one_agent_and_a_shell_is_not_sharing() {
    let mut app = app_with_tree();

    let buf = draw_at(&mut app, 120, 20);
    let rendered = lines(&buf);

    assert!(
        !rendered.iter().any(|l| l.contains("shared")),
        "the fixture has one agent and one shell in that checkout"
    );
}

#[test]
fn the_main_branch_gets_a_row_of_its_own_and_the_rest_do_not() {
    let mut app = app_with_tree();
    let r = &mut app.tree[0].repositories[0];
    r.branches = vec!["hotfix".to_string(), "trunk".to_string()];
    r.default_branch = Some("trunk".to_string());

    let buf = draw_at(&mut app, 120, 20);
    let rendered = lines(&buf);
    assert!(
        !rendered.iter().any(|l| l.contains("hotfix")),
        "an ordinary branch stays out of the column: {rendered:?}"
    );
    let at = rendered
        .iter()
        .position(|l| l.contains("trunk"))
        .expect("the main branch keeps its row whether or not it has a directory");

    // A row is two lines: the name, then what it is.
    assert!(
        rendered[at + 1].contains("no checkout"),
        "a branch row has to say what it is: {:?}",
        rendered[at + 1]
    );
}

#[test]
fn a_remote_only_branch_says_that_is_where_it_is() {
    let mut app = app_with_tree();
    app.tree[0].repositories[0].remote_branches = vec!["origin/spike".to_string()];
    app.show_branches = true;

    // Wide, so the column has room for the row's own words.
    let rendered = lines(&draw_at(&mut app, 200, 20));
    let at = rendered
        .iter()
        .position(|l| l.contains("origin/spike"))
        .expect("a branch the remote has should be reachable");
    assert!(
        rendered[at + 1].contains("on the remote only"),
        "{:?}",
        rendered[at + 1]
    );
}

#[test]
fn the_other_branches_appear_once_the_column_is_expanded() {
    let mut app = app_with_tree();
    app.tree[0].repositories[0].branches = vec!["hotfix".to_string()];
    app.show_branches = true;

    let rendered = lines(&draw_at(&mut app, 120, 20));
    let at = rendered
        .iter()
        .position(|l| l.contains("hotfix"))
        .expect("expanded, the branch should have a row of its own");
    assert!(
        rendered[at + 1].contains("no checkout"),
        "{:?}",
        rendered[at + 1]
    );
}

#[test]
fn all_five_columns_are_drawn_in_the_normal_pane_view() {
    let mut app = app_with_tree();
    app.focus = Focus::PaneContent;
    let text = lines(&draw(&mut app)).join("\n");
    for title in ["projects", "repositories", "checkouts", "panes"] {
        assert!(
            text.contains(title),
            "{title} column missing while inside a pane"
        );
    }
}

#[test]
fn fullscreen_gives_the_main_area_to_the_selected_pane() {
    let mut app = app_with_tree();
    app.focus = Focus::PaneContent;
    draw(&mut app);
    let column_width = app.layout.content.outer.width;

    app.pane_fullscreen = true;
    let text = lines(&draw(&mut app)).join("\n");

    assert!(app.layout.content.outer.width > column_width);
    for panel in [
        app.layout.projects,
        app.layout.repositories,
        app.layout.checkouts,
        app.layout.panes,
    ] {
        assert_eq!(
            panel.outer,
            Rect::default(),
            "hidden columns must not remain clickable"
        );
    }
    for title in ["projects", "repositories", "checkouts", "panes"] {
        assert!(
            !text.contains(title),
            "{title} column remained visible in fullscreen"
        );
    }
    assert!(text.contains("argus › orion › master › claude"));
    assert!(text.contains("f restore"));
}

#[test]
fn a_focused_terminal_places_the_hardware_cursor_at_the_child_cursor() {
    let mut app = app_with_tree();
    app.focus = Focus::PaneContent;
    app.grids.insert(
        PaneId(100),
        Grid::with_cursor(
            vec![vec![Default::default(); 4]; 3],
            argus_protocol::Cursor {
                row: 1,
                col: 2,
                visible: true,
                ..Default::default()
            },
            Default::default(),
        ),
    );
    let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();

    terminal.draw(|f| render(f, &mut app)).unwrap();

    terminal.backend_mut().assert_cursor_position((
        app.layout.content.inner.x + 2,
        app.layout.content.inner.y + 1,
    ));
}

#[test]
fn an_overlay_whose_cursor_is_hidden_does_not_leave_the_column_cursor_on_top_of_it() {
    // Regression: the content column and the overlay both drew a
    // terminal, and ratatui has one cursor slot per frame. The overlay
    // took the early return when its child had hidden its cursor,
    // which left the column's position in the slot — so the hardware
    // cursor sat on top of the overlay, pointing at a coordinate that
    // belonged to the pane behind it.
    let mut app = app_with_tree();
    app.focus = Focus::PaneContent;
    app.grids.insert(
        PaneId(100),
        Grid::with_cursor(
            vec![vec![Default::default(); 40]; 10],
            argus_protocol::Cursor {
                row: 1,
                col: 2,
                visible: true,
                ..Default::default()
            },
            Default::default(),
        ),
    );
    app.grids.insert(
        PaneId(101),
        Grid::with_cursor(
            vec![vec![Default::default(); 40]; 10],
            argus_protocol::Cursor {
                row: 1,
                col: 2,
                visible: false,
                ..Default::default()
            },
            Default::default(),
        ),
    );
    app.overlay = Some(Overlay::Pane {
        pane: PaneId(101),
        title: "editor".to_string(),
        ephemeral: true,
    });

    draw(&mut app);

    assert_eq!(
        app.layout.cursor, None,
        "the overlay owns the cursor and its child has hidden it"
    );
}

#[test]
fn a_prompt_takes_the_cursor_away_from_the_pane_behind_it() {
    // The prompt draws its own caret; a second cursor blinking in the
    // column underneath is just noise.
    let mut app = app_with_tree();
    app.focus = Focus::PaneContent;
    app.grids.insert(
        PaneId(100),
        Grid::with_cursor(
            vec![vec![Default::default(); 40]; 10],
            argus_protocol::Cursor {
                row: 1,
                col: 2,
                visible: true,
                ..Default::default()
            },
            Default::default(),
        ),
    );
    draw(&mut app);
    assert!(app.layout.cursor.is_some());

    app.prompt = Some(Prompt::EditorCommand {
        input: String::new(),
    });
    draw(&mut app);

    assert_eq!(app.layout.cursor, None);
}

#[test]
fn a_cursor_past_the_end_of_the_grid_is_not_drawn() {
    // A pane's on-screen box is resized a frame before its pty is, so
    // the grid can be smaller than the area it is drawn into. Placing
    // the cursor by the area alone points it at rows that were never
    // drawn.
    let area = Rect::new(0, 0, 80, 40);
    let grid = Grid::with_cursor(
        vec![vec![Default::default(); 80]; 24],
        argus_protocol::Cursor {
            row: 30,
            col: 0,
            visible: true,
            ..Default::default()
        },
        Default::default(),
    );

    assert_eq!(term_cursor(Some(&grid), area, true), None);
}

#[test]
fn a_cursor_inside_both_the_grid_and_the_area_is_drawn() {
    let area = Rect::new(3, 5, 80, 40);
    let grid = Grid::with_cursor(
        vec![vec![Default::default(); 80]; 24],
        argus_protocol::Cursor {
            row: 7,
            col: 9,
            visible: true,
            ..Default::default()
        },
        Default::default(),
    );

    assert_eq!(
        term_cursor(Some(&grid), area, true).map(|c| c.position),
        Some(Position::new(12, 12))
    );
}

#[test]
fn an_unfocused_or_hidden_cursor_is_not_drawn() {
    let area = Rect::new(0, 0, 80, 40);
    let visible = Grid::with_cursor(
        vec![vec![Default::default(); 80]; 24],
        argus_protocol::Cursor {
            row: 1,
            col: 1,
            visible: true,
            ..Default::default()
        },
        Default::default(),
    );
    let hidden = Grid::with_cursor(
        vec![vec![Default::default(); 80]; 24],
        argus_protocol::Cursor {
            row: 1,
            col: 1,
            visible: false,
            ..Default::default()
        },
        Default::default(),
    );

    assert_eq!(term_cursor(Some(&visible), area, false), None);
    assert_eq!(term_cursor(Some(&hidden), area, true), None);
    assert_eq!(term_cursor(None, area, true), None);
}

#[test]
fn preferred_column_widths_are_used_and_keep_a_minimum() {
    let mut app = app_with_tree();
    app.column_widths = Some(vec![2, 18, 20, 20, 40]);
    draw(&mut app);

    assert_eq!(app.layout.projects.outer.width, MIN_COLUMN_WIDTH);
    assert_eq!(app.layout.repositories.outer.width, 18);
    assert_eq!(app.layout.checkouts.outer.width, 20);
    assert_eq!(app.layout.panes.outer.width, 20);
    assert_eq!(app.layout.content.outer.width, 42, "the slack lands in the live view");
}

#[test]
fn narrow_row_text_ends_in_an_ellipsis() {
    let mut app = app_with_tree();
    app.tree[0].name = "a-project-with-a-very-long-name".to_string();
    app.column_widths = Some(vec![MIN_COLUMN_WIDTH, 18, 18, 18, 34]);
    let text = lines(&draw(&mut app)).join("\n");

    assert!(
        text.contains("● a-proj…"),
        "a name past the column's width should end in an ellipsis:\n{text}"
    );
}

#[test]
fn the_tree_contents_actually_reach_the_screen() {
    let mut app = app_with_tree();
    let text = lines(&draw(&mut app)).join("\n");
    assert!(text.contains("argus"), "project name");
    assert!(text.contains("master"), "checkout name");
    assert!(text.contains("claude"), "pane title");
}

#[test]
fn repository_rows_roll_up_checkout_counts_panes_and_status() {
    let mut app = app_with_tree();
    app.tree[0].repositories.push(RepositoryInfo {
        id: RepositoryId(3),
        name: "satellite".to_string(),
        branches: Vec::new(),
        default_branch: None,
        remote_branches: Vec::new(),
        checkouts: vec![CheckoutInfo {
            id: CheckoutId(12),
            name: "main".to_string(),
            path: "/satellite".to_string(),
            primary: true,
            git: None,
            panes: vec![PaneInfo {
                id: PaneId(102),
                kind: PaneKind::Agent,
                title: "waiting".to_string(),
                status: PaneStatus::Waiting,
                note: None,
                template: None,
                children: Vec::new(),
            }],
            notes: Default::default(),
            has_note: false,
        }],
    });

    let buf = draw_at(&mut app, 140, 20);
    let text = lines(&buf).join("\n");
    assert!(
        text.contains("satellite"),
        "repository row missing:\n{text}"
    );
    assert!(
        text.contains("2 repositories"),
        "project rollup missing:\n{text}"
    );
    assert!(
        text.contains("1 ▣"),
        "repository pane rollup missing:\n{text}"
    );

    let status = buf
        .cell((
            app.layout.repositories.inner.x + 1,
            app.layout.repositories.inner.y + ROW_HEIGHT,
        ))
        .unwrap();
    assert_eq!(status.symbol(), "▲");
    assert_eq!(status.fg, app.theme.err);
}

#[test]
fn the_focused_column_alone_gets_the_accent_border() {
    let th = Theme::default();
    let mut app = app_with_tree();
    app.focus = Focus::Checkouts;
    let buf = draw(&mut app);

    let corner = |p: Panel| buf.cell((p.outer.x, p.outer.y)).unwrap().fg;
    assert_eq!(corner(app.layout.checkouts), th.accent, "focused column");
    assert_eq!(corner(app.layout.projects), th.edge, "unfocused column");
}

#[test]
fn the_three_elevations_show_up_on_screen() {
    // Page behind unfocused panel behind focused panel. This is what
    // makes the panels read as cards rather than boxes.
    let th = Theme::default();
    let mut app = app_with_tree();
    app.focus = Focus::Projects;
    let buf = draw(&mut app);

    // A blank cell inside each panel, below the last row.
    let blank = |p: Panel| {
        buf.cell((p.inner.x, p.inner.y + p.inner.height - 1))
            .unwrap()
            .bg
    };
    assert_eq!(
        blank(app.layout.projects),
        th.surface_focus,
        "focused panel"
    );
    assert_eq!(blank(app.layout.checkouts), th.surface, "unfocused panel");
    assert_eq!(buf.cell((0, 0)).unwrap().bg, th.bg, "the page behind them");
}

#[test]
fn the_selected_row_is_marked_and_raised_never_reversed() {
    let th = Theme::default();
    let mut app = app_with_tree();
    app.focus = Focus::Checkouts;
    app.sel_checkout = 1;
    let buf = draw(&mut app);

    let inner = app.layout.checkouts.inner;
    let marker = buf.cell((inner.x, inner.y + ROW_HEIGHT)).unwrap();
    assert_eq!(
        marker.symbol(),
        MARKER,
        "selection marker on the selected row"
    );
    assert_eq!(marker.fg, th.accent);
    assert_eq!(marker.bg, th.sel_bg);

    let unselected = buf.cell((inner.x, inner.y)).unwrap();
    assert_eq!(
        unselected.symbol(),
        GUTTER,
        "other rows keep an aligned gutter"
    );
    assert!(
        !unselected.modifier.contains(Modifier::REVERSED),
        "reverse video would fight the status colors"
    );
}

#[test]
fn an_unfocused_columns_selection_is_still_visible_but_quieter() {
    let th = Theme::default();
    let mut app = app_with_tree();
    app.focus = Focus::Projects;
    app.sel_checkout = 0;
    let buf = draw(&mut app);

    let inner = app.layout.checkouts.inner;
    let cell = buf.cell((inner.x + 1, inner.y)).unwrap();
    assert_eq!(
        cell.bg, th.sel_bg_dim,
        "you should still see where you were"
    );
}

#[test]
fn an_empty_column_explains_itself_instead_of_going_blank() {
    let mut app = app_with_tree();
    app.sel_checkout = 1; // the worktree with no panes
    app.focus = Focus::Panes;
    let text = lines(&draw(&mut app)).join("\n");
    assert!(text.contains("nothing running"), "{text}");
    assert!(
        text.contains("shell"),
        "an empty panes column says what to press:
{text}"
    );
}

#[test]
fn the_live_view_titles_itself_with_the_path_through_the_tree() {
    let app = app_with_tree();
    assert_eq!(content_title(&app), "argus › orion › master › claude");
}

#[test]
fn the_status_bar_shows_the_keymap_and_swaps_it_inside_a_pane() {
    let mut app = app_with_tree();
    let nav = lines(&draw(&mut app)).join("\n");
    assert!(nav.contains("l open"), "nav keymap");

    app.focus = Focus::PaneContent;
    let typing = lines(&draw(&mut app)).join("\n");
    assert!(
        typing.contains("typing"),
        "a pane's keys are the child's, and it should say so"
    );
    assert!(
        !typing.contains("l open"),
        "the nav keymap would be a lie here"
    );
}

#[test]
fn a_pending_leader_chord_is_announced() {
    let mut app = app_with_tree();
    app.focus = Focus::PaneContent;
    app.leader_pending = true;
    let text = lines(&draw(&mut app)).join("\n");
    assert!(
        text.contains("leader"),
        "a half-entered chord must be visible"
    );
}

#[test]
fn an_error_takes_over_the_status_bar_from_the_breadcrumb() {
    let mut app = app_with_tree();
    app.on_server_msg(argus_protocol::ServerMsg::Error {
        message: "git worktree add failed".to_string(),
    });
    let text = lines(&draw(&mut app)).join("\n");
    assert!(
        text.contains("git worktree add failed"),
        "errors must be read, not buried"
    );
}

#[test]
fn an_error_hands_the_bar_back_once_a_key_acknowledges_it() {
    let mut app = app_with_tree();
    app.on_server_msg(argus_protocol::ServerMsg::Error {
        message: "git worktree add failed".to_string(),
    });
    app.on_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('j'),
        crossterm::event::KeyModifiers::NONE,
    ));
    let bar = bar(&draw(&mut app));
    assert!(
        !bar.contains("git worktree add failed"),
        "a read error must not outlive the key that acknowledges it:
{bar}"
    );
    assert!(
        bar.trim_start().starts_with("projects"),
        "the breadcrumb gets its seat back:
{bar}"
    );
}

#[test]
fn a_fresh_bar_shows_the_breadcrumb_not_a_message() {
    let mut app = app_with_tree();
    let bar = bar(&draw(&mut app));
    assert!(
        bar.trim_start().starts_with("projects"),
        "nothing has happened yet, so the seat is the breadcrumb's:
{bar}"
    );
}

#[test]
fn an_ordinary_report_takes_the_breadcrumbs_seat_too() {
    // Feedback the user asked for by pressing something — it is no use
    // to anyone if only errors are allowed on screen.
    let mut app = app_with_tree();
    app.report("4 changed vs staged");
    let bar = bar(&draw(&mut app));
    assert!(bar.contains("4 changed vs staged"), "{bar}");
}

#[test]
fn only_an_alarm_is_colored_like_one() {
    let th = Theme::default();
    let mut app = app_with_tree();

    app.report("4 changed vs staged");
    let buf = draw(&mut app);
    // Status bar is always the second-to-last row (height 20, status bar at row 18).
    let y = buf.area.height - 2;
    assert!(
        (0..buf.area.width).all(|x| buf.cell((x, y)).unwrap().fg != th.err),
        "an ordinary report is news, not an alarm"
    );

    app.alert("error: git worktree add failed");
    let buf = draw(&mut app);
    let y = buf.area.height - 2;
    assert!(
        (0..buf.area.width).any(|x| buf.cell((x, y)).unwrap().fg == th.err),
        "and an error still has to look like one"
    );
}

#[test]
fn a_destructive_prompt_is_colored_as_a_warning() {
    let th = Theme::default();
    let mut app = app_with_tree();
    app.focus = Focus::Checkouts;
    app.sel_checkout = 1;
    app.on_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('D'),
        crossterm::event::KeyModifiers::NONE,
    ));
    let buf = draw(&mut app);
    let text = lines(&buf).join("\n");
    assert!(text.contains("remove checkout?"));
    assert!(
        (0..buf.area.height)
            .any(|y| (0..buf.area.width).any(|x| buf.cell((x, y)).unwrap().fg == th.err)),
        "a removal must not look like an ordinary text field"
    );
}

#[test]
fn a_prompt_draws_over_the_columns_rather_than_beside_them() {
    let mut app = app_with_tree();
    app.on_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('n'),
        crossterm::event::KeyModifiers::NONE,
    ));
    let text = lines(&draw(&mut app)).join("\n");
    assert!(text.contains("add project"));
    assert!(
        text.contains("enter add on ·"),
        "a prompt should say how to commit it"
    );
}

#[test]
fn the_box_finder_reads_the_modal_and_not_the_columns_behind_it() {
    // Guards the four tests below: they would all pass on a broken
    // finder that returned the whole screen.
    let mut app = comment_prompt("just this");
    let rows = box_rows(&draw(&mut app));
    assert!(
        rows.iter().all(|r| !r.contains("projects")),
        "the column titles are outside the box: {rows:?}"
    );
    assert!(rows.iter().any(|r| r.contains("just this")), "{rows:?}");
}

#[test]
fn a_long_comment_wraps_inside_the_box_instead_of_running_off_it() {
    // The bug: one line, no wrap — past the edge of the box you were
    // typing text you could not read back.
    let sentence = "this loop rebuilds the whole row set on every keystroke, \
                    which is fine at ten files and visibly slow at a thousand";
    let mut app = comment_prompt(sentence);
    let buf = draw(&mut app);

    let rows = box_rows(&buf);
    assert!(rows.len() > 3, "the box grew to fit: {rows:?}");
    let typed: String = rows.join(" ");
    for word in ["rebuilds", "keystroke", "thousand"] {
        assert!(
            typed.contains(word),
            "{word:?} should be readable: {rows:?}"
        );
    }
}

#[test]
fn the_box_never_overruns_the_screen_it_floats_over() {
    let mut app = comment_prompt(&"word ".repeat(200));
    let buf = draw(&mut app);
    // `lines` trims the right edge, so an overrun shows up as a row
    // wider than the terminal or a panic in `draw`.
    assert!(lines(&buf).iter().all(|l| l.chars().count() <= buf.area.width as usize));
    assert!(
        lines(&buf).len() <= 20,
        "and it stays a modal rather than becoming the screen"
    );
}

#[test]
fn what_you_are_typing_stays_on_screen_once_the_box_stops_growing() {
    // The tail is kept, not the head: the caret is at the end.
    let mut app = comment_prompt(&format!("{} tailword", "filler ".repeat(300)));
    let buf = draw(&mut app);
    let rows = box_rows(&buf).join(" ");
    assert!(rows.contains("tailword"), "{rows}");
    assert!(rows.contains(CARET), "and the caret with it: {rows}");
}

#[test]
fn the_anchor_gets_its_own_line_rather_than_the_room_to_type_in() {
    let mut app = comment_prompt("make this lazy");
    let buf = draw(&mut app);
    let rows = box_rows(&buf);

    let anchored = rows
        .iter()
        .position(|r| r.contains("ui.rs:1013"))
        .expect("the anchor is shown");
    let typed = rows
        .iter()
        .position(|r| r.contains("make this lazy"))
        .expect("and so is the comment");
    assert!(
        anchored < typed,
        "on separate lines, anchor first: {rows:?}"
    );
}

#[test]
fn a_narrow_terminal_still_gets_a_box_that_fits() {
    let mut app = comment_prompt("nope");
    let buf = draw_at(&mut app, 30, 12);
    assert!(lines(&buf).iter().all(|l| l.chars().count() <= 30));
    assert!(lines(&buf).join("\n").contains("nope"));
}

#[test]
fn the_agent_picker_lists_templates_over_the_tree() {
    let mut app = app_with_tree();
    app.templates = vec!["claude".to_string(), "codex".to_string()];
    app.on_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('a'),
        crossterm::event::KeyModifiers::NONE,
    ));
    let text = lines(&draw(&mut app)).join("\n");
    assert!(text.contains("spawn agent"));
    assert!(text.contains("codex"));
}

#[test]
fn a_narrow_terminal_still_renders_without_panicking() {
    // Panics here take the whole TUI down, and terminals get resized to
    // silly sizes all the time.
    let mut app = app_with_tree();
    for (w, h) in [(20, 6), (10, 4), (4, 3), (1, 1)] {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| render(f, &mut app)).unwrap();
    }
}

/// Not an assertion — a way to look at the UI while working on it:
/// `cargo test -p argus dump_frame -- --ignored --nocapture`. Beats
/// launching the client into a real terminal just to see the layout.
#[test]
#[ignore = "prints a frame for eyeballing; asserts nothing"]
fn dump_frame() {
    let mut app = app_with_tree();
    app.focus = Focus::Checkouts;
    app.templates = vec!["claude".to_string()];
    let w = std::env::var("DUMP_W")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);
    let h = std::env::var("DUMP_H")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    for line in lines(&draw_at(&mut app, w, h)) {
        println!("|{line}");
    }
}

/// Same idea as `dump_frame`, for the two views it doesn't reach.
#[test]
#[ignore]
fn dump_review() {
    let mut app = app_with_review();
    for line in lines(&draw_at(&mut app, 100, 20)) {
        println!("|{line}");
    }
}
