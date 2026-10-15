//! The contextual rail: its heading, repository ordering, expansion and
//! quiet fold, and what a click on each of its rows does.

use super::*;

/// The rail's own columns of a frame, short of its border, so a row is
/// never matched by what the stage happens to say beside it.
fn rail_lines(app: &mut App, width: u16, height: u16) -> Vec<String> {
    let text = lines(&draw_at(app, width, height));
    let rail = usize::from(app.layout.rail.outer.width).saturating_sub(1);
    text.iter().map(|line| line.chars().take(rail).collect()).collect()
}

/// The fixture's project with `quiet` more repositories that have nothing
/// running, after the active `orion`.
fn with_quiet_repositories(quiet: u64) -> App {
    let mut app = command_center();
    for i in 0..quiet {
        app.tree[0].repositories.push(repository(
            20 + i,
            &format!("quiet-{i}"),
            vec![checkout(40 + i, "main", true, vec![])],
        ));
    }
    app
}

#[test]
fn the_heading_is_one_row_and_counts_what_needs_you_only_when_something_does() {
    let mut app = command_center();
    let text = rail_lines(&mut app, 120, 30);
    let top = usize::from(app.layout.rail.outer.y);

    assert!(text[top].starts_with("▌ argus"), "{}", text.join("\n"));
    assert!(!text[top].contains("need"), "nothing is waiting: {}", text[top]);
    assert!(
        text[top + 1].chars().all(|c| c == '─'),
        "one rule under it: {:?}",
        text[top + 1]
    );
    assert!(text[top + 2].contains("orion"), "then the rows: {}", text.join("\n"));
    for gone in ["ACTIVE WORKSPACE", "REPOSITORIES", "REPOS", "/repo"] {
        assert!(!text.join("\n").contains(gone), "{gone} stayed:\n{}", text.join("\n"));
    }

    app.tree[0].repositories[0].checkouts[0].panes[1].status = PaneStatus::Waiting;
    let th = app.theme;
    let buf = draw_at(&mut app, 120, 30);
    let text = rail_lines(&mut app, 120, 30);
    assert!(text[top].trim_end().ends_with(" 1 need"), "{:?}", text[top]);
    assert_eq!(fg_of(&buf, "1 need"), Some(th.warn));
}

#[test]
fn the_rail_draws_no_agents_section() {
    // Tall enough that the section, a third of the rail, had room to spare.
    let mut app = command_center();
    app.focus = Focus::PaneContent;
    let text = rail_lines(&mut app, 140, 40);

    assert!(!text.join("\n").contains("AGENTS"), "{}", text.join("\n"));
    assert!(
        !text.iter().any(|line| line.contains("#100")),
        "an agent is listed once, on its pane's row:\n{}",
        text.join("\n")
    );
    let list = app.layout.rail.inner;
    assert_eq!(
        list.bottom(),
        app.layout.rail.outer.bottom(),
        "the repositories take the height the section had"
    );
}

#[test]
fn a_rail_at_its_minimum_width_neither_panics_nor_spills() {
    let mut app = with_quiet_repositories(3);
    app.tree.push(project(4, "hermes", vec![]));
    let pane = &mut app.tree[0].repositories[0].checkouts[0].panes[0];
    pane.status = PaneStatus::Waiting;
    pane.title = "a title far longer than the rail".into();
    pane.children.push(argus_protocol::ChildAgentInfo {
        label: "a child with a long label too".into(),
        status: PaneStatus::Waiting,
        note: None,
    });
    app.settings.rail_width = Some(1);
    app.focus = Focus::PaneContent;

    for width in [30, 49, 60] {
        let buf = draw_at(&mut app, width, 20);
        let rail = app.layout.rail.outer;
        for y in rail.y..rail.bottom() {
            let edge = buf.cell((rail.right() - 1, y)).map(|c| c.symbol().to_string());
            assert!(
                matches!(edge.as_deref(), Some("│" | "┤" | "├")),
                "row {y} wrote over the rail's border at width {width}: {edge:?}\n{}",
                lines(&buf).join("\n")
            );
        }
    }
}

#[test]
fn a_waiting_child_counts_in_the_need_count_and_the_repository_dot() {
    // The command band counts a child stopped on a question; the rail
    // standing for the same agents has to say the same.
    let mut app = command_center();
    app.tree[0].repositories[0].checkouts[0].panes[0]
        .children
        .push(argus_protocol::ChildAgentInfo {
            label: "reviewer".into(),
            status: PaneStatus::Waiting,
            note: None,
        });
    let th = app.theme;
    let buf = draw_at(&mut app, 120, 30);
    let text = lines(&buf).join("\n");

    assert!(text.contains(" 1 need"), "{text}");
    assert_eq!(fg_of(&buf, "● orion"), Some(th.warn), "{text}");
}

#[test]
fn a_failed_exit_outranks_a_working_agent_on_the_repository_dot() {
    let mut app = command_center();
    app.tree[0].repositories[0].checkouts[0].panes[1].status = PaneStatus::Exited { code: Some(1) };
    let th = app.theme;
    let buf = draw_at(&mut app, 120, 30);

    assert_eq!(
        fg_of(&buf, "● orion"),
        Some(th.warn),
        "{}",
        lines(&buf).join("\n")
    );
}

#[test]
fn an_editor_never_appears_among_the_rails_panes() {
    // An editor is a way of looking at a file, not something running in
    // the checkout that you might come back to.
    let mut app = command_center();
    app.tree[0].repositories[0].checkouts[0]
        .panes
        .push(editor(700, "zzz-editor.rs"));
    app.choose_rail_row(crate::app::RailRow::Repository(0));
    let out = lines(&draw_at(&mut app, 120, 30)).join("\n");

    assert!(!out.contains("zzz-editor"), "editors are not panes:\n{out}");
    assert!(out.contains("claude"), "the agent still is:\n{out}");
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
    let row = app.layout.rail.inner.y;
    app.on_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: app.layout.rail.inner.x + 2,
        row,
        modifiers: KeyModifiers::NONE,
    });
    let text = rail_lines(&mut app, 120, 30);
    let at = |start: &str| text.iter().position(|line| line.starts_with(start));

    // A checkout four columns in, its panes six, and no elbows drawn.
    let master = at("    master").unwrap_or_else(|| panic!("{}", text.join("\n")));
    // A working agent shows the spinner in place of its still mark.
    let agent: Vec<char> = text[master + 1].chars().collect();
    assert!(
        agent[..6].iter().all(|c| *c == ' ') && agent[6] != ' ' && agent[6] != '◆',
        "{}",
        text[master + 1]
    );
    assert!(text[master + 1].contains(" claude"), "{}", text[master + 1]);
    assert_eq!(at("      › shell"), Some(master + 2), "{}", text.join("\n"));
    assert!(!text.join("\n").contains('└'), "{}", text.join("\n"));
}

#[test]
fn the_rails_checkout_row_shows_ahead_and_staged_while_dirty() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

    let mut app = command_center();
    let mut status = git(Some("master"), true, 3, 2, 0);
    (status.staged, status.unstaged) = (2, 1);
    app.tree[0].repositories[0].checkouts[0].git = Some(status);
    draw_at(&mut app, 120, 30);
    let row = app.layout.rail.inner.y;
    app.on_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: app.layout.rail.inner.x + 2,
        row,
        modifiers: KeyModifiers::NONE,
    });
    let text = rail_lines(&mut app, 120, 30);

    assert!(
        text.iter().any(|line| line.starts_with("    master ↑2 +2 !1")),
        "{}",
        text.join("\n")
    );
}

#[test]
fn a_folded_checkout_counts_the_panes_it_hides() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

    let mut app = command_center();
    app.tree[0].repositories[0].checkouts[0].panes[1].status = PaneStatus::Waiting;
    draw_at(&mut app, 120, 30);
    let click = |app: &mut App, row: u16| {
        app.on_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: app.layout.rail.inner.x + 2,
            row,
            modifiers: KeyModifiers::NONE,
        })
    };
    let repository = app.layout.rail.inner.y;
    click(&mut app, repository);
    draw_at(&mut app, 120, 30);
    // The first click picks the checkout, the second folds it.
    click(&mut app, repository + 1);
    click(&mut app, repository + 1);
    let th = app.theme;
    let buf = draw_at(&mut app, 120, 30);
    let text = rail_lines(&mut app, 120, 30);

    let row = text
        .iter()
        .position(|line| line.starts_with("▌ ▸ master"))
        .unwrap_or_else(|| panic!("{}", text.join("
")));
    let master = text[row].trim_end();
    assert!(master.ends_with(" 2"), "{master:?}");
    assert!(!text.join("
").contains("shell"), "{}", text.join("
"));
    // The loudest of what it hides colours the count.
    let count = (master.chars().count() - 1) as u16;
    let fg = buf.cell((count, row as u16)).map(|c| c.fg);
    assert_eq!(fg, Some(status_color(PaneStatus::Waiting, th)));
}

#[test]
fn current_pane_is_highlighted_in_an_expanded_repository() {
    let mut app = command_center();
    app.focus = Focus::PaneContent;
    let th = app.theme;
    let buf = draw_at(&mut app, 120, 30);
    let text = rail_lines(&mut app, 120, 30);

    let row = text
        .iter()
        .position(|line| line.starts_with("▌     ") && line.contains(" claude"))
        .unwrap_or_else(|| panic!("{}", text.join("\n")));
    let bg = |y: usize| buf.cell((1, y as u16)).map(|c| c.bg);
    assert_eq!(bg(row), Some(th.surface_focus));
    // One highlight: the selected repository and the rest of what it holds
    // are told by the bar and the bold name, not by a band behind them.
    for other in [row - 2, row - 1, row + 1] {
        assert!(
            ![Some(th.surface), Some(th.surface_focus)].contains(&bg(other)),
            "{}",
            text[other]
        );
    }
}

#[test]
fn the_primary_checkout_keeps_its_row_with_nothing_running_and_another_does_not() {
    let mut app = command_center();
    app.tree[0].repositories[0].checkouts = vec![
        checkout(10, "master", true, vec![]),
        checkout(
            11,
            "feat",
            false,
            vec![pane_info(100, PaneKind::Agent, "claude", PaneStatus::Working)],
        ),
        checkout(12, "stale", false, vec![]),
    ];
    app.choose_rail_row(crate::app::RailRow::Repository(0));
    let text = rail_lines(&mut app, 120, 30);
    let lists = |branch: &str| text.iter().any(|line| line.starts_with(&format!("    {branch}")));

    assert!(lists("master") && lists("feat"), "{}", text.join("\n"));
    assert!(!lists("stale"), "{}", text.join("\n"));

    // Its row is one like any other: the cursor stops on it, and a shell
    // or an agent started there starts in it.
    app.on_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    assert_eq!(app.rail_cursor(), Some(crate::app::RailRow::Checkout(0, 0)));
    assert_eq!(app.current_checkout().map(|c| c.id), Some(CheckoutId(10)));
}

#[test]
fn clicking_a_repository_leaves_the_others_open_and_a_second_click_folds_it() {
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
    let x = app.layout.rail.inner.x + 2;
    let y = app.layout.rail.inner.y;
    click(&mut app, x, y);
    let text = rail_lines(&mut app, 120, 30);
    let lists = |text: &[String], branch: &str| {
        text.iter().any(|line| line.starts_with(&format!("    {branch}")))
    };
    assert!(lists(&text, "master") && !lists(&text, "trunk"), "{}", text.join("\n"));

    let second = text
        .iter()
        .position(|line| line.contains("second"))
        .expect("second repository row") as u16;
    click(&mut app, x, second);
    let text = rail_lines(&mut app, 120, 30);
    assert!(lists(&text, "trunk") && lists(&text, "master"), "{}", text.join("\n"));

    click(&mut app, x, second);
    let text = rail_lines(&mut app, 120, 30);
    assert!(!lists(&text, "trunk") && lists(&text, "master"), "{}", text.join("\n"));
}

#[test]
fn clicking_a_pane_in_the_rail_keeps_keys_on_the_rail_so_x_closes_it() {
    let mut app = command_center();
    draw_at(&mut app, 120, 30);
    let x = app.layout.rail.inner.x + 2;
    let y = app.layout.rail.inner.y;
    click(&mut app, x, y);
    let row = rail_lines(&mut app, 120, 30)
        .iter()
        .position(|line| line.starts_with("      ") && line.contains(" claude"))
        .expect("pane row") as u16;
    click(&mut app, x, row);

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
    let border = app.layout.rail.outer.right() as usize - 1;
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
    // The heading is one row: the rule under it is not part of the target.
    let outer = app.layout.rail.outer;
    click(&mut app, outer.x + 4, outer.y + 1);
    assert!(app.picker.is_none(), "the rule is not the heading");
    click(&mut app, outer.x + 4, outer.y);
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
fn dragging_the_rails_border_resizes_it_and_the_stage_follows() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

    let mut app = command_center();
    draw_at(&mut app, 120, 30);
    let edge = app.layout.rail.outer.right() - 1;
    let row = app.layout.rail.inner.y + 2;
    let mouse = |kind, column| MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    };
    app.on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), edge));
    app.on_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), 69));
    app.on_mouse(mouse(MouseEventKind::Up(MouseButton::Left), 69));
    draw_at(&mut app, 120, 30);

    assert_eq!(app.layout.rail.outer.width, 70);
    assert!(app.layout.terminal.outer.x >= 70);
    assert!(!app.rail_drag, "the release lets go of the border");
}

#[test]
fn the_rail_cannot_be_dragged_narrower_than_its_minimum_or_past_the_stage() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

    let mut app = command_center();
    draw_at(&mut app, 120, 30);
    let edge = app.layout.rail.outer.right() - 1;
    let row = app.layout.rail.inner.y + 2;
    let mouse = |kind, column| MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    };
    app.on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), edge));
    app.on_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), 2));
    draw_at(&mut app, 120, 30);
    assert_eq!(app.layout.rail.outer.width, crate::ui::RAIL_MIN_WIDTH);

    app.on_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), 119));
    draw_at(&mut app, 120, 30);
    assert_eq!(app.layout.rail.outer.width, 120 - 24);
}

/// The fixture's agent idle at its prompt while a CLI it started runs.
fn agent_with_a_running_child() -> App {
    let mut app = command_center();
    let pane = &mut app.tree[0].repositories[0].checkouts[0].panes[0];
    pane.status = PaneStatus::Idle;
    pane.children.push(argus_protocol::ChildAgentInfo {
        label: "codex review".into(),
        status: PaneStatus::Working,
        note: None,
    });
    app
}

/// The agent's row in the rail, when it is drawn working: the spinner in
/// the ok colour where its still mark would be.
fn working_agent_row(app: &mut App) -> Option<String> {
    let th = app.theme;
    let buf = draw_at(app, 140, 40);
    let rail = app.layout.rail.inner;
    (rail.y..rail.bottom()).map(|y| (y, row_text(&buf, y, rail))).find_map(|(y, row)| {
        let glyph = buf.cell((rail.x + 6, y))?;
        let working = row.contains(" claude") && glyph.symbol() != "◆" && glyph.fg == th.ok;
        working.then_some(row)
    })
}

#[test]
fn an_open_repository_lists_a_panes_children_under_it_and_the_cursor_steps_over_them() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = agent_with_a_running_child();
    draw_at(&mut app, 120, 30);
    let list = app.layout.rail.inner;
    click(&mut app, list.x + 2, list.y);
    let text = rail_lines(&mut app, 120, 30);

    let agent = text
        .iter()
        .position(|line| line.starts_with("      ") && line.contains(" claude"))
        .unwrap();
    assert!(!text[agent].contains('◆'), "the child's work is the agent's: {}", text[agent]);
    // Two columns further in than its pane, and working is not a state
    // that asks to be spelled out.
    assert_eq!(text[agent + 1].trim_end(), "        ⤷ codex review", "{}", text.join("\n"));
    assert!(text[agent + 2].starts_with("      › shell"), "{}", text.join("\n"));

    // A click on the child lands on its agent; the keys step past it.
    click(&mut app, list.x + 12, agent as u16 + 1);
    assert_eq!(app.current_pane().map(|pane| pane.id), Some(PaneId(100)));
    app.on_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    assert_eq!(app.current_pane().map(|pane| pane.id), Some(PaneId(101)));
}

#[test]
fn an_agent_idle_while_its_workflow_runs_reads_as_working_with_no_row_for_it() {
    let mut app = command_center();
    let pane = &mut app.tree[0].repositories[0].checkouts[0].panes[0];
    pane.status = PaneStatus::Idle;
    pane.background = true;
    app.focus = Focus::PaneContent;
    let text = lines(&draw_at(&mut app, 140, 40));

    assert!(working_agent_row(&mut app).is_some(), "{}", text.join("\n"));
    let header = text.iter().find(|line| line.contains("claude #100")).unwrap();
    assert!(header.contains("RUNNING") && !header.contains("IDLE"), "{header}");
    assert!(!text.iter().any(|line| line.contains("⤷")), "{}", text.join("\n"));
}

#[test]
fn an_agent_idle_while_its_child_works_reads_as_working() {
    let mut app = agent_with_a_running_child();
    app.focus = Focus::PaneContent;
    let text = lines(&draw_at(&mut app, 140, 40));

    assert!(working_agent_row(&mut app).is_some(), "{}", text.join("\n"));
    let header = text.iter().find(|line| line.contains("claude #100")).unwrap();
    assert!(header.contains("RUNNING") && !header.contains("IDLE"), "{header}");
}

#[test]
fn a_pane_row_writes_its_status_word_only_when_it_needs_you() {
    use PaneStatus::*;

    let mut app = command_center();
    app.focus = Focus::Panes;
    let th = app.theme;
    let quiet = [Idle, Working, Done, Exited { code: Some(0) }];
    let loud = [(Waiting, "NEEDS YOU"), (NeedsReview, "REVIEW"), (Failed, "FAILED")];
    let words = ["IDLE", "RUNNING", "DONE", "EXITED", "NEEDS YOU", "REVIEW", "FAILED"];
    let mut agent_row = |status| {
        app.tree[0].repositories[0].checkouts[0].panes[0].status = status;
        let buf = draw_at(&mut app, 120, 30);
        let rail = app.layout.rail.inner;
        let y = (rail.y..rail.bottom())
            .find(|y| row_text(&buf, *y, rail).contains(" claude"))
            .expect("the agent's row");
        (row_text(&buf, y, rail), buf.cell((rail.x + 6, y)).map(|c| c.fg))
    };

    for status in quiet {
        let (row, glyph) = agent_row(status);
        assert!(!words.iter().any(|word| row.contains(word)), "{status:?}: {row}");
        assert_eq!(glyph, Some(crate::ui::command_center::status_color(status, th)), "the glyph still says it");
    }
    for (status, word) in loud {
        let (row, _) = agent_row(status);
        assert!(row.trim_end().ends_with(&format!(" {word}")), "{status:?}: {row}");
    }
}

#[test]
fn a_child_row_writes_its_status_word_only_when_it_needs_you() {
    let mut app = agent_with_a_running_child();
    app.focus = Focus::Panes;
    let child_row = |app: &mut App| {
        let text = rail_lines(app, 120, 30);
        text.iter().find(|line| line.contains("⤷ codex review")).unwrap().trim_end().to_string()
    };
    assert_eq!(child_row(&mut app), "        ⤷ codex review");

    app.tree[0].repositories[0].checkouts[0].panes[0].children[0].status = PaneStatus::Waiting;
    assert!(child_row(&mut app).ends_with(" NEEDS YOU"), "{}", child_row(&mut app));
}

#[test]
fn a_repository_row_ends_in_its_bare_pane_count_and_a_quiet_one_in_nothing() {
    let mut app = with_quiet_repositories(1);
    let text = rail_lines(&mut app, 120, 30);

    let orion = text.iter().find(|line| line.contains("orion")).unwrap();
    assert!(orion.starts_with('▌'), "{orion}");
    assert_eq!(orion.split_whitespace().last(), Some("2"), "{orion}");
    let quiet = text.iter().find(|line| line.contains("quiet-0")).unwrap();
    assert_eq!(quiet.trim(), "· quiet-0", "{quiet}");
    assert!(!text.join("\n").contains("checkout"), "{}", text.join("\n"));
}

// --- the divider ---------------------------------------------------------

#[test]
fn a_rule_divides_the_active_repositories_from_the_quiet_ones() {
    let mut app = with_quiet_repositories(3);
    let th = app.theme;
    let buf = draw_at(&mut app, 120, 30);
    let text = rail_lines(&mut app, 120, 30);
    let list = usize::from(app.layout.rail.inner.y);

    assert!(text[list].contains("orion"), "{}", text.join("\n"));
    assert!(text[list + 1].trim().chars().all(|c| c == '─'), "{}", text.join("\n"));
    assert_eq!(buf.cell((1, (list + 1) as u16)).map(|c| c.fg), Some(th.edge));
    for (i, row) in text[list + 2..list + 5].iter().enumerate() {
        assert!(row.contains(&format!("quiet-{i}")), "{}", text.join("\n"));
    }
}

#[test]
fn no_rule_is_drawn_when_no_repository_is_active() {
    let mut app = with_quiet_repositories(3);
    app.tree[0].repositories[0].checkouts[0].panes.clear();
    let text = rail_lines(&mut app, 120, 30);
    let top = usize::from(app.layout.rail.inner.y);
    let list = &text[top..top + 4];
    let text = text.join("\n");

    for name in ["orion", "quiet-0", "quiet-1", "quiet-2"] {
        assert!(text.contains(name), "{name} is missing:\n{text}");
    }
    assert!(!list.iter().any(|row| row.contains("──")), "nothing to divide:\n{text}");
}

#[test]
fn a_click_on_the_rule_lands_on_nothing() {
    let mut app = with_quiet_repositories(3);
    app.focus = Focus::Repositories;
    draw_at(&mut app, 120, 30);
    let (x, rule) = (app.layout.rail.inner.x + 3, app.layout.rail.inner.y + 1);
    click(&mut app, x, rule);

    assert_eq!(app.rail_cursor(), Some(crate::app::RailRow::Repository(0)));
}

/// The fixture with a daemon that keeps consoles, running `titles`.
fn with_consoles(titles: &[&str]) -> App {
    let mut app = command_center();
    app.greeted(Some(&argus_protocol::Hello::this_build()));
    let consoles = titles
        .iter()
        .zip(900..)
        .map(|(title, id)| pane_info(id, PaneKind::Shell, title, PaneStatus::Idle))
        .collect();
    app.on_server_msg(argus_protocol::ServerMsg::Consoles(consoles));
    app
}

#[test]
fn the_consoles_sit_at_the_foot_of_the_rail_under_their_rule() {
    let mut app = with_consoles(&["zsh", "pwsh"]);
    let text = rail_lines(&mut app, 120, 30);
    let foot = app.layout.consoles;
    let rule = usize::from(foot.outer.y);

    assert_eq!(foot.outer.bottom(), app.layout.rail.outer.bottom(), "{}", text.join("\n"));
    assert_eq!(app.layout.rail.inner.bottom(), foot.outer.y, "the repositories end where it starts");
    assert!(text[rule].contains("consoles"), "{}", text.join("\n"));
    assert!(text[rule + 1].contains("zsh"), "{}", text.join("\n"));
    assert!(text[rule + 2].contains("pwsh"), "{}", text.join("\n"));
}

#[test]
fn a_daemon_without_consoles_draws_no_section_for_them() {
    let mut app = command_center();
    let text = rail_lines(&mut app, 120, 30).join("\n");
    assert!(!text.contains("consoles"), "{text}");
    assert_eq!(app.layout.consoles.outer.height, 0);
}

#[test]
fn with_no_console_running_the_foot_says_how_to_start_one() {
    let mut app = with_consoles(&[]);
    let text = rail_lines(&mut app, 120, 30).join("\n");
    assert!(text.contains("consoles"), "{text}");
    assert!(text.contains("c starts one"), "{text}");
}

#[test]
fn a_click_on_a_console_shows_it_and_lights_its_row() {
    let mut app = with_consoles(&["zsh", "pwsh"]);
    draw_at(&mut app, 120, 30);
    let (x, y) = (app.layout.consoles.inner.x + 2, app.layout.consoles.inner.y + 1);
    click(&mut app, x, y);

    assert_eq!(app.column_pane(), Some(argus_protocol::PaneId(901)));
    assert_eq!(app.rail_cursor(), Some(crate::app::RailRow::Console(1)));
    let th = app.theme;
    let buf = draw_at(&mut app, 120, 30);
    let bg = |row: u16| buf.cell((x + 4, app.layout.consoles.inner.y + row)).map(|c| c.bg);
    assert_eq!(bg(1), Some(th.surface_focus));
    assert_ne!(bg(0), Some(th.surface_focus));
    let text = lines(&buf).join("\n");
    assert!(text.contains("console / pwsh"), "the stage's crumb names it:\n{text}");
}

#[test]
fn many_consoles_take_no_more_than_half_the_rail() {
    let titles: Vec<String> = (0..30).map(|i| format!("console-{i}")).collect();
    let titles: Vec<&str> = titles.iter().map(String::as_str).collect();
    let mut app = with_consoles(&titles);
    draw_at(&mut app, 120, 30);
    let below_heading = app.layout.rail.outer.height - 1;
    assert!(app.layout.consoles.outer.height <= below_heading / 2);
    assert!(app.layout.rail.inner.height > 0, "the repositories keep rows");
}

/// The rail with consoles at its foot, one of them on the stage:
/// `cargo test -p argus dump_rail_consoles -- --ignored --nocapture`.
#[test]
#[ignore = "prints a frame for eyeballing; asserts nothing"]
fn dump_rail_consoles() {
    let mut app = with_consoles(&["zsh", "pwsh", "build watcher"]);
    app.consoles[2].status = PaneStatus::Waiting;
    app.choose_rail_row(crate::app::RailRow::Console(1));
    for line in lines(&draw_at(&mut app, 100, 24)) {
        println!("|{line}");
    }
}
