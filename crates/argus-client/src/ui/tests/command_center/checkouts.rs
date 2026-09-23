//! The Checkouts stage: the table, its filter and scrolling, and clicks.

use super::*;

#[test]
fn checkout_overview_uses_the_designs_operational_table() {
    let mut app = command_center();
    app.open_view(View::Checkouts);
    // The wide table needs enough stage width once the rail is drawn.
    let text = lines(&draw_at(&mut app, 140, 30)).join("\n");

    assert!(text.contains("checkouts & worktrees"), "{text}");
    for heading in ["BRANCH", "STATE", "PANES", "PATH"] {
        assert!(text.contains(heading), "missing {heading}:\n{text}");
    }
    assert!(text.contains("B BRANCHES · m CHECKOUT · n WORKTREE"), "{text}");
    assert!(text.contains("master"), "{text}");
}

#[test]
fn b_lists_branch_only_rows_in_the_checkouts_table() {
    let mut app = command_center();
    app.tree[0].repositories[0].branches = vec!["spike".into()];
    app.open_view(View::Checkouts);
    let hidden = lines(&draw_at(&mut app, 120, 30)).join("\n");
    assert!(
        !hidden.contains("spike"),
        "branch-only rows stay out until expanded:\n{hidden}"
    );

    app.on_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('B'),
        crossterm::event::KeyModifiers::NONE,
    ));
    let shown = lines(&draw_at(&mut app, 120, 30)).join("\n");
    assert!(shown.contains("spike"), "{shown}");
    assert!(shown.contains("no checkout"), "{shown}");
}

#[test]
fn a_long_checkouts_table_scrolls_with_the_selection() {
    let mut app = command_center();
    {
        let r = &mut app.tree[0].repositories[0];
        r.checkouts = (0..12)
            .map(|i| argus_protocol::CheckoutInfo {
                id: argus_protocol::CheckoutId(20 + i),
                name: format!("branch-{i:02}"),
                path: format!("/repo/wt-{i}"),
                primary: i == 0,
                git: None,
                panes: Vec::new(),
            })
            .collect();
    }
    app.open_view(View::Checkouts);
    app.sel_checkout = app.checkout_table_row_indices().len() - 1;
    draw_at(&mut app, 120, 24);
    assert!(
        app.layout.checkouts.first > 0,
        "the table should scroll when the selection is below the fold"
    );
}

#[test]
fn slash_filters_checkouts_by_branch_name() {
    let mut app = command_center();
    {
        let r = &mut app.tree[0].repositories[0];
        r.checkouts = vec![
            checkout(20, "alpha", true, vec![]),
            checkout(21, "beta", false, vec![]),
            checkout(22, "alphabet", false, vec![]),
        ];
    }
    app.open_view(View::Checkouts);
    app.on_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('/'),
        crossterm::event::KeyModifiers::NONE,
    ));
    app.on_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('a'),
        crossterm::event::KeyModifiers::NONE,
    ));
    app.on_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('l'),
        crossterm::event::KeyModifiers::NONE,
    ));
    app.on_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('p'),
        crossterm::event::KeyModifiers::NONE,
    ));
    app.on_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    ));
    let indices = app.checkout_table_row_indices();
    assert_eq!(indices.len(), 2, "alpha and alphabet match");
    let text = lines(&draw_at(&mut app, 120, 30)).join("\n");
    assert!(text.contains("alpha"), "{text}");
    assert!(text.contains("alphabet"), "{text}");
    assert!(!text.contains("beta"), "{text}");
}

#[test]
fn slash_filter_shows_where_you_are_searching() {
    let mut app = command_center();
    app.open_view(View::Checkouts);
    app.on_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('/'),
        crossterm::event::KeyModifiers::NONE,
    ));
    app.on_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('a'),
        crossterm::event::KeyModifiers::NONE,
    ));
    let text = lines(&draw_at(&mut app, 120, 30)).join("\n");
    assert!(
        text.contains("filter /a"),
        "the checkouts stage should name the active query:\n{text}"
    );
    assert!(
        text.contains("argus › orion · filter /a"),
        "the status bar should name project, repository, and query:\n{text}"
    );
}

#[test]
fn clicking_a_checkout_row_selects_the_row_under_the_pointer() {
    // A default branch with no directory gets a navigation row of its own
    // that the table does not draw; clicks used to land one row short.
    let mut app = command_center();
    {
        let repo = &mut app.tree[0].repositories[0];
        repo.default_branch = Some("main".into());
        repo.branches = vec!["main".into()];
    }
    app.open_view(View::Checkouts);
    let text = lines(&draw_at(&mut app, 120, 30));
    let rows = app.layout.checkouts.inner;
    for (index, name) in ["master", "feat"].iter().enumerate() {
        let top = rows.y + index as u16 * 3;
        // Every line of the row is part of it, and its text is on the middle one.
        assert!(text[top as usize + 1].contains(name), "{}", text.join("\n"));
        for y in top..top + 3 {
            click(&mut app, rows.x + 4, y);
            assert_eq!(
                app.current_checkout().map(|c| c.name.as_str()),
                Some(*name),
                "click on line {y}"
            );
        }
    }
}

#[test]
fn a_long_checkout_path_keeps_its_end() {
    let mut app = command_center();
    app.tree[0].repositories[0].checkouts[0].path =
        "/very/long/prefix/that/will/not/fit/in/the/column/at/all/final-segment".into();
    app.open_view(View::Checkouts);
    let text = lines(&draw_at(&mut app, 120, 30)).join("\n");
    assert!(text.contains("final-segment"), "{text}");
}
