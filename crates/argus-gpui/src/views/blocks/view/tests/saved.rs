//! A saved view drawn inside another: the verdict core gives, the frame, and the context its blocks
//! are drawn in.

use std::rc::Rc;

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::layout::docks::Panel;
use argus_desktop_core::views::saved::{PageSaved, PageScope, SavedViewsPage};
use argus_protocol::AgentView;
use gpui_kit::TestAppContext;
use serde_json::{json, Value};

use super::*;

fn saved(id: &str, title: &str, stale: bool, blocks: Value) -> PageSaved {
    PageSaved {
        id: id.into(),
        panel: Panel::Saved(id.into()),
        scope: PageScope { rank: 0, label: "Global".into() },
        here: true,
        features: vec![],
        recipe: None,
        sources: vec![],
        built_at: 0,
        built_commit: None,
        stale,
        view: AgentView { id: id.into(), title: title.into(), blocks: serde_json::from_value(blocks).unwrap(), ..Default::default() },
        diagrams: vec![],
        refreshing: false,
        pressed: vec![],
        under: None,
    }
}

fn page(views: Vec<PageSaved>) -> Option<Rc<SavedViewsPage>> {
    Some(Rc::new(SavedViewsPage { views, panels: vec![], can_press: true }))
}

fn embedding(id: &str) -> Value {
    json!([{"kind": "embed", "view": id}])
}

fn agent() -> ViewCtx {
    ViewCtx { can_press: true, ..ViewCtx::agent(7, "v") }
}

#[gpui_kit::test]
fn an_embed_draws_the_saved_view_in_a_frame_under_its_title_and_marks_it_stale(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, embedding("inner"));
    show_with(cx, &rig, embedding("inner"), agent(), page(vec![saved("inner", "Inner view", true, json!([{"kind": "callout", "tone": "info", "text": "from inside"}]))]));
    assert_eq!(label(cx, &rig, "7/v.0-embed").as_deref(), Some("Inner view"));
    assert!(exists(cx, &rig, "saved/7/v.0/inner.0-callout"), "its blocks are drawn at a path of their own, per embed");
}

#[gpui_kit::test]
fn an_embed_of_a_view_not_saved_yet_says_so_and_follows_the_view_when_it_is(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, embedding("later"));
    assert_eq!(label(cx, &rig, "7/v.0-embed").as_deref(), Some("view not found: later"));
    show_with(cx, &rig, embedding("later"), agent(), page(vec![saved("later", "Later", false, json!([]))]));
    assert_eq!(label(cx, &rig, "7/v.0-embed").as_deref(), Some("Later"));
}

#[gpui_kit::test]
fn a_view_that_embeds_itself_is_a_line_and_never_a_loop(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, embedding("a"));
    show_with(cx, &rig, embedding("a"), agent(), page(vec![saved("a", "A", false, embedding("a"))]));
    assert_eq!(label(cx, &rig, "saved/7/v.0/a.0-embed").as_deref(), Some("view embeds itself: a"));
}

#[gpui_kit::test]
fn embeds_are_followed_three_deep_and_no_further(cx: &mut TestAppContext) {
    let views = vec![
        saved("a", "A", false, embedding("b")),
        saved("b", "B", false, embedding("c")),
        saved("c", "C", false, embedding("d")),
        saved("d", "D", false, embedding("e")),
        saved("e", "E", false, json!([])),
    ];
    let rig = open(cx, 700.0, embedding("a"));
    show_with(cx, &rig, embedding("a"), agent(), page(views));
    let deepest = "saved/saved/saved/7/v.0/a.0/a>b.0/a>b>c.0-embed";
    assert_eq!(label(cx, &rig, deepest).as_deref(), Some("embedded too deep: d"));
}

#[gpui_kit::test]
fn the_open_button_of_an_embed_asks_rust_for_the_view_in_its_own_column(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, embedding("inner"));
    show_with(cx, &rig, embedding("inner"), agent(), page(vec![saved("inner", "Inner", false, json!([]))]));
    click(cx, &rig, "7/v.0-embed-open");
    assert_eq!(calls(&rig), [said(Call::OpenSavedView { id: "inner".into(), from: None })]);
}

#[test]
fn a_saved_views_blocks_are_keyed_by_column_and_embed_so_a_choice_is_kept_where_it_was_made() {
    let view = saved("inner", "Inner", false, json!([]));
    let top = ViewCtx::saved(&view, None, vec!["inner".into()], 0, "col0", true);
    let again = ViewCtx::saved(&view, None, vec!["inner".into()], 0, "col1", true);
    assert_eq!(top.path, "saved/col0/inner");
    assert_ne!(top.path, again.path, "the same view in two columns has two tabs to choose");
    assert_eq!((top.pane, top.kind, top.id.as_str()), (0, crate::views::ctx::Kind::Saved, "inner"));
}
