use argus_desktop_core::connection::page::{Connection, Event};
use argus_desktop_core::extensions::views::{ExtConsoleEnd, ExtTableAction, ExtTableConsole, ExtensionsPage};
use argus_desktop_core::layout::docks::DocksPage;
use argus_desktop_core::panes::console::ConsolePlace;
use gpui_kit::TestAppContext;

use super::*;
use crate::kit::menu::item::{lines, Hint};
use crate::layout::work::tests::{centre, console, docks, down, hear, images, open, present_id, status, taken, up, Opened};

fn action(id: &str, label: &str, key: Option<&str>, view_wide: bool) -> ExtTableAction {
    ExtTableAction { id: id.into(), label: label.into(), key: key.map(str::to_owned), view_wide }
}

fn table(page: bool, selected: Option<&str>) -> ExtTable {
    ExtTable {
        panel: images(),
        extension: "docker".into(),
        view: "images".into(),
        title: "Images".into(),
        page,
        columns: Vec::new(),
        rows: Vec::new(),
        actions: vec![
            action("pull", "Pull", None, true),
            action("remove", "Remove", Some("d"), false),
            action("inspect", "Inspect", None, false),
            action("run", "Run", Some("r"), false),
        ],
        selected: selected.map(str::to_owned),
        default: None,
        console: None,
    }
}

#[test]
fn the_menu_lists_the_view_wide_actions_then_the_keyed_ones_for_the_selected_row() {
    let tools = of(&images(), &[table(false, Some("a"))], &[]).expect("a docked view has tools");
    assert_eq!(lines(&tools.menu), ["Pull", "---", "# Keys, on the selected row", "Remove", "Run"], "an action with no key is not listed on its own");
    let keyed = |at: usize| match tools.menu[at].as_choice().map(|choice| &choice.hint) {
        Some(Hint::Keys(keys)) => keys.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(keyed(3), [["d"]]);
    assert_eq!(
        format!("{:?}", tools.menu[0].as_choice().unwrap().does),
        "Extension { panel: Extension(\"docker/images\"), gesture: Act { action: \"pull\" } }"
    );
}

#[test]
fn the_keys_need_a_row_and_are_refused_while_none_is_selected() {
    let tools = of(&images(), &[table(false, None)], &[]).unwrap();
    assert_eq!(lines(&tools.menu)[3..], ["Remove (no row selected)", "Run (no row selected)"]);
    assert!(tools.menu[0].as_choice().unwrap().reason.is_none(), "a view-wide action needs no row");
}

#[test]
fn a_view_that_is_a_page_or_not_on_screen_has_no_tools_in_a_strip() {
    assert!(of(&images(), &[table(true, None)], &[]).is_none(), "it carries its way back on its own head");
    assert!(of(&images(), &[], &[]).is_none());
    assert!(of(&Panel::Views, &[table(false, None)], &[]).is_none(), "another panel's table is not this one's");
}

#[test]
fn an_ended_console_view_offers_no_view_wide_action_in_the_menu() {
    let mut ended = table(false, Some("a"));
    ended.console = Some(ExtTableConsole { ended: Some(ExtConsoleEnd { code: Some(1) }), ..ExtTableConsole::default() });
    let tools = of(&images(), &[ended], &[]).unwrap();
    assert_eq!(lines(&tools.menu)[0], "# Keys, on the selected row", "its actions are in the strip beside how it ended");
}

#[test]
fn the_page_button_names_its_keys_once_the_keymap_is_heard() {
    let keymap = argus_desktop_core::keys::keymap::groups();
    assert_eq!(of(&images(), &[table(false, None)], &keymap).unwrap().page_words, "Open as page (>)");
    assert_eq!(of(&images(), &[table(false, None)], &[]).unwrap().page_words, "Open as page");
    assert_eq!(format!("{:?}", of(&images(), &[table(false, None)], &[]).unwrap().page()), "ExtensionMode { panel: Extension(\"docker/images\"), page: true }");
}

fn shown(images_shown: bool) -> DocksPage {
    let mut page = docks();
    if images_shown {
        page.right.shown = Some(images());
    }
    page
}

fn with_tables(cx: &mut TestAppContext, images_shown: bool) -> Opened {
    let tables = ExtensionsPage { tables: vec![table(false, Some("a"))], ..ExtensionsPage::default() };
    open(
        cx,
        vec![
            status(Connection::Connected),
            console(ConsolePlace::Docked),
            Event::Docks(Box::new(shown(images_shown))),
            Event::Extensions(Box::new(tables)),
        ],
    )
}

#[gpui_kit::test]
fn the_strip_ends_with_the_shown_extension_views_tools_and_the_page_button_asks_for_the_page(cx: &mut TestAppContext) {
    let opened = with_tables(cx, true);
    let (open_id, menu_id) = ("dock-tools-open-extension:docker/images", "dock-tools-menu-extension:docker/images");
    assert!(present_id(cx, &opened, open_id) && present_id(cx, &opened, menu_id));
    let at = centre(cx, &opened, open_id);
    down(cx, &opened, at);
    up(cx, &opened, at);
    assert_eq!(taken(&opened), ["ExtensionMode { panel: Extension(\"docker/images\"), page: true }"]);
}

#[gpui_kit::test]
fn the_tools_are_the_shown_panels_alone(cx: &mut TestAppContext) {
    let opened = with_tables(cx, false);
    assert!(!present_id(cx, &opened, "dock-tools-open-extension:docker/images"), "the views panel is shown, not the extension's view");
    hear(cx, &opened, vec![Event::Docks(Box::new(shown(true)))]);
    assert!(present_id(cx, &opened, "dock-tools-open-extension:docker/images"));
    hear(cx, &opened, vec![Event::Docks(Box::new(shown(false)))]);
    assert!(!present_id(cx, &opened, "dock-tools-open-extension:docker/images"));
}

#[gpui_kit::test]
fn a_stopped_daemon_leaves_the_page_button_asking_nothing(cx: &mut TestAppContext) {
    let opened = with_tables(cx, true);
    hear(cx, &opened, vec![status(Connection::Stopped)]);
    let at = centre(cx, &opened, "dock-tools-open-extension:docker/images");
    down(cx, &opened, at);
    up(cx, &opened, at);
    assert!(taken(&opened).is_empty());
}

/// The extension's view alone in the right dock: a title row, not a tab strip.
fn alone(cx: &mut TestAppContext) -> Opened {
    let mut page = shown(true);
    page.right.tabs.retain(|tab| tab.panel == images());
    open(
        cx,
        vec![
            status(Connection::Connected),
            console(ConsolePlace::Docked),
            Event::Docks(Box::new(page)),
            Event::Extensions(Box::new(ExtensionsPage { tables: vec![table(false, Some("a"))], ..ExtensionsPage::default() })),
        ],
    )
}

#[gpui_kit::test]
fn a_view_alone_in_its_dock_has_a_hide_that_hides_it_and_a_tab_strip_has_none(cx: &mut TestAppContext) {
    // EXTVIEW-21: Hide, "Hide <title>", sends `docks hide`.
    let opened = alone(cx);
    assert_eq!(crate::layout::work::tests::label(cx, &opened, "dock-hide-right").as_deref(), Some("Hide Images"));
    let at = centre(cx, &opened, "dock-hide-button-right");
    down(cx, &opened, at);
    up(cx, &opened, at);
    assert_eq!(taken(&opened), ["Docks { gesture: Hide { panel: Extension(\"docker/images\") } }"]);
    let tabbed = with_tables(cx, true);
    assert!(!present_id(cx, &tabbed, "dock-hide-right"), "its tab's menu holds Hide");
}

#[gpui_kit::test]
fn a_stopped_daemon_leaves_hide_asking_nothing(cx: &mut TestAppContext) {
    let opened = alone(cx);
    hear(cx, &opened, vec![status(Connection::Stopped)]);
    let at = centre(cx, &opened, "dock-hide-button-right");
    down(cx, &opened, at);
    up(cx, &opened, at);
    assert!(taken(&opened).is_empty());
}
