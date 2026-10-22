//! The question before an action in a window: drawn once while the page holds it, answered once by
//! its buttons, Escape and the scrim, and gone when the extension takes the action away.

use argus_desktop_core::connection::page::Event;
use argus_desktop_core::extensions::views::ExtConfirm;
use gpui_kit::TestAppContext;

use crate::extensions::view::tests::scene;
use crate::keys::root::tests::{asked, blur, click_label, drain, has_label, hear, open, overlays, press};
use crate::kit::overlay::kind::Overlay;

/// The docker scene with the question `Stop?` waiting on a yes.
fn asking(confirm: Option<ExtConfirm>) -> Vec<Event> {
    scene("extension-stage")
        .into_iter()
        .map(|event| match event {
            Event::Extensions(mut page) => {
                page.confirm = confirm.clone();
                Event::Extensions(page)
            }
            other => other,
        })
        .collect()
}

fn stop() -> Option<ExtConfirm> {
    Some(ExtConfirm { extension: "docker".into(), label: "Stop".into(), question: "Stop this container?".into() })
}

fn up(cx: &mut TestAppContext) -> bool {
    overlays(cx).contains(&Overlay::ExtensionConfirm)
}

/// EXTVIEW-18: `<label>?`, the question, `from <extension>`, and the answers.
#[gpui_kit::test]
fn the_question_is_up_while_the_page_holds_it_with_its_words(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, asking(stop()));
    assert!(up(cx));
    for words in ["Stop?", "Stop this container?"] {
        assert!(has_label(cx, &opened, words), "{words}");
    }
    hear(cx, &opened, asking(None));
    assert!(!up(cx), "taken away when the page says there is none, as when the row or action went");
}

#[gpui_kit::test]
fn the_danger_button_says_yes_once_and_the_page_heard_again_does_not_put_it_back(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, asking(stop()));
    drain(&opened);
    click_label(cx, &opened, "Stop");
    assert_eq!(asked(&opened), ["AnswerExtension { yes: true }"]);
    assert!(!up(cx));
    hear(cx, &opened, asking(stop()));
    assert!(!up(cx), "the session has not yet dropped it, and it is not asked twice");
    hear(cx, &opened, asking(None));
    hear(cx, &opened, asking(stop()));
    assert!(up(cx), "a question that comes again is a new one");
}

/// The question up with the keys on Cancel, as the person who took the action from the view has them,
/// and whatever `how` does sent.
fn said_no_by(cx: &mut TestAppContext, how: &str) {
    let opened = open(cx);
    blur(cx, &opened);
    hear(cx, &opened, asking(stop()));
    // The dialog slides in, and the keys are where it has settled them.
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    hear(cx, &opened, Vec::new());
    drain(&opened);
    match how {
        "button" => click_label(cx, &opened, "Cancel"),
        key => press(cx, &opened, key),
    }
    assert_eq!(asked(&opened), ["AnswerExtension { yes: false }"], "{how}");
    assert!(!up(cx), "{how}");
}

/// EXTVIEW-18: Cancel has the keys, so Escape and Enter both say no, and so does the button.
#[gpui_kit::test]
fn escape_says_no(cx: &mut TestAppContext) {
    said_no_by(cx, "escape");
}

#[gpui_kit::test]
fn enter_on_cancel_says_no(cx: &mut TestAppContext) {
    said_no_by(cx, "enter");
}

#[gpui_kit::test]
fn the_cancel_button_says_no(cx: &mut TestAppContext) {
    said_no_by(cx, "button");
}
