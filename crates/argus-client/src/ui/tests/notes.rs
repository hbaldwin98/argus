//! The note window.

use super::*;

#[test]
fn the_note_window_draws_its_text_with_the_boxes_picked_out() {
    let mut app = app_with_a_note("# Plan
- [ ] open one
- [x] done one
- [!] pinned one");
    let rendered = lines(&draw(&mut app)).join("
");

    assert!(rendered.contains("note · master"), "{rendered}");
    assert!(rendered.contains("# Plan"), "prose is drawn as written");
    assert!(rendered.contains("☐ open one"), "{rendered}");
    assert!(rendered.contains("☑ done one"), "{rendered}");
    assert!(rendered.contains("★ pinned one"), "{rendered}");
}

#[test]
fn the_note_window_counts_what_is_in_it() {
    let mut app = app_with_a_note("- [ ] a
- [ ] b
- [x] c
- [!] d");
    let rendered = lines(&draw(&mut app)).join("
");
    assert!(rendered.contains("☐2"), "{rendered}");
    assert!(rendered.contains("★1"), "{rendered}");
    assert!(rendered.contains("☑1"), "{rendered}");
}

#[test]
fn an_empty_note_says_how_to_start_one() {
    let mut app = app_with_a_note("");
    let rendered = lines(&draw(&mut app)).join("
");
    assert!(rendered.contains("press i to write something"), "{rendered}");
}

#[test]
fn the_title_says_which_mode_the_note_is_in() {
    let mut app = app_with_a_note("x");
    assert!(lines(&draw(&mut app)).join("
").contains("i edit"));

    app.notes.as_mut().unwrap().insert_mode();
    let rendered = lines(&draw(&mut app)).join("
");
    assert!(rendered.contains("INSERT"), "{rendered}");
    assert!(rendered.contains("esc to save"), "{rendered}");
}

#[test]
fn a_refused_write_replaces_the_counts_with_the_reason() {
    let mut app = app_with_a_note("- [ ] a");
    app.notes.as_mut().unwrap().error = Some("note exceeds 65536 bytes".to_string());
    let rendered = lines(&draw(&mut app)).join("
");
    assert!(rendered.contains("not saved: note exceeds"), "{rendered}");
}

#[test]
fn a_checkout_with_open_items_says_so_from_its_row() {
    let mut app = app_with_tree();
    app.tree[0].repositories[0].checkouts[0].notes = NoteCounts {
        open: 3,
        done: 1,
        pinned: 0,
    };
    app.tree[0].repositories[0].checkouts[0].has_note = true;
    let rendered = lines(&draw_at(&mut app, 160, 20)).join("
");
    assert!(rendered.contains("☐3"), "{rendered}");
}

#[test]
fn a_note_with_nothing_to_count_still_marks_its_row() {
    // "Nothing open" and "nothing written down" are different answers,
    // and the row has to tell them apart.
    let mut app = app_with_tree();
    app.tree[0].repositories[0].checkouts[0].has_note = true;
    let rendered = lines(&draw_at(&mut app, 160, 20)).join("
");
    assert!(rendered.contains("✎"), "{rendered}");
}

#[test]
fn a_row_with_no_note_says_nothing_about_notes() {
    let mut app = app_with_tree();
    let rendered = lines(&draw_at(&mut app, 160, 20)).join("
");
    assert!(!rendered.contains("✎"), "{rendered}");
    assert!(!rendered.contains("☐"), "{rendered}");
}

#[test]
fn the_projects_column_shows_what_is_owed_beneath_it() {
    let mut app = app_with_tree();
    app.tree[0].repositories[0].checkouts[1].notes = NoteCounts {
        open: 4,
        done: 0,
        pinned: 0,
    };
    app.tree[0].repositories[0].checkouts[1].has_note = true;
    let rendered = lines(&draw_at(&mut app, 160, 20));
    let project_row = rendered
        .iter()
        .find(|r| r.contains("repository"))
        .expect("the project detail line");
    assert!(project_row.contains("☐4"), "{project_row}");
}

#[test]
fn the_status_bar_offers_the_notes_own_keys_while_it_is_up() {
    let mut app = app_with_a_note("- [ ] a");
    let rendered = lines(&draw(&mut app)).join("
");
    assert!(rendered.contains("space tick"), "{rendered}");
    assert!(!rendered.contains("ctrl-v paste"), "not the pane keymap");

    app.notes.as_mut().unwrap().insert_mode();
    let rendered = lines(&draw(&mut app)).join("
");
    assert!(rendered.contains("esc to stop and save"), "{rendered}");
}

#[test]
#[ignore]
fn dump_note() {
    let mut app = app_with_a_note("# Plan

- [!] read the design doc first
- [ ] wire the store
- [x] parse the checkboxes

prose about the plan");
    for line in lines(&draw_at(&mut app, 100, 20)) {
        println!("|{line}");
    }
}

#[test]
#[ignore]
fn dump_split_review() {
    let mut app = app_with_diff(
        true,
        vec![
            diff_line(
                argus_protocol::LineKind::Context,
                Some(9),
                Some(9),
                "fn f() {",
            ),
            diff_line(
                argus_protocol::LineKind::Removed,
                Some(10),
                None,
                "    let x = old();",
            ),
            diff_line(
                argus_protocol::LineKind::Removed,
                Some(11),
                None,
                "    drop(x);",
            ),
            diff_line(
                argus_protocol::LineKind::Added,
                None,
                Some(10),
                "    let x = new();",
            ),
            diff_line(argus_protocol::LineKind::Context, Some(12), Some(11), "}"),
        ],
    );
    for line in lines(&draw_at(&mut app, 120, 20)) {
        println!("|{line}");
    }
}
