//! The `?` window: that it answers for the mode you are actually in, that
//! it opens over what raised the question rather than instead of it, and
//! that `?` is still a character wherever one is being typed.

use super::*;

fn press(app: &mut App, c: char) {
    app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
}

#[test]
fn the_keys_it_lists_are_the_ones_this_mode_actually_takes() {
    let mut app = app_with_tree();
    press(&mut app, '?');
    let nav = lines(&draw(&mut app)).join("\n");
    assert!(nav.contains("a shell here"), "the column keys:\n{nav}");
    assert!(!nav.contains("line by line"), "not the diff's:\n{nav}");

    app.help = None;
    app.overlay = Some(Overlay::History);
    press(&mut app, '?');
    let history = lines(&draw(&mut app)).join("\n");
    assert!(
        history.contains("commit by commit"),
        "the history keys:\n{history}"
    );
    assert!(!history.contains("a shell here"), "{history}");

    // And what is true everywhere is said in both, given the room.
    let roomy = |app: &mut App| lines(&draw_at(app, 200, 44)).concat();
    assert!(roomy(&mut app).contains("everywhere"));
}

#[test]
fn the_window_opens_over_what_raised_the_question_rather_than_instead_of_it() {
    let mut app = app_with_tree();
    app.overlay = Some(Overlay::History);
    press(&mut app, '?');
    assert!(app.help.is_some());
    assert!(
        matches!(app.overlay, Some(Overlay::History)),
        "asking about the keys should not close what you asked about"
    );

    // Anything that is not a scroll key puts it away again, and leaves
    // what was underneath exactly as it was.
    press(&mut app, 'z');
    assert!(app.help.is_none());
    assert!(matches!(app.overlay, Some(Overlay::History)));
}

#[test]
fn a_question_mark_where_text_is_being_typed_is_a_character() {
    let mut app = app_with_tree();
    app.prompt = Some(Prompt::NewWorktree {
        base: CheckoutId(10),
        input: String::new(),
    });
    press(&mut app, '?');
    assert!(app.help.is_none(), "a prompt takes it as text");
    assert!(matches!(&app.prompt, Some(Prompt::NewWorktree { input, .. }) if input == "?"));
}

#[test]
fn the_bar_stops_advertising_keys_while_the_window_is_up() {
    let mut app = app_with_tree();
    assert!(bar(&draw(&mut app)).contains("? keys"));

    press(&mut app, '?');
    let bar = bar(&draw(&mut app));
    assert!(bar.contains("closes"), "how to work the window: {bar:?}");
    assert!(!bar.contains("? keys"), "and not a key it just listed");
}

#[test]
fn the_window_fits_the_terminal_it_is_drawn_over() {
    let mut app = app_with_tree();
    press(&mut app, '?');
    for (w, h) in [(80u16, 24u16), (120, 20), (200, 40), (60, 12)] {
        let buf = draw_at(&mut app, w, h);
        let panel = app.layout.help.outer;
        assert!(
            panel.right() <= w && panel.bottom() <= h && panel.width > 0,
            "the keymap overran a {w}x{h} terminal: {panel:?}"
        );
        for line in lines(&buf) {
            assert!(line.chars().count() <= w as usize);
        }
    }
}

#[test]
fn a_keymap_taller_than_the_window_scrolls_rather_than_being_cut_off() {
    let mut app = app_with_tree();
    press(&mut app, '?');
    let top = lines(&draw_at(&mut app, 60, 12)).join("\n");
    assert!(top.contains("moving"), "starts at the top:\n{top}");

    for _ in 0..6 {
        press(&mut app, 'j');
    }
    let down = lines(&draw_at(&mut app, 60, 12)).join("\n");
    assert!(app.help.is_some(), "j scrolls rather than closing");
    assert_ne!(top, down, "and something moved");

    // Scrolling past the end is held at it, so the window can never be
    // left showing nothing.
    for _ in 0..40 {
        press(&mut app, 'd');
    }
    let bottom = lines(&draw_at(&mut app, 60, 12)).join("\n");
    assert!(
        bottom.contains("everywhere") || bottom.contains("detach"),
        "the end of the list, not past it:\n{bottom}"
    );
}


