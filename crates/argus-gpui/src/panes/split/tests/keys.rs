//! Which cell has GPUI's focus: the one the session says has the keys.

use gpui_kit::{FocusHandle, TestAppContext};

use super::*;

fn handle(cx: &mut TestAppContext, opened: &Opened, cell: u8) -> FocusHandle {
    let shell = opened.shell.clone();
    cx.update(|cx| shell.read(cx).split().read(cx).handle_of(cell, cx).expect("the cell has a screen"))
}

fn focused(cx: &mut TestAppContext, opened: &Opened) -> Option<FocusHandle> {
    cx.update_window(opened.handle, |_, window, cx| window.focused(cx)).unwrap()
}

fn keyed(n: u8) -> Event {
    let mut row = three();
    row.keyed = Some(n);
    rail(row)
}

#[gpui_kit::test]
fn the_focus_follows_the_keys_the_session_moves_between_cells(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    assert_eq!(focused(cx, &opened), Some(handle(cx, &opened, 0)), "the Workspace's pane has them to begin with");
    hear(cx, &opened, vec![keyed(1)]);
    assert_eq!(focused(cx, &opened), Some(handle(cx, &opened, 1)));
    hear(cx, &opened, vec![keyed(2)]);
    assert_eq!(focused(cx, &opened), Some(handle(cx, &opened, 2)));
    hear(cx, &opened, vec![keyed(0)]);
    assert_eq!(focused(cx, &opened), Some(handle(cx, &opened, 0)));
}

#[gpui_kit::test]
fn a_focus_that_lands_on_the_workspaces_screen_goes_on_to_the_cell_with_the_keys(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    hear(cx, &opened, vec![keyed(1)]);
    // The rail handing the keys back, or a dialog closing, focuses the Workspace's screen.
    let workspace = handle(cx, &opened, 0);
    cx.update_window(opened.handle, |_, window, cx| window.focus(&workspace, cx)).unwrap();
    cx.run_until_parked();
    assert_eq!(focused(cx, &opened), Some(handle(cx, &opened, 1)));
}

#[gpui_kit::test]
fn a_press_on_a_cell_without_the_keys_asks_for_them_and_waits_for_the_answer(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    let body = bounds(cx, &opened, "cell-1");
    let press = by(body.center(), 0.0, 20.0);
    down(cx, &opened, press);
    up(cx, &opened, press);
    assert_eq!(asked(&opened), ["FocusView { pane: 2 }"]);
    assert_eq!(focused(cx, &opened), Some(handle(cx, &opened, 0)), "the focus moves when the session says so, not on the press");
}

#[gpui_kit::test]
fn a_pick_in_the_rail_keeps_the_keys_when_they_move_between_cells(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    cx.update_window(opened.handle, |_, window, cx| window.click("checkout-110", cx)).unwrap();
    cx.run_until_parked();
    let on_rail = focused(cx, &opened);
    assert_ne!(on_rail, Some(handle(cx, &opened, 0)), "the rail has the keys after a pick");
    hear(cx, &opened, vec![keyed(1)]);
    assert_eq!(focused(cx, &opened), on_rail, "SHELL-RAIL-24: a rail pick keeps them");
}

/// An extension's console docked on the right, on `View(0)`, and the grid's rail: with `keyed` the
/// session says the console has the keys, so no cell of the grid does.
fn docked_console(keyed: bool) -> Vec<Event> {
    use argus_desktop_core::extensions::views::{ExtTable, ExtTableConsole, ExtensionsPage};
    use crate::layout::work::tests::{docks, images};
    let mut page = docks();
    page.right.shown = Some(images());
    let console = ExtTableConsole { side: Some(Side::View(0)), pane: Some(9), ended: None, keyed };
    let table = ExtTable {
        panel: images(),
        extension: "docker".into(),
        view: "images".into(),
        title: "Images".into(),
        page: false,
        columns: Vec::new(),
        rows: Vec::new(),
        actions: Vec::new(),
        selected: None,
        default: None,
        console: Some(console),
    };
    let mut row = three();
    row.keyed = (!keyed).then_some(0);
    vec![Event::Docks(Box::new(page)), Event::Extensions(Box::new(ExtensionsPage { tables: vec![table], ..ExtensionsPage::default() })), rail(row)]
}

fn console_screen(cx: &mut TestAppContext) -> FocusHandle {
    use crate::extensions::bodies::Bodies;
    use crate::layout::work::tests::images;
    cx.update(|cx| {
        let view = Bodies::of(cx).made(&images()).expect("the dock made the view");
        let terminal = view.read(cx).console.terminal().cloned().expect("the console has a screen");
        terminal.read(cx).focus_handle().clone()
    })
}

#[gpui_kit::test]
fn a_press_on_the_workspace_while_an_extension_console_has_the_keys_asks_for_them_back(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    hear(cx, &opened, docked_console(true));
    assert_eq!(focused(cx, &opened), Some(console_screen(cx)), "the console the session keyed has the focus");
    opened.calls.borrow_mut().clear();
    let press = by(bounds(cx, &opened, "cell-0").center(), 0.0, 20.0);
    down(cx, &opened, press);
    up(cx, &opened, press);
    assert_eq!(asked(&opened), ["Select { pane: 1 }"], "the Workspace does not have them while the console does");
}

#[gpui_kit::test]
fn the_focus_comes_back_from_an_extension_console_when_the_session_gives_the_workspace_the_keys(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    hear(cx, &opened, docked_console(true));
    assert_eq!(focused(cx, &opened), Some(console_screen(cx)));
    hear(cx, &opened, docked_console(false));
    assert_eq!(focused(cx, &opened), Some(handle(cx, &opened, 0)));
}
