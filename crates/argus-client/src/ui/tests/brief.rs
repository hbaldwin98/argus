//! The brief window.

use super::*;

#[test]
fn the_brief_window_draws_its_text_as_prose() {
    let mut app = app_with_a_brief("# Plan\n- [ ] stays as written");
    let rendered = lines(&draw(&mut app)).join("\n");

    assert!(rendered.contains("brief · The pty"), "{rendered}");
    assert!(rendered.contains("# Plan"), "prose is drawn as written");
    assert!(rendered.contains("- [ ] stays as written"), "{rendered}");
}

#[test]
fn an_empty_brief_says_how_to_start_one() {
    let mut app = app_with_a_brief("");
    let rendered = lines(&draw(&mut app)).join("\n");
    assert!(rendered.contains("press i to write something"), "{rendered}");
}

#[test]
fn the_title_says_which_mode_the_brief_is_in() {
    let mut app = app_with_a_brief("x");
    assert!(lines(&draw(&mut app)).join("\n").contains("i edit"));

    app.brief.as_mut().unwrap().insert_mode();
    let rendered = lines(&draw(&mut app)).join("\n");
    assert!(rendered.contains("INSERT"), "{rendered}");
    assert!(rendered.contains("esc to save"), "{rendered}");
}

#[test]
fn the_status_bar_offers_the_briefs_own_keys_while_it_is_up() {
    let mut app = app_with_a_brief("x");
    let rendered = lines(&draw(&mut app)).join("\n");
    assert!(rendered.contains("i insert"), "{rendered}");
    assert!(!rendered.contains("space tick"), "{rendered}");
    assert!(!rendered.contains("ctrl-v paste"), "not the pane keymap");

    app.brief.as_mut().unwrap().insert_mode();
    let rendered = lines(&draw(&mut app)).join("\n");
    assert!(rendered.contains("esc to stop and save"), "{rendered}");
}
