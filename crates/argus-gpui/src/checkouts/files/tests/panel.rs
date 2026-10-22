//! The panel's frame: its header, crumb and modes, body states and foot (FILES-2, 7 to 9, 20).

use argus_desktop_core::checkouts::files_page::{FilesFooter, Mode};
use gpui_kit::TestAppContext;

use super::*;
use crate::checkouts::stage::tests::{click_id, label, present, selected};

/// FILES-2: the kicker and the checkout it reads, whose tooltip is the root.
#[gpui_kit::test]
fn the_header_names_the_panel_and_the_checkout_it_reads(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    assert_eq!(label(cx, &opened, "files-kicker").as_deref(), Some("Files"));
    assert_eq!(label(cx, &opened, "files-checkout").as_deref(), Some("desktop"));
    assert_eq!(label(cx, &opened, "files-panel").as_deref(), Some("Files of desktop"));
}

/// FILES-2: each button sends its gesture, the reveal with no path, and a held press changes nothing here.
#[gpui_kit::test]
fn each_header_button_sends_its_gesture(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    for (id, gesture) in [
        ("files-reveal", "Files { gesture: Reveal { path: None } }"),
        ("files-shell", "Files { gesture: Shell }"),
        ("files-reread", "Files { gesture: Reread }"),
        ("files-close", "Files { gesture: Close }"),
    ] {
        click_id(cx, &opened, id);
        assert_eq!(sent(&opened), [gesture], "{id}");
    }
    assert!(present(cx, &opened, "files-panel"), "the panel stays until the session says it is gone");
}

/// FILES-2: the file manager is offered only for a daemon on this machine.
#[gpui_kit::test]
fn the_file_manager_is_offered_only_for_a_local_daemon(cx: &mut TestAppContext) {
    let opened = with(cx, FilesPage { can_reveal: false, ..page() });
    assert!(!present(cx, &opened, "files-reveal"));
    assert!(present(cx, &opened, "files-shell") && present(cx, &opened, "files-reread") && present(cx, &opened, "files-close"));
}

/// FILES-7: the crumb is core's, the last part emphasised, and the mode says which is on.
#[gpui_kit::test]
fn the_crumb_is_the_folder_and_the_mode_says_which_files_are_listed(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    for (at, part) in ["desktop", "crates", "argus-desktop"].into_iter().enumerate() {
        assert_eq!(label(cx, &opened, ("files-crumb-part", at)).as_deref(), Some(part));
    }
    assert_eq!(selected(cx, &opened, "files-mode-all"), Some(true));
    assert_eq!(selected(cx, &opened, "files-mode-changed"), Some(false));
    says(cx, &opened, FilesPage { mode: Mode::Changed, ..page() });
    assert_eq!((selected(cx, &opened, "files-mode-all"), selected(cx, &opened, "files-mode-changed")), (Some(false), Some(true)));
}

/// FILES-7: a click on a mode sends it and changes nothing until the page says so.
#[gpui_kit::test]
fn a_click_on_a_mode_sends_it_and_the_mode_waits_for_the_page(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    click_id(cx, &opened, "files-mode-changed");
    assert_eq!(sent(&opened), ["Files { gesture: Mode { mode: Changed } }"]);
    assert_eq!(selected(cx, &opened, "files-mode-all"), Some(true), "the client never predicts");
}

/// FILES-8: a folder that would not read says why where its rows would be.
#[gpui_kit::test]
fn a_folder_that_would_not_read_says_why(cx: &mut TestAppContext) {
    let opened = with(cx, FilesPage { error: Some("permission denied".into()), ..page() });
    assert_eq!(label(cx, &opened, "files-error").as_deref(), Some("Could not read the folder: permission denied"));
    assert!(!present(cx, &opened, ("files-row", 1usize)), "no rows under an error");
}

/// FILES-8: loading bars until the first listing, else the line that says why the list is empty.
#[gpui_kit::test]
fn the_body_is_loading_bars_then_rows_or_the_reason_there_are_none(cx: &mut TestAppContext) {
    let opened = with(cx, FilesPage { loading: true, rows: Vec::new(), ..page() });
    assert!(present(cx, &opened, "files-loading"));
    let empty = FilesPage { rows: Vec::new(), ..page() };
    says(cx, &opened, empty.clone());
    assert_eq!(label(cx, &opened, "files-nothing").as_deref(), Some("This folder is empty."));
    says(cx, &opened, FilesPage { mode: Mode::Changed, ..empty.clone() });
    assert_eq!(label(cx, &opened, "files-nothing").as_deref(), Some("Nothing changed in the folders read."));
    says(cx, &opened, FilesPage { filter: "zz".into(), mode: Mode::Changed, ..empty });
    assert_eq!(label(cx, &opened, "files-nothing").as_deref(), Some("Nothing named \u{201c}zz\u{201d} in the folders read."), "a filter is named before the mode");
}

/// FILES-9: a listing cut short says so above the tree.
#[gpui_kit::test]
fn a_truncated_listing_warns_above_the_tree(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    assert!(!present(cx, &opened, "files-truncated"));
    says(cx, &opened, FilesPage { truncated: true, ..page() });
    assert_eq!(label(cx, &opened, "files-truncated").as_deref(), Some("A folder is too large to list whole; the first part is shown."));
}

/// FILES-20: the foot names the selected entry with core's size and age, and a file has the button.
#[gpui_kit::test]
fn the_foot_says_the_selected_entry_and_offers_a_file_in_code(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    assert_eq!(label(cx, &opened, "files-fname").as_deref(), Some("style.css"));
    assert_eq!(label(cx, &opened, "files-detail").as_deref(), Some("212 KB \u{b7} changed 4m ago"));
    click_id(cx, &opened, "files-foot-open");
    assert_eq!(sent(&opened), ["Files { gesture: Open { path: \"crates/argus-desktop/style.css\", how: Code } }"]);
}

/// FILES-20: a folder has no Open in Code, and nothing selected asks for a file.
#[gpui_kit::test]
fn a_folder_has_no_open_button_and_no_selection_asks_for_a_file(cx: &mut TestAppContext) {
    let folder = FilesPage {
        selected: Some("crates".into()),
        footer: Some(FilesFooter { name: "crates".into(), dir: true, detail: "changed 2h ago".into() }),
        ..page()
    };
    let opened = with(cx, folder);
    assert_eq!(label(cx, &opened, "files-fname").as_deref(), Some("crates"));
    assert!(!present(cx, &opened, "files-foot-open"));
    says(cx, &opened, FilesPage { selected: None, footer: None, ..page() });
    assert_eq!(label(cx, &opened, "files-hint").as_deref(), Some("Select a file."));
}

/// The panel is the stage's: put away by the session's word, it is not drawn.
#[gpui_kit::test]
fn the_panel_is_drawn_only_while_the_page_has_it(cx: &mut TestAppContext) {
    let opened = with(cx, page());
    assert!(present(cx, &opened, "files-panel"));
    hear(cx, &opened, vec![heard(table("c101"))]);
    assert!(!present(cx, &opened, "files-panel") && !present(cx, &opened, "checkouts-files"));
}
