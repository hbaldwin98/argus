//! The Views stage with a saved view open: its header, its blocks with the
//! embeds drawn in place, the markers on what opens another, and the list
//! of links.

use super::*;

use crate::app::View;
use argus_protocol::{SavedView, ServerMsg};

fn press(app: &mut App, c: char) {
    app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
}

fn saved(id: &str, title: &str, blocks: serde_json::Value) -> SavedView {
    serde_json::from_value(serde_json::json!({
        "id": id,
        "scope": { "kind": "global" },
        "built_at": argus_protocol::ago::now() as i64 - 600,
        "stale": false,
        "view": { "id": id, "title": title, "blocks": blocks },
    }))
    .unwrap()
}

/// Two saved views: `outer` embeds `inner`, links to it through a section,
/// and embeds a view that is not saved.
fn app_with_saved() -> App {
    let mut app = app_with_tree();
    let mut outer = saved(
        "outer",
        "Release plan",
        serde_json::json!([
            { "kind": "section", "heading": "Scope", "opens": "inner", "blocks": [
                { "kind": "markdown", "text": "What ships." } ] },
            { "kind": "embed", "view": "inner" },
            { "kind": "embed", "view": "gone" }
        ]),
    );
    outer.stale = true;
    app.on_server_msg(ServerMsg::SavedViews(vec![
        outer,
        saved("inner", "Checklist", serde_json::json!([
            { "kind": "markdown", "text": "Tests pass." } ])),
    ]));
    app.open_view(View::AgentViews);
    app.choose_stage(0);
    app
}

#[test]
fn a_saved_view_shows_its_scope_stale_mark_and_build_time() {
    let mut app = app_with_saved();
    let out = lines(&draw_at(&mut app, 120, 60)).join("\n");
    assert!(out.contains("Release plan"), "{out}");
    assert!(out.contains("global · built"), "{out}");
    assert!(out.contains("stale"), "{out}");
    assert!(out.contains("What ships."), "{out}");
}

#[test]
fn an_embed_is_drawn_under_its_title_and_a_missing_one_says_so() {
    let mut app = app_with_saved();
    let out = lines(&draw_at(&mut app, 120, 60)).join("\n");
    assert!(out.contains("↳ Checklist"), "{out}");
    assert!(out.contains("Tests pass."), "{out}");
    assert!(out.contains("view gone not found"), "{out}");
}

#[test]
fn a_thing_that_opens_a_view_carries_a_marker_after_its_label() {
    let mut app = app_with_saved();
    let out = lines(&draw_at(&mut app, 120, 60)).join("\n");
    assert!(out.contains("Scope ›"), "{out}");
}

#[test]
fn the_links_list_names_each_link_and_marks_one_that_is_not_saved() {
    let mut app = app_with_saved();
    press(&mut app, 'l');
    let out = lines(&draw_at(&mut app, 120, 60)).join("\n");
    assert!(out.contains("links from Release plan"), "{out}");
    assert!(out.contains("inner  Checklist"), "{out}");
    assert!(out.contains("gone  not saved yet"), "{out}");
}

#[test]
fn a_view_that_is_not_saved_is_drawn_as_missing() {
    let mut app = app_with_tree();
    app.open_view(View::AgentViews);
    app.on_server_msg(ServerMsg::SavedViews(vec![
        saved("a", "A", serde_json::json!([])),
        saved("b", "B", serde_json::json!([])),
    ]));
    app.choose_stage(0);
    app.on_server_msg(ServerMsg::SavedViewRemoved { id: "a".into() });
    let out = lines(&draw_at(&mut app, 120, 40)).join("\n");
    assert!(out.contains("view a is not saved"), "{out}");
}

#[test]
#[ignore]
fn dump_saved_view() {
    let mut app = app_with_saved();
    let w = std::env::var("DUMP_W").ok().and_then(|v| v.parse().ok()).unwrap_or(120);
    let h = std::env::var("DUMP_H").ok().and_then(|v| v.parse().ok()).unwrap_or(40);
    for line in lines(&draw_at(&mut app, w, h)) {
        println!("|{line}");
    }
}
