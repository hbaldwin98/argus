//! What an input method types into the browser's screen (SHELL-INPUT-3): the committed text once,
//! a preedit not at all until it ends, and an ordinary key as it always was.

use gpui_kit::test::TestInput;
use gpui_kit::{px, size, Bounds, ElementInputHandler, TestAppContext};

use super::*;
use crate::browser::pane::BrowserPane;

/// A pane of its own over the opened window's session, so its input handler can be driven the way
/// the platform drives it.
fn composing(cx: &mut TestAppContext, f: impl FnOnce(&mut TestInput, &mut gpui_kit::App)) -> Vec<String> {
    let opened = opened(cx, 1, browser());
    opened.calls.borrow_mut().clear();
    let (heard, sink) = (opened.heard.clone(), opened.calls.clone());
    cx.update_window(opened.handle, |_, window, cx| {
        let caller = Caller::new(move |call| sink.borrow_mut().push(call));
        let pane = cx.new(|cx| BrowserPane::new(heard, caller, PANE, window, cx));
        let bounds = Bounds::new(Default::default(), size(px(300.0), px(200.0)));
        let mut input = TestInput::new(ElementInputHandler::new(bounds, pane), window);
        f(&mut input, cx);
    })
    .unwrap();
    said(&opened)
}

#[gpui_kit::test]
fn committed_text_is_sent_as_text_once(cx: &mut TestAppContext) {
    let calls = composing(cx, |input, cx| {
        input.commit_text("日本", cx);
    });
    assert_eq!(calls, ["BrowserInput { pane: 105, input: Text { text: \"日本\" } }"]);
}

#[gpui_kit::test]
fn a_preedit_is_sent_only_when_it_ends_and_never_again_after_a_commit(cx: &mut TestAppContext) {
    let calls = composing(cx, |input, cx| {
        input.compose_text(None, "に", None, cx);
        input.compose_text(None, "にほ", None, cx);
        input.compose_text(None, "にほん", None, cx);
        input.commit_text("日本", cx);
        input.unmark_text(cx);
    });
    assert_eq!(calls, ["BrowserInput { pane: 105, input: Text { text: \"日本\" } }"], "the commit replaced the preedit: one text, none twice");
}

#[gpui_kit::test]
fn unmarking_a_preedit_commits_it_as_the_page_does_on_compositionend(cx: &mut TestAppContext) {
    let calls = composing(cx, |input, cx| {
        input.compose_text(None, "かな", None, cx);
        input.unmark_text(cx);
        input.unmark_text(cx);
    });
    assert_eq!(calls, ["BrowserInput { pane: 105, input: Text { text: \"かな\" } }"]);
}

#[gpui_kit::test]
fn a_cancelled_preedit_sends_nothing(cx: &mut TestAppContext) {
    let calls = composing(cx, |input, cx| {
        input.compose_text(None, "かな", None, cx);
        input.compose_text(None, "", None, cx);
        input.unmark_text(cx);
    });
    assert!(calls.is_empty(), "{calls:?}");
}

/// The screen hands the platform its handler while it has the keys, and a key typed on a
/// keyboard is the key walk's alone: the platform's text for it is not also typed.
#[gpui_kit::test]
fn a_key_is_one_text_whether_or_not_the_screen_takes_the_platforms_text(cx: &mut TestAppContext) {
    let opened = opened(cx, 1, browser());
    click(cx, &opened, "browser-screen-105");
    opened.calls.borrow_mut().clear();
    type_text(cx, &opened, "ab");
    assert_eq!(said(&opened), ["BrowserInput { pane: 105, input: Text { text: \"a\" } }", "BrowserInput { pane: 105, input: Text { text: \"b\" } }"]);
    cx.update_window(opened.handle, |_, window, cx| window.input("日本", cx)).unwrap();
    cx.run_until_parked();
    // The test window types each character as a keystroke, which the key walk takes and stops: one
    // text apiece, none doubled by the handler registered on the same screen.
    assert_eq!(said(&opened).len(), 2, "one text a character");
}

/// Text of more than one character is not a key the page knows, so the key walk leaves it and the
/// platform hands it to the input handler the screen registered: here, as GPUI's own simulated
/// keystroke carrying it, through the window's handler and not one built by the test.
fn hand_text(cx: &mut TestAppContext, opened: &Opened, text: &str) {
    cx.update_window(opened.handle, |_, window, cx| {
        let stroke = gpui_kit::Keystroke { modifiers: Default::default(), key: text.into(), key_char: Some(text.into()) };
        window.dispatch_keystroke(stroke, cx);
    })
    .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn text_the_platform_hands_the_focused_screen_reaches_the_page_once(cx: &mut TestAppContext) {
    let opened = opened(cx, 1, browser());
    click(cx, &opened, "browser-screen-105");
    opened.calls.borrow_mut().clear();
    hand_text(cx, &opened, "日本");
    assert_eq!(said(&opened), ["BrowserInput { pane: 105, input: Text { text: \"日本\" } }"]);
}

#[gpui_kit::test]
fn text_the_platform_hands_an_unfocused_screen_is_not_the_pages(cx: &mut TestAppContext) {
    let opened = opened(cx, 1, browser());
    click(cx, &opened, "browser-address-105");
    opened.calls.borrow_mut().clear();
    hand_text(cx, &opened, "日本");
    let said = said(&opened);
    assert!(said.is_empty(), "{said:?}");
}

#[gpui_kit::test]
fn the_candidate_window_opens_at_a_line_at_the_screens_corner(cx: &mut TestAppContext) {
    let opened = opened(cx, 1, browser());
    let screen = Bounds::new(gpui_kit::point(px(40.0), px(60.0)), size(px(300.0), px(200.0)));
    let (heard, sink) = (opened.heard.clone(), opened.calls.clone());
    let got = cx
        .update_window(opened.handle, |_, window, cx| {
            let caller = Caller::new(move |call| sink.borrow_mut().push(call));
            let pane = cx.new(|cx| BrowserPane::new(heard, caller, PANE, window, cx));
            pane.update(cx, |pane, cx| gpui_kit::EntityInputHandler::bounds_for_range(pane, 0..0, screen, window, cx))
        })
        .unwrap()
        .unwrap();
    assert_eq!(got.origin, screen.origin);
    assert_eq!(got.size.width, px(0.0));
    assert!(got.size.height > px(0.0) && got.size.height < screen.size.height, "{:?}", got.size);
}
