//! An extension's view in a window: the docker scene's page, drawn from the events a session sends,
//! its clicks and keys going out as the calls the page makes.

use argus_desktop_core::connection::page::Event;
use argus_desktop_core::layout::docks::Panel;
use gpui_kit::{AppContext as _, TestAppContext};

use crate::extensions::bodies::Bodies;
use crate::keys::root::tests::{asked, blur, click_label, drain, has_label, hear, open, press, type_text, Opened};

/// Everything a fresh window would hear in the core's scene `name`.
pub(crate) fn scene(name: &str) -> Vec<Event> {
    let found = argus_desktop_core::screens::scenes().into_iter().find(|scene| scene.name == name).expect("the scene exists");
    let mut session = (found.build)();
    let mut events = session.everything(argus_desktop_core::screens::NOW);
    events.extend(session.take_frames());
    events.into_iter().filter(|event| !matches!(event, Event::Frame(_))).collect()
}

fn panel() -> Panel {
    Panel::extension("docker", "containers")
}

/// The window with the containers as the stage's page, `api-worker` selected.
fn paged(cx: &mut TestAppContext) -> Opened {
    let opened = open(cx);
    hear(cx, &opened, scene("extension-stage"));
    opened
}

/// How wide the view measured the box its table is in, in the page's pixels.
fn measured(cx: &mut TestAppContext) -> f32 {
    cx.update(|cx| {
        let view = Bodies::of(cx).made(&panel()).expect("the page made its view");
        view.read(cx).widths.get(&format!("ext-{}", panel()))
    })
}

#[gpui_kit::test]
fn the_page_draws_the_view_with_its_actions_and_the_selected_row(cx: &mut TestAppContext) {
    let opened = paged(cx);
    assert!(has_label(cx, &opened, "api-worker"), "a row for each container, named by its key");
    assert!(has_label(cx, &opened, "Prune stopped"), "the view-wide action in the page's row");
    assert!(has_label(cx, &opened, "Dock"));
    let width = measured(cx);
    assert!(width > 600.0, "the table measures the box it is in, not its content: {width}");
}

#[gpui_kit::test]
fn the_selected_row_has_text_buttons_while_the_view_is_wide(cx: &mut TestAppContext) {
    let opened = paged(cx);
    for action in ["Logs", "Restart", "Stop", "Shell"] {
        assert!(has_label(cx, &opened, &format!("{action} api-worker")), "{action}");
    }
    assert!(!has_label(cx, &opened, "Row actions"), "never both the buttons and the menu");
}

#[gpui_kit::test]
fn a_click_on_a_row_selects_it_and_a_click_on_its_action_selects_then_acts(cx: &mut TestAppContext) {
    let opened = paged(cx);
    click_label(cx, &opened, "postgres");
    let said = drain(&opened);
    assert!(said.iter().any(|call| call.contains("Select") && call.contains("postgres")), "{said:?}");
    // The selected row's own action: no select first.
    click_label(cx, &opened, "Restart api-worker");
    let said = drain(&opened);
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].contains("Act") && said[0].contains("restart"), "{said:?}");
}

#[gpui_kit::test]
fn the_pages_keys_step_the_selection_and_take_an_actions_key(cx: &mut TestAppContext) {
    let opened = paged(cx);
    blur(cx, &opened);
    drain(&opened);
    press(cx, &opened, "j");
    press(cx, &opened, "k");
    press(cx, &opened, "enter");
    press(cx, &opened, "r");
    let said = drain(&opened);
    assert_eq!(said.len(), 4, "{said:?}");
    assert!(said[0].contains("Key") && said[0].contains("Next"), "{said:?}");
    assert!(said[1].contains("Previous"), "{said:?}");
    assert!(said[2].contains("Choose"), "{said:?}");
    assert!(said[3].contains("Act") && said[3].contains("restart"), "{said:?}");
}

#[gpui_kit::test]
fn the_less_than_key_docks_the_page_again_and_a_command_scoped_to_a_checkout_is_not_heard_in_it(cx: &mut TestAppContext) {
    let opened = paged(cx);
    blur(cx, &opened);
    drain(&opened);
    press(cx, &opened, "alt-u");
    assert!(asked(&opened).is_empty(), "the checkout's command is the rail's: {:?}", asked(&opened));
    press(cx, &opened, "<");
    let said = drain(&opened);
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].contains("ExtensionMode") && said[0].contains("page: false"), "{said:?}");
}

#[gpui_kit::test]
fn a_key_with_no_use_is_held_by_the_page_and_typed_nowhere(cx: &mut TestAppContext) {
    let opened = paged(cx);
    blur(cx, &opened);
    drain(&opened);
    press(cx, &opened, "x");
    assert!(asked(&opened).is_empty(), "{:?}", asked(&opened));
}

/// The docker scene with the containers a console: what the session says of a console view on screen,
/// the screen it gave it and who has the keys.
fn console_scene(console: argus_desktop_core::extensions::views::ExtTableConsole) -> Vec<Event> {
    scene("extension-stage")
        .into_iter()
        .map(|event| match event {
            Event::Extensions(mut page) => {
                for table in &mut page.tables {
                    table.columns.clear();
                    table.rows.clear();
                    table.selected = None;
                    table.console = Some(console.clone());
                }
                Event::Extensions(page)
            }
            other => other,
        })
        .collect()
}

fn running(keyed: bool) -> argus_desktop_core::extensions::views::ExtTableConsole {
    use argus_desktop_core::connection::page::Side;
    argus_desktop_core::extensions::views::ExtTableConsole { side: Some(Side::View(0)), pane: Some(7), ended: None, keyed }
}

fn has_screen(cx: &mut TestAppContext) -> bool {
    cx.update(|cx| Bodies::of(cx).made(&panel()).is_some_and(|view| view.read(cx).console.terminal().is_some()))
}

/// EXTVIEW-25: the strip is hidden while it runs, says Starting until the daemon names the pane, and
/// says how it ended with the view's own actions.
#[gpui_kit::test]
fn a_console_view_says_it_is_starting_runs_with_no_strip_and_says_how_it_ended(cx: &mut TestAppContext) {
    use argus_desktop_core::extensions::views::{ExtConsoleEnd, ExtTableConsole};
    let opened = open(cx);
    hear(cx, &opened, console_scene(ExtTableConsole { pane: None, ..running(false) }));
    assert!(has_label(cx, &opened, "Starting\u{2026}"));
    hear(cx, &opened, console_scene(running(false)));
    assert!(!has_label(cx, &opened, "Starting\u{2026}") && has_screen(cx), "a screen of its own once it runs");
    hear(cx, &opened, console_scene(ExtTableConsole { ended: Some(ExtConsoleEnd { code: Some(137) }), ..running(false) }));
    assert!(has_label(cx, &opened, "Ended (exit 137)"));
    assert!(has_label(cx, &opened, "Prune stopped"), "the view-wide actions are in the strip");
    hear(cx, &opened, console_scene(ExtTableConsole { ended: Some(ExtConsoleEnd { code: None }), ..running(false) }));
    assert!(has_label(cx, &opened, "Ended by a signal"));
}

/// EXTVIEW-25, -34: the console the session says has the keys takes GPUI's focus, and one that leaves
/// the screen lets its screen go.
#[gpui_kit::test]
fn the_console_view_with_the_keys_takes_the_focus_and_lets_its_screen_go_off_screen(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, console_scene(running(false)));
    blur(cx, &opened);
    hear(cx, &opened, console_scene(running(true)));
    let focused = cx.update_window(opened.handle, |_, window, cx| {
        let view = Bodies::of(cx).made(&panel()).expect("made");
        let terminal = view.read(cx).console.terminal().cloned().expect("a screen");
        let focus = terminal.read(cx).focus_handle().clone();
        focus.is_focused(window)
    });
    assert!(focused.unwrap(), "the keys are the console's");
    hear(cx, &opened, scene("extension-stage"));
    assert!(!has_screen(cx), "the table has none");
}

/// SHELL-PALETTE, EXTNAV-7: an extension's view and its commands are in the palette, from Rust's rows.
#[gpui_kit::test]
fn the_palette_lists_go_to_for_each_view_and_each_extension_command(cx: &mut TestAppContext) {
    use argus_desktop_core::connection::page::Palette;
    use argus_desktop_core::keys::keymap::{commands, groups, LAYERS};
    let opened = open(cx);
    let mut events = scene("extension-stage");
    events.push(Event::Keymap { groups: groups(), commands: commands(), layers: LAYERS });
    hear(cx, &opened, events);
    blur(cx, &opened);
    press(cx, &opened, "ctrl-k");
    hear(cx, &opened, vec![Event::Palette(Box::new(Palette { open: true, features_loaded: true, ..Palette::default() }))]);
    type_text(cx, &opened, "Containers");
    assert!(has_label(cx, &opened, "Go to Containers"));
    for _ in 0.."Containers".len() {
        press(cx, &opened, "backspace");
    }
    type_text(cx, &opened, "Compose up");
    assert!(has_label(cx, &opened, "Compose up this checkout"));
}

/// EXTVIEW-29: the sheet has a heading for each extension, and it is Rust's.
#[gpui_kit::test]
fn the_sheet_has_a_heading_for_each_extension(cx: &mut TestAppContext) {
    use argus_desktop_core::keys::keymap::{commands, groups, LAYERS};
    let opened = open(cx);
    let mut events = scene("extension-stage");
    events.push(Event::Keymap { groups: groups(), commands: commands(), layers: LAYERS });
    hear(cx, &opened, events);
    blur(cx, &opened);
    crate::keys::root::tests::question_mark(cx, &opened);
    assert!(has_label(cx, &opened, "Extensions \u{b7} docker"));
}

/// EXTVIEW-7, -31: narrow, the selected row's actions are one "..." button and never also the text.
#[gpui_kit::test]
fn a_narrow_view_holds_a_rows_actions_behind_one_button(cx: &mut TestAppContext) {
    let opened = crate::keys::root::tests::open_sized(cx, 600.0);
    hear(cx, &opened, scene("extension-stage"));
    // The width is measured as the table is laid out, and the view is drawn again at it.
    hear(cx, &opened, Vec::new());
    let width = measured(cx);
    assert!(has_label(cx, &opened, "Row actions"), "the selected row's menu button, at {width}");
    assert!(!has_label(cx, &opened, "Restart api-worker"), "none of the text buttons");
    assert!(width > 0.0 && width <= 560.0, "{width}");
}

/// EXTVIEW-35: the same view is drawn in its dock, from the docks scene, by the one element.
#[gpui_kit::test]
fn the_docked_view_is_drawn_by_the_element_the_page_would_have(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, scene("docks"));
    assert!(!has_label(cx, &opened, "Dock") && !has_label(cx, &opened, "Prune stopped"), "a docked view has no page row of its own");
    assert!(has_label(cx, &opened, "api-web"));
}

/// The docker scene with `change` made to each view on screen.
fn changed(name: &str, change: impl Fn(&mut argus_desktop_core::extensions::views::ExtTable)) -> Vec<Event> {
    scene(name)
        .into_iter()
        .map(|event| match event {
            Event::Extensions(mut page) => {
                page.tables.iter_mut().for_each(&change);
                Event::Extensions(page)
            }
            other => other,
        })
        .collect()
}

/// EXTVIEW-8: a click selects the row and only a double click takes the view's default action, on
/// the row clicked.
#[gpui_kit::test]
fn a_single_click_only_selects_and_a_double_click_takes_the_default_on_that_row(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;
    let opened = open(cx);
    hear(cx, &opened, changed("extension-stage", |table| table.default = Some("logs".into())));
    drain(&opened);
    click_label(cx, &opened, "postgres");
    let said = drain(&opened);
    assert_eq!(said.len(), 1, "only the select: {said:?}");
    assert!(said[0].contains("Select") && said[0].contains("postgres"), "{said:?}");
    let id = format!("ext-row-{}-redis", panel());
    cx.update_window(opened.handle, |_, window, cx| window.double_click(id, cx)).unwrap();
    cx.run_until_parked();
    let said = drain(&opened);
    assert_eq!(said.len(), 3, "a select for each click, then the default once: {said:?}");
    let defaults: Vec<&String> = said.iter().filter(|call| call.contains("Default")).collect();
    assert_eq!(defaults.len(), 1, "{said:?}");
    assert!(defaults[0].contains("redis"), "{said:?}");
}

/// EXTVIEW-15, -16: a docked view that becomes the page takes the keys into its body.
#[gpui_kit::test]
fn a_view_moved_from_its_dock_to_the_page_takes_the_keys_into_its_body(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, scene("docks"));
    blur(cx, &opened);
    let docked = cx.update(|cx| opened.heard.read(cx).extension_tables.first().map(|table| table.panel.clone())).expect("a docked view");
    let keys = |cx: &mut TestAppContext| {
        cx.update_window(opened.handle, |_, window, cx| Bodies::of(cx).made(&docked).map(|view| view.read(cx).has_keys(window, cx))).unwrap()
    };
    assert_eq!(keys(cx), Some(false), "docked, the body does not have them");
    hear(cx, &opened, changed("docks", |table| table.page = true));
    assert_eq!(keys(cx), Some(true), "the page's body has the keys");
}

/// EXTVIEW-25, -36: a press on the terminal of a console view the session says is keyed takes the
/// keys there and asks for nothing; on one that is not, it asks the session for them and waits.
#[gpui_kit::test]
fn a_press_on_a_console_view_asks_for_the_keys_only_when_it_is_not_keyed(cx: &mut TestAppContext) {
    use gpui_kit::test::TestEventExt as _;
    let press_on_screen = |cx: &mut TestAppContext, opened: &Opened| {
        let at = gpui_kit::point(gpui_kit::px(700.0), gpui_kit::px(350.0));
        cx.update_window(opened.handle, |_, window, cx| {
            window.pointer_move(at, None, cx);
            window.pointer_down(at, gpui_kit::MouseButton::Left, cx);
            window.pointer_up(at, gpui_kit::MouseButton::Left, cx);
        })
        .unwrap();
        cx.run_until_parked();
    };
    let opened = open(cx);
    hear(cx, &opened, console_scene(running(true)));
    drain(&opened);
    press_on_screen(cx, &opened);
    let said = drain(&opened);
    assert!(said.iter().all(|call| !call.starts_with("FocusView")), "it has the keys already: {said:?}");
    hear(cx, &opened, console_scene(running(false)));
    blur(cx, &opened);
    drain(&opened);
    press_on_screen(cx, &opened);
    let said = drain(&opened);
    assert!(said.iter().any(|call| call.starts_with("FocusView")), "it asks for the keys: {said:?}");
}

fn point_at(cx: &mut TestAppContext, opened: &Opened, at: gpui_kit::Point<gpui_kit::Pixels>) {
    use gpui_kit::test::{TestEventExt as _, TestWindowExt as _};
    cx.update_window(opened.handle, |_, window, cx| window.pointer_move(at, None, cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.run_until_parked();
}

fn centre_of(cx: &mut TestAppContext, opened: &Opened, label: &str) -> gpui_kit::Point<gpui_kit::Pixels> {
    use gpui_kit::test::TestQueryExt as _;
    cx.update_window(opened.handle, |_, window, _| window.find_by_label(label).bounds().center()).unwrap()
}

#[gpui_kit::test]
fn a_rows_bar_stays_after_the_pointer_leaves_only_while_the_keys_are_in_it(cx: &mut TestAppContext) {
    // EXTVIEW-33: `table.ts` keeps the bar on `pointerleave` while `document.activeElement` is in it.
    let opened = paged(cx);
    let home = centre_of(cx, &opened, "api-worker");
    for keys_inside in [false, true] {
        let on = centre_of(cx, &opened, "postgres");
        point_at(cx, &opened, on);
        assert!(has_label(cx, &opened, "Restart postgres"), "the pointer on the row shows its bar");
        if keys_inside {
            let handle = cx.update(|cx| Bodies::of(cx).made(&panel()).unwrap().read(cx).bars.borrow().get("postgres").cloned().expect("its bar has a place for the keys"));
            cx.update_window(opened.handle, |_, window, cx| window.focus(&handle, cx)).unwrap();
        }
        // Another row, so the pointer has left postgres and the selected row is not it.
        point_at(cx, &opened, home);
        assert_eq!(has_label(cx, &opened, "Restart postgres"), keys_inside, "keys inside: {keys_inside}");
        blur(cx, &opened);
        point_at(cx, &opened, home);
    }
}
