//! A block that changed since the person saw it is washed once, and a view changed out of sight leaves
//! a dot on its tab (AGENTVIEWS-13, BLOCKS-2).

use std::rc::Rc;

use gpui_kit::{px, size, Bounds, TestAppContext, WindowBounds, WindowOptions};
use serde_json::{json, Value};

use super::*;
use crate::theme::Tokens;
use crate::views::blocks::changed::unseen;

fn notes() -> Value {
    json!([
        {"kind": "callout", "tone": "info", "text": "first"},
        {"kind": "callout", "tone": "info", "text": "second"},
        {"kind": "section", "heading": "Nested", "blocks": [{"kind": "callout", "tone": "info", "text": "inside"}]}
    ])
}

fn marked(paths: &[&str]) -> ViewCtx {
    ViewCtx { can_press: true, changed: Rc::new(paths.iter().map(|path| (*path).to_owned()).collect()), ..ViewCtx::agent(7, "v") }
}

// BLOCKS-2: only the blocks named are washed; a container is washed when it is the block that changed.
#[gpui_kit::test]
fn only_the_blocks_that_changed_are_washed(cx: &mut TestAppContext) {
    let rig = open_as(cx, 700.0, notes(), marked(&["1", "2.0"]));
    assert!(!exists(cx, &rig, "7/v.0-changed"));
    assert!(exists(cx, &rig, "7/v.1-changed"));
    assert!(!exists(cx, &rig, "7/v.2-changed"), "the section did not change, what is in it did");
    assert!(exists(cx, &rig, "7/v.2.0-changed"));
    // What was washed is still what it was: the mark does not stand in for the block.
    assert_eq!(label(cx, &rig, "7/v.1-callout").as_deref(), Some("second"));
}

#[gpui_kit::test]
fn a_view_with_nothing_changed_draws_no_wash_at_all(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, notes());
    for id in ["7/v.0-changed", "7/v.1-changed", "7/v.2.0-changed"] {
        assert!(!exists(cx, &rig, id), "{id}");
    }
}

// A wash runs once per change: the element drawing the blocks counts the changes.
#[gpui_kit::test]
fn the_same_view_said_again_is_not_a_new_change_and_a_changed_one_is(cx: &mut TestAppContext) {
    let rig = open_as(cx, 700.0, notes(), marked(&["1"]));
    let count = |cx: &mut TestAppContext| cx.update(|cx| rig.view.read(cx).flash());
    let first = count(cx);
    show(cx, &rig, notes(), marked(&["1"]));
    assert_eq!(count(cx), first, "the session says the same thing whenever anything else changes");
    show(cx, &rig, notes(), marked(&["0"]));
    assert_eq!(count(cx), first + 1, "a different block changed");
    let mut other = notes();
    other[1]["text"] = json!("second, again");
    show(cx, &rig, other, marked(&["0"]));
    assert_eq!(count(cx), first + 2, "the blocks themselves changed");
}

struct Dot;

impl Render for Dot {
    fn render(&mut self, _: &mut gpui_kit::Window, cx: &mut gpui_kit::Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        gpui_kit::div().child(unseen(&tokens))
    }
}

// AGENTVIEWS-13: the dot a tab wears while its view changed out of sight.
#[gpui_kit::test]
fn the_unseen_dot_says_changed(cx: &mut TestAppContext) {
    cx.update(crate::window::init);
    let (handle, _) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions { window_bounds: Some(WindowBounds::Windowed(Bounds::new(Default::default(), size(px(200.0), px(100.0))))), ..Default::default() },
            cx,
            |_, cx| cx.new(|_| Dot),
        )
        .expect("opens")
    });
    cx.update_window(handle, |_, window, cx| window.render_frame(cx)).unwrap();
    let said = cx.update_window(handle, |_, window, _| window.try_find("view-unseen".to_owned()).map(|found| (found.label().map(str::to_owned), found.bounds().size))).unwrap();
    let (words, at) = said.expect("the dot is drawn");
    assert_eq!(words.as_deref(), Some("changed"));
    assert_eq!((f32::from(at.width), f32::from(at.height)), (6.0, 6.0));
}
