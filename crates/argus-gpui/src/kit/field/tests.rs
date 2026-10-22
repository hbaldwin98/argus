use super::*;
use gpui_kit::component::input::Input;
use gpui_kit::{
    div, prelude::*, px, size, AnyWindowHandle, Bounds, Context, Render, TestAppContext, WindowBounds, WindowOptions,
};

struct Host {
    field: Field,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(Input::new(self.field.input()))
    }
}

struct Opened {
    handle: AnyWindowHandle,
    host: Entity<Host>,
}

fn open(cx: &mut TestAppContext) -> Opened {
    cx.update(crate::window::init);
    let mut built = None;
    let (handle, _) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(Default::default(), size(px(400.0), px(200.0))))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                let host = cx.new(|cx| Host { field: Field::new("Filter", window, cx) });
                built = Some(host.clone());
                host
            },
        )
        .expect("open test window")
    });
    let opened = Opened { handle, host: built.unwrap() };
    cx.run_until_parked();
    opened
}

/// Rust says the field reads `said`; whether it was written.
fn show(cx: &mut TestAppContext, opened: &Opened, said: &str) -> bool {
    let field = cx.update(|cx| opened.host.read(cx).field.clone());
    cx.update_window(opened.handle, |_, window, cx| field.show(said, window, cx)).unwrap()
}

fn text(cx: &mut TestAppContext, opened: &Opened) -> String {
    cx.update(|cx| opened.host.read(cx).field.text(cx).to_string())
}

fn type_in(cx: &mut TestAppContext, opened: &Opened, words: &str) {
    let field = cx.update(|cx| opened.host.read(cx).field.clone());
    cx.update_window(opened.handle, |_, window, cx| {
        field.focus(window, cx);
        field.input().clone().update(cx, |input, cx| input.insert(words, window, cx));
    })
    .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn a_field_nobody_is_typing_in_shows_what_rust_says(cx: &mut TestAppContext) {
    let opened = open(cx);
    assert!(show(cx, &opened, "main"));
    assert_eq!(text(cx, &opened), "main");
    assert!(show(cx, &opened, "release"));
    assert_eq!(text(cx, &opened), "release");
}

#[gpui_kit::test]
fn what_it_already_says_is_not_written_again(cx: &mut TestAppContext) {
    let opened = open(cx);
    assert!(show(cx, &opened, "main"));
    assert!(!show(cx, &opened, "main"), "a rewrite of the same text moves the caret for nothing");
}

#[gpui_kit::test]
fn a_field_with_the_keys_is_never_overwritten_by_an_echo(cx: &mut TestAppContext) {
    // SHARED-4: Rust's echo of an earlier letter lands after the next is typed.
    let opened = open(cx);
    show(cx, &opened, "fi");
    type_in(cx, &opened, "x");
    let typed = text(cx, &opened);
    assert!(typed.contains('x'), "the test typed nothing: {typed:?}");
    assert!(!show(cx, &opened, "fi"), "an echo was written over what is being typed");
    assert_eq!(text(cx, &opened), typed);
}
