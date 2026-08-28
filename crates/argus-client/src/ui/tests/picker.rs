//! The fuzzy picker.

use super::*;

#[test]
fn a_fuzzy_picker_shows_what_has_been_typed() {
    let mut app = app_with_branch_picker("log");
    let out = lines(&draw(&mut app)).join("\n");
    assert!(out.contains("› log"), "the query line:\n{out}");
}

#[test]
fn an_empty_query_says_what_the_line_is_for() {
    let mut app = app_with_branch_picker("");
    let out = lines(&draw(&mut app)).join("\n");
    assert!(out.contains("type to filter"), "{out}");
}

#[test]
fn only_the_matching_rows_are_drawn() {
    let mut app = app_with_branch_picker("log");
    let out = lines(&draw(&mut app)).join("\n");
    assert!(out.contains("feature/login"), "{out}");
    assert!(
        !out.contains("hotfix"),
        "a non-match should be gone:\n{out}"
    );
}

#[test]
fn the_create_row_is_visible_rather_than_implied() {
    // Creating a branch by pressing Enter on nothing would be a
    // surprise; it gets a row you can see and aim at.
    let mut app = app_with_branch_picker("brand-new");
    let out = lines(&draw(&mut app)).join("\n");
    assert!(out.contains("create brand-new"), "{out}");
}

#[test]
fn a_long_list_scrolls_instead_of_filling_the_screen() {
    let mut app = app_with_tree();
    let items: Vec<String> = (0..200).map(|i| format!("branch-{i:03}")).collect();
    let mut p = crate::app::Picker::new(
        PickerKind::Branch {
            checkout: CheckoutId(10),
        },
        "switch branch",
        items,
        0,
    );
    p.type_query("");
    app.picker = Some(p);

    let buf = draw_at(&mut app, 100, 24);
    let out = lines(&buf).join("\n");
    assert!(out.contains("branch-000"), "{out}");
    assert!(
        !out.contains("branch-199"),
        "the box must stay a modal:\n{out}"
    );
}

#[test]
fn the_short_pickers_keep_their_plain_list() {
    // A query line over four theme names would be clutter.
    let mut app = app_with_tree();
    app.picker = Some(crate::app::Picker::new(
        PickerKind::Theme,
        "theme",
        crate::theme::THEMES.iter().map(|t| t.to_string()).collect(),
        1,
    ));
    let out = lines(&draw(&mut app)).join("\n");
    assert!(!out.contains("type to filter"), "{out}");
    assert!(out.contains("mocha"), "{out}");
}

#[test]
fn only_a_cyclable_setting_shows_the_arrows() {
    // Arrows on free text would promise a carousel that isn't there.
    let mut app = app_with_tree();
    app.open_settings();
    let out = lines(&draw(&mut app)).join("\n");
    assert!(out.contains("‹ floating window ›"), "{out}");
    assert!(
        !out.contains("‹ (from"),
        "the command row is typed, not cycled:\n{out}"
    );
}

#[test]
fn the_settings_panel_says_where_it_saves() {
    let mut app = app_with_tree();
    app.open_settings();
    let out = lines(&draw(&mut app)).join("\n");
    assert!(out.contains("client.toml"), "{out}");
}

#[test]
fn an_editor_never_appears_in_the_panes_column() {
    let mut app = app_with_tree();
    if let Some(c) = app.tree[0].repositories[0].checkouts.get_mut(0) {
        c.panes.push(PaneInfo {
            id: PaneId(700),
            kind: PaneKind::Editor,
            title: "zzz-editor.rs".to_string(),
            status: PaneStatus::Idle,
            note: None,
            template: None,
            children: Vec::new(),
        });
    }
    let out = lines(&draw(&mut app)).join("\n");
    assert!(!out.contains("zzz-editor"), "editors are not panes:\n{out}");
    assert!(out.contains("claude"), "the agent still is:\n{out}");
}

#[test]
fn collapsed_projects_column_renders_as_a_tab() {
    let mut app = app_with_tree();
    app.projects_collapsed = true;
    let buf = draw(&mut app);

    // The tab occupies the left page gutter: one cell wide, the column
    // band tall (so the whole edge is clickable), sitting on x = 0.
    assert_eq!(app.layout.projects.outer.width, 1);
    assert_eq!(app.layout.projects.outer.x, 0);
    assert_eq!(app.layout.repositories.outer.x, 1);

    // The other columns absorb the freed space; the live view is widest.
    assert!(app.layout.repositories.outer.width > 10);
    assert!(app.layout.checkouts.outer.width > 10);
    assert!(app.layout.panes.outer.width > 10);
    assert!(app.layout.content.outer.width > 20);

    let all = lines(&buf);
    let tab_y = app.layout.projects.outer.y as usize;
    let first = |line: &str| line.chars().next().unwrap_or(' ');
    assert_eq!(first(&all[tab_y]), '▸', "disclosure on the tab row");

    // Below the mark the gutter is empty page, not a bordered rail.
    let gutter = app.layout.projects.outer;
    for y in gutter.y..gutter.y.saturating_add(gutter.height) {
        let ch = first(&all[y as usize]);
        if y as usize == tab_y {
            continue;
        }
        assert_ne!(ch, '│', "full-height rail at row {y}");
        assert_ne!(ch, '╭', "card border in the gutter at row {y}");
        assert_ne!(ch, '╰', "card border in the gutter at row {y}");
        assert_eq!(ch, ' ', "gutter below the tab should be empty at row {y}");
    }

    let text = all.join("\n");
    assert!(
        text.contains("argus"),
        "project still named in the breadcrumb"
    );
    assert!(
        !all.iter().any(|line| line.starts_with("argus")),
        "project name must not occupy the gutter"
    );
}

#[test]
fn collapsed_constraints_cede_the_projects_width() {
    // With captured widths, the four survivors keep them and the slack
    // lands in the content column, same as dragging a gutter.
    let total = 100;
    let preferred = Some(vec![12u16, 18, 20, 20, 40]);
    let c = collapsed_projects_constraints(total, preferred.as_deref());
    match c.as_slice() {
        [Constraint::Length(18), Constraint::Length(20), Constraint::Length(20), Constraint::Length(39)] =>
            {}
        other => panic!("unexpected collapsed constraints: {other:?}"),
    }
}

#[test]
fn collapsed_constraints_default_redeals_over_four_columns() {
    let total = 100;
    let c = collapsed_projects_constraints(total, None);
    assert_eq!(c.len(), 4);
    let sum: u16 = c
        .iter()
        .map(|c| match c {
            Constraint::Length(w) => *w,
            _ => panic!("all lengths"),
        })
        .sum();
    assert_eq!(sum + 3, total, "4 columns + 3 gutters = total");
}
