//! The command band: which keys it offers, and how it gives way on a
//! narrow terminal.

use super::*;

#[test]
fn a_narrow_bar_shortens_the_keymap_rather_than_cutting_it() {
    let mut app = app_with_tree();
    app.focus = Focus::Checkouts;

    let wide = bar(&draw_at(&mut app, 200, 24));
    assert!(
        wide.contains("F fetch") && wide.contains("H history"),
        "{wide:?}"
    );

    for width in [60u16, 70, 80, 100, 120] {
        let bar = bar(&draw_at(&mut app, width, 24));
        assert!(
            bar.chars().count() <= width as usize,
            "the bar overran {width}: {bar:?}"
        );
        // Whatever tier survived, it ends on a whole word rather than
        // wherever the width happened to land -- and the way to the rest
        // of the keys is the one thing no width drops.
        assert!(
            bar.trim_end().ends_with("? keys"),
            "the way to the full keymap went missing at {width}: {bar:?}"
        );
    }
}

#[test]
fn the_directory_browser_gets_its_own_keys_on_the_bar() {
    // It used to fall through to the rail's, advertising keys that did
    // nothing while the browser had them.
    let mut app = app_with_tree();
    app.on_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
    assert!(app.dir_picker.is_some());
    let bar = bar(&draw_at(&mut app, 160, 30));
    assert!(bar.contains("enter choose"), "{bar}");
    assert!(!bar.contains("n add"), "{bar}");
}
