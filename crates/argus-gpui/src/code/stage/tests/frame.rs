//! The stage's frame: when it is drawn, its strip, three columns and their sashes (CODE-4 to 6, 11).

use argus_desktop_core::connection::page::Event;

use gpui_kit::TestAppContext;

use super::*;
use argus_desktop_core::chrome::strip::Part;

/// CODE-4: before the first page, and while it is not open, four loading bars stand in the columns.
#[gpui_kit::test]
fn four_bars_stand_where_the_columns_will_be_until_a_page_is_open(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), Event::Rail(Box::new(code_rail()))]);
    assert!(present(cx, &opened, "code-loading"));
    assert!(!present(cx, &opened, "code-tree"));
    says(cx, &opened, page());
    assert!(!present(cx, &opened, "code-loading") && present(cx, &opened, "code-tree"));
    says(cx, &opened, CodePage { open: false, ..page() });
    assert!(present(cx, &opened, "code-loading"), "an `open` of false is the stage not being there");
}

/// CODE-5: the page the session sent last is kept for the person coming back to it.
#[gpui_kit::test]
fn the_last_page_is_kept_while_another_stage_is_up_and_drawn_again_on_return(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    hear(cx, &opened, vec![Event::Rail(Box::new(crate::checkouts::stage::tests::rail()))]);
    assert!(!present(cx, &opened, "code-tree"), "the Checkouts stage is up");
    hear(cx, &opened, vec![Event::Rail(Box::new(code_rail()))]);
    assert!(present(cx, &opened, "code-tree") && present(cx, &opened, "code-start"), "no new page was sent, and the stage is as it was");
}

/// CODE-6: the strip names the checkout and, when it is another word, its branch, and has the keys button.
#[gpui_kit::test]
fn the_strip_names_the_checkout_and_its_branch_and_has_a_button_for_the_keys(cx: &mut TestAppContext) {
    let opened = with(cx, CodePage { name: "desktop".into(), branch: "desktop-design".into(), ..page() });
    let says = cx.update(|cx| crate::chrome::header::slots::Slots::peek(cx).says(Stage::Code, cx));
    assert_eq!(says.trail, [Part::Mono("desktop".into()), Part::Mono("desktop-design".into())]);
    click_id(cx, &opened, "code-help");
    assert_eq!(sent(&opened), ["Command { action: Shortcuts }"]);
}


/// CODE-11: the files column and the file, and the outline only while it is on and a file is open.
#[gpui_kit::test]
fn the_outline_is_a_third_column_only_while_it_is_on_and_a_file_is_open(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    assert!(present(cx, &opened, "code-files") && present(cx, &opened, "code-file") && !present(cx, &opened, "code-outline"));
    let reading = CodePage { path: Some("DESIGN.md".into()), outline: true, ..page() };
    says(cx, &opened, reading.clone());
    assert!(present(cx, &opened, "code-outline"));
    says(cx, &opened, CodePage { outline: false, ..reading.clone() });
    assert!(!present(cx, &opened, "code-outline"), "put away");
    says(cx, &opened, CodePage { path: None, outline: true, ..reading });
    assert!(!present(cx, &opened, "code-outline"), "an outline of no file is chrome with nothing in it");
}

/// CODE-11: the outline's close button sends the click, and it stays until the page says it is gone.
#[gpui_kit::test]
fn the_outline_close_button_sends_the_click(cx: &mut TestAppContext) {
    let opened = with(cx, CodePage { path: Some("DESIGN.md".into()), outline: true, ..page() });
    click_id(cx, &opened, "code-outline-close");
    assert_eq!(sent(&opened), ["Code { gesture: Click { target: Outline } }"]);
    assert!(present(cx, &opened, "code-outline"));
}

/// CODE-11: the files column is as wide as the rules say undragged, and as the session says once dragged.
#[gpui_kit::test]
fn the_files_column_is_the_usual_width_and_then_the_one_the_session_says(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    assert_eq!(width_of(cx, &opened, "code-files"), 288.0);
    let mut dragged = code_rail();
    dragged.code_tree_width = Some(400);
    hear(cx, &opened, vec![Event::Rail(Box::new(dragged))]);
    assert_eq!(width_of(cx, &opened, "code-files"), 400.0);
}

/// CODE-11 (`style.css` narrow rules): even dragged wider, a window of 1200 or less caps the
/// columns at their narrow widths, which is what is drawn and not only what `widths` computes.
#[gpui_kit::test]
fn a_narrow_window_caps_a_dragged_column_in_what_is_drawn(cx: &mut TestAppContext) {
    let opened = with(cx, CodePage { path: Some("DESIGN.md".into()), outline: true, ..page() });
    cx.simulate_window_resize(opened.handle, gpui_kit::size(gpui_kit::px(1100.0), gpui_kit::px(700.0)));
    let mut dragged = code_rail();
    dragged.code_tree_width = Some(500);
    dragged.code_outline_width = Some(500);
    hear(cx, &opened, vec![Event::Rail(Box::new(dragged))]);
    assert_eq!(width_of(cx, &opened, "code-files"), 240.0);
    assert_eq!(width_of(cx, &opened, "code-outline"), 184.0);
}

/// CODE-11: a drag of the sash between the files and the file reports its width once, on release.
#[gpui_kit::test]
fn dragging_the_files_sash_reports_the_width_on_release(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    let grab = sash(cx, &opened, "code-tree-sash");
    drag(cx, &opened, grab, 60.0);
    assert_eq!(sent(&opened), ["CodeTreeWidth { width: Some(348) }"]);
}

/// CODE-11: a double-click on the sash reports `None`, the usual width.
#[gpui_kit::test]
fn a_double_click_on_the_files_sash_reports_none(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    cx.update_window(opened.handle, |_, window, cx| window.double_click("code-tree-sash".to_owned(), cx)).unwrap();
    cx.run_until_parked();
    assert_eq!(sent(&opened), ["CodeTreeWidth { width: None }"]);
}

/// CODE-11: the outline's sash is on its left edge, so going left widens it.
#[gpui_kit::test]
fn dragging_the_outline_sash_left_widens_it_and_reports_it(cx: &mut TestAppContext) {
    let opened = with(cx, CodePage { path: Some("DESIGN.md".into()), outline: true, ..page() });
    let grab = sash(cx, &opened, "code-outline-sash");
    drag(cx, &opened, grab, -40.0);
    assert_eq!(sent(&opened), ["CodeOutlineWidth { width: Some(272) }"]);
}

/// The window held: a refused action leaves the stage as it was, so a press with the daemon gone sends nothing.
#[gpui_kit::test]
fn a_held_window_sends_nothing(cx: &mut TestAppContext) {
    use argus_desktop_core::connection::page::{Connection, Status};
    let opened = with(cx, page());
    hear(cx, &opened, vec![Event::Status(Status { connection: Connection::Stopped, detail: None, alarm: None, chime: true, host: None })]);
    drain(&opened);
    click_id(cx, &opened, "code-changed-only");
    assert!(sent(&opened).is_empty());
}
