//! Each kind of value in a test window: what is drawn, and what pressing it asks for.

use std::cell::RefCell;
use std::rc::Rc;

use argus_desktop_core::connection::page::{Call, Reference};
use argus_desktop_core::views::nav::NavView;
use argus_desktop_core::views::saved::SavedColumn;
use argus_desktop_core::system::clock::Clock;
use argus_protocol::{Chip, Tone, Value};
use gpui_kit::component::v_flex;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{px, size, AnyWindowHandle, Bounds, TestAppContext, WindowBounds, WindowOptions};

use super::*;
use crate::connection::caller::Caller;
use argus_desktop_core::layout::docks::Panel;
use crate::views::ctx::{Kind, ViewCtx};

struct Gallery {
    values: Vec<Value>,
    caller: Caller,
    view: Option<ViewCtx>,
    saved: Vec<NavView>,
}

impl Render for Gallery {
    fn render(&mut self, _: &mut gpui_kit::Window, cx: &mut gpui_kit::Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        // What an age says is core's, tested there; here only that one is drawn.
        let clock = Clock::local();
        let draw = Draw { tokens: &tokens, clock: &clock, caller: &self.caller, saved: &self.saved, view: self.view.as_ref() };
        v_flex().gap_2().children(self.values.iter().enumerate().map(|(k, v)| value(v, &format!("v{k}"), &draw)))
    }
}

struct Opened {
    handle: AnyWindowHandle,
    calls: Rc<RefCell<Vec<Call>>>,
}

fn open(cx: &mut TestAppContext, values: Vec<Value>, view: Option<ViewCtx>) -> Opened {
    cx.update(crate::window::init);
    let calls: Rc<RefCell<Vec<Call>>> = Rc::default();
    let sink = calls.clone();
    let (handle, _) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions { window_bounds: Some(WindowBounds::Windowed(Bounds::new(Default::default(), size(px(500.0), px(500.0))))), ..Default::default() },
            cx,
            |_, cx| {
                let caller = Caller::new(move |call| sink.borrow_mut().push(call));
                let saved = vec![NavView { id: "release".into(), title: "Release 0.34".into(), scope: "Global".into(), here: true, stale: false, under: None, panel: Panel::Saved("release".into()) }];
                cx.new(|_| Gallery { values, caller, view, saved })
            },
        )
        .expect("open test window")
    });
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| window.render_frame(cx)).unwrap();
    }
    Opened { handle, calls }
}

fn press(cx: &mut TestAppContext, opened: &Opened, id: &str) {
    cx.update_window(opened.handle, |_, window, cx| window.click(id.to_owned(), cx)).unwrap();
    cx.run_until_parked();
}

fn drawn(cx: &mut TestAppContext, opened: &Opened, id: &str) -> bool {
    cx.update_window(opened.handle, |_, window, _| window.try_find(id.to_owned()).is_some()).unwrap()
}

fn chip(label: &str) -> Chip {
    Chip { label: label.into(), tone: Tone::Good }
}

#[gpui_kit::test]
fn a_link_asks_rust_to_open_its_address_and_the_address_is_not_what_is_shown(cx: &mut TestAppContext) {
    let opened = open(cx, vec![Value::Link { label: "the docs".into(), url: "https://example.com/d".into() }], None);
    press(cx, &opened, "v0-link");
    assert!(matches!(&opened.calls.borrow()[..], [Call::OpenUrl { url }] if url == "https://example.com/d"));
}

#[gpui_kit::test]
fn a_ticket_is_a_link_only_when_it_has_an_address(cx: &mut TestAppContext) {
    let ticket = |url: Option<&str>| Value::Ticket { key: "ARG-142".into(), title: Some("Rank recipes".into()), url: url.map(Into::into), status: Some(chip("open")) };
    let opened = open(cx, vec![ticket(Some("https://jira.example/ARG-142")), ticket(None)], None);
    assert!(drawn(cx, &opened, "v0-link") && !drawn(cx, &opened, "v1-link"));
    press(cx, &opened, "v0-link");
    assert!(matches!(&opened.calls.borrow()[..], [Call::OpenUrl { url }] if url == "https://jira.example/ARG-142"));
}

#[gpui_kit::test]
fn a_reference_asks_for_the_pane_that_showed_the_view_and_names_what_it_points_at(cx: &mut TestAppContext) {
    let values = vec![
        Value::Path { path: "src/main.rs".into(), line: Some(12) },
        Value::Commit { sha: "aa10a2010c567a48999479fed257411c0a8b2b67".into(), title: Some("Fix".into()) },
        Value::Feature { number: 41 },
        Value::Pane { pane: 9, label: None },
    ];
    let opened = open(cx, values, Some(ViewCtx::agent(7, "ci")));
    for k in 0..4 {
        press(cx, &opened, &format!("v{k}-ref"));
    }
    let calls = opened.calls.borrow();
    let expected = [
        Reference::Path { path: "src/main.rs".into(), line: Some(12) },
        Reference::Commit { sha: "aa10a2010c567a48999479fed257411c0a8b2b67".into() },
        Reference::Feature { number: 41 },
        Reference::Pane { pane: 9 },
    ];
    assert_eq!(calls.len(), 4);
    for (call, want) in calls.iter().zip(expected) {
        assert!(matches!(call, Call::OpenRef { pane: Some(7), reference } if *reference == want), "{call:?}");
    }
}

#[gpui_kit::test]
fn a_reference_in_a_table_with_no_view_names_no_pane(cx: &mut TestAppContext) {
    let opened = open(cx, vec![Value::Feature { number: 3 }], None);
    press(cx, &opened, "v0-ref");
    assert!(matches!(&opened.calls.borrow()[..], [Call::OpenRef { pane: None, .. }]));
}

#[gpui_kit::test]
fn a_saved_views_references_name_no_pane_either(cx: &mut TestAppContext) {
    let saved = ViewCtx { kind: Kind::Saved, pane: 0, ..ViewCtx::agent(0, "release") };
    let opened = open(cx, vec![Value::Feature { number: 3 }], Some(saved));
    press(cx, &opened, "v0-ref");
    assert!(matches!(&opened.calls.borrow()[..], [Call::OpenRef { pane: None, .. }]));
}

#[gpui_kit::test]
fn a_path_outside_the_checkout_is_text_and_has_no_button(cx: &mut TestAppContext) {
    let values = ["/etc/passwd", "../up.rs", "C:\\x", "a/../b"].map(|path| Value::Path { path: path.into(), line: None }).to_vec();
    let opened = open(cx, values, Some(ViewCtx::agent(7, "ci")));
    for k in 0..4 {
        assert!(!drawn(cx, &opened, &format!("v{k}-ref")), "{k}");
    }
    let inside = open(cx, vec![Value::Path { path: "a/b.rs".into(), line: None }], Some(ViewCtx::agent(7, "ci")));
    assert!(drawn(cx, &inside, "v0-ref"));
}

#[gpui_kit::test]
fn a_saved_view_link_is_pressed_from_the_column_it_is_drawn_in_and_still_pressed_when_missing(cx: &mut TestAppContext) {
    let column = SavedColumn { panel: Panel::Saved("release".into()), column: 1 };
    let ctx = ViewCtx { kind: Kind::Saved, saved: Some(column.clone()), ..ViewCtx::agent(0, "release") };
    let values = vec![Value::View { id: "release".into(), label: None }, Value::View { id: "later".into(), label: Some("Later".into()) }];
    let opened = open(cx, values, Some(ctx));
    press(cx, &opened, "v0-saved");
    press(cx, &opened, "v1-saved");
    let calls = opened.calls.borrow();
    assert!(matches!(&calls[0], Call::OpenSavedView { id, from: Some(from) } if id == "release" && *from == column), "{:?}", calls[0]);
    assert!(matches!(&calls[1], Call::OpenSavedView { id, from: Some(_) } if id == "later"), "a view not saved yet is still asked for");
}

#[gpui_kit::test]
fn a_saved_view_link_from_an_agents_view_names_no_column(cx: &mut TestAppContext) {
    let opened = open(cx, vec![Value::View { id: "release".into(), label: None }], Some(ViewCtx::agent(7, "ci")));
    press(cx, &opened, "v0-saved");
    assert!(matches!(&opened.calls.borrow()[..], [Call::OpenSavedView { from: None, .. }]));
}

#[gpui_kit::test]
fn the_kinds_with_nothing_to_press_draw_and_ask_for_nothing(cx: &mut TestAppContext) {
    let values = vec![
        Value::Text { text: "plain".into() },
        Value::Number { number: 1234.5, unit: Some("ms".into()) },
        Value::Bool { value: true },
        Value::Bool { value: false },
        Value::Date { date: "2026-10-05".into() },
        Value::DateTime { at: "2026-10-21T15:00:00Z".into() },
        Value::Person { name: "Ada Lovelace".into() },
        Value::People { names: ["Ada", "Grace", "Alan", "Edsger"].map(String::from).to_vec() },
        Value::Chip { chip: chip("ok") },
        Value::Chips { chips: vec![chip("a"), chip("b")] },
        Value::Progress { value: 3.0, max: 4.0, tone: Tone::Active },
        Value::Duration { ms: 252_000 },
        Value::Code { text: "x = 1".into() },
        Value::Empty,
        Value::Unknown,
    ];
    let opened = open(cx, values, None);
    assert!(opened.calls.borrow().is_empty());
    // The tooltips and names that are the page's own words.
    assert!(drawn(cx, &opened, "v5-time") && drawn(cx, &opened, "v10-progress") && drawn(cx, &opened, "v11-duration") && drawn(cx, &opened, "v14-unknown"));
}

#[gpui_kit::test]
fn four_people_draw_three_avatars_and_the_rest_are_counted(cx: &mut TestAppContext) {
    let names = ["Ada", "Grace", "Alan", "Edsger"].map(String::from).to_vec();
    let opened = open(cx, vec![Value::People { names }], None);
    for shown in ["Ada", "Grace", "Alan"] {
        assert!(drawn(cx, &opened, format!("avatar-{shown}").as_str()), "{shown}");
    }
    assert!(!drawn(cx, &opened, "avatar-Edsger"), "the fourth is counted, not drawn");
}

#[gpui_kit::test]
fn a_progress_value_says_its_percent_to_a_reader_and_an_overshoot_is_a_full_bar(cx: &mut TestAppContext) {
    let opened = open(cx, vec![Value::Progress { value: 12.0, max: 10.0, tone: Tone::Good }, Value::Progress { value: 1.0, max: 3.0, tone: Tone::Good }], None);
    let said = |cx: &mut TestAppContext, id: &str| cx.update_window(opened.handle, |_, window, _| window.find(id.to_owned()).label().map(str::to_owned)).unwrap();
    assert_eq!(said(cx, "v0-progress").as_deref(), Some("100%"));
    assert_eq!(said(cx, "v1-progress").as_deref(), Some("33%"));
}
