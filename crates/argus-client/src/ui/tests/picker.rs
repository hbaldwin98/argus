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
