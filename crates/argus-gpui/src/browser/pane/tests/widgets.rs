//! The widgets headless Chrome draws nowhere, drawn by the window: a script's dialog over the page,
//! and a `<select>`'s list under it.

use argus_protocol::{BrowserDialog, BrowserDialogKind, BrowserOption, BrowserSelect};
use gpui_kit::test::TestQueryExt as _;
use gpui_kit::TestAppContext;

use super::*;

fn settle(cx: &mut TestAppContext, opened: &Opened) {
    for _ in 0..2 {
        cx.run_until_parked();
        cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    }
    cx.run_until_parked();
}

fn asking(kind: BrowserDialogKind) -> PageBrowser {
    let mut asking = browser();
    asking.dialog = Some(BrowserDialog { kind, message: "Delete the draft?".into(), default_prompt: "draft".into() });
    asking
}

#[gpui_kit::test]
fn a_confirm_is_asked_over_the_page_and_each_button_answers_it(cx: &mut TestAppContext) {
    let opened = opened(cx, 1, asking(BrowserDialogKind::Confirm));
    assert_eq!(label(cx, &opened, "browser-dialog-105").as_deref(), Some("Delete the draft?"));
    assert!(!has(cx, &opened, "browser-dialog-prompt-105"), "a confirm has no field");
    opened.calls.borrow_mut().clear();
    click(cx, &opened, "browser-dialog-accept-105");
    click(cx, &opened, "browser-dialog-refuse-105");
    assert_eq!(
        said(&opened),
        ["BrowserInput { pane: 105, input: Answer { accept: true, text: None } }", "BrowserInput { pane: 105, input: Answer { accept: false, text: None } }"],
        "the buttons answer, and a press on the card reaches no page under it"
    );
    hear(cx, &opened, vec![page(browser())]);
    assert!(!has(cx, &opened, "browser-dialog-105"), "the card goes when the daemon says the dialog closed");
}

#[gpui_kit::test]
fn a_prompt_starts_at_the_pages_text_and_ok_sends_what_is_typed(cx: &mut TestAppContext) {
    let opened = opened(cx, 1, asking(BrowserDialogKind::Prompt));
    settle(cx, &opened);
    click(cx, &opened, "browser-dialog-prompt-105");
    type_text(cx, &opened, "s");
    opened.calls.borrow_mut().clear();
    click(cx, &opened, "browser-dialog-accept-105");
    let calls = said(&opened);
    assert!(calls.len() == 1 && calls[0].contains("Answer { accept: true, text: Some(\"draft") && calls[0].contains('s'), "{calls:?}");
    let alert = asking_alert(cx);
    assert!(!has(cx, &alert, "browser-dialog-refuse-105"), "an alert has only OK");
}

fn asking_alert(cx: &mut TestAppContext) -> Opened {
    opened(cx, 1, asking(BrowserDialogKind::Alert))
}

fn listing() -> PageBrowser {
    let mut listing = browser();
    let option = |index: u32, label: &str, disabled: bool| BrowserOption { index, label: label.into(), group: None, disabled };
    listing.select = Some(BrowserSelect {
        options: vec![option(0, "Small", false), option(1, "Medium", false), option(2, "Large", true)],
        selected: Some(0),
        x: 10.0,
        y: 10.0,
        width: 120.0,
        height: 20.0,
    });
    listing
}

fn pictured_listing(cx: &mut TestAppContext) -> Opened {
    let opened = opened(cx, 1, browser());
    hear(cx, &opened, vec![Event::BrowserShot(BrowserShot { pane: PANE, jpeg: png(), width: 1280, height: 800 })]);
    settle(cx, &opened);
    hear(cx, &opened, vec![page(listing())]);
    settle(cx, &opened);
    opened.calls.borrow_mut().clear();
    opened
}

fn pick(cx: &mut TestAppContext, opened: &Opened, label: &str) {
    let at = cx.update_window(opened.handle, |_, window, _| window.find_by_label(label).bounds().center()).unwrap();
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(at, MouseButton::Left, cx);
        window.pointer_up(at, MouseButton::Left, cx);
    })
    .unwrap();
    settle(cx, opened);
}

#[gpui_kit::test]
fn an_open_select_is_listed_and_a_pick_sends_its_index_and_nothing_else(cx: &mut TestAppContext) {
    let opened = pictured_listing(cx);
    pick(cx, &opened, "Medium");
    let calls = said(&opened);
    assert_eq!(calls, ["BrowserInput { pane: 105, input: Choose { index: Some(1) } }"], "a pick is not also a dismissal");
    hear(cx, &opened, vec![page(listing())]);
    settle(cx, &opened);
    assert!(cx.update_window(opened.handle, |_, window, _| window.try_find_by_label("Medium").is_none()).unwrap(), "the same select is not opened again");
}

#[gpui_kit::test]
fn escape_puts_the_list_away_and_says_none_was_picked(cx: &mut TestAppContext) {
    let opened = pictured_listing(cx);
    press(cx, &opened, "escape");
    settle(cx, &opened);
    assert_eq!(said(&opened), ["BrowserInput { pane: 105, input: Choose { index: None } }"]);
}

#[gpui_kit::test]
fn an_agents_size_is_said_in_the_toolbar_with_a_way_back_to_the_views(cx: &mut TestAppContext) {
    let opened = opened(cx, 1, browser());
    assert!(!has(cx, &opened, "browser-fit-105"), "nothing to fit while the view's size holds");
    let mut sized = browser();
    sized.sized = Some(argus_protocol::BrowserSize { width: 390, height: 844 });
    hear(cx, &opened, vec![page(sized)]);
    assert!(has(cx, &opened, "browser-sized-105"));
    opened.calls.borrow_mut().clear();
    click(cx, &opened, "browser-fit-105");
    assert_eq!(said(&opened), ["BrowserInput { pane: 105, input: FitView }"]);
}
