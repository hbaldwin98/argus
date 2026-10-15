//! The saved views: asked for once the daemon can send them, held as it
//! sends them, and the links followed between them.

use super::*;
use argus_protocol::{Hello, SavedView, ServerMsg};

fn saved(id: &str, blocks: serde_json::Value) -> SavedView {
    serde_json::from_value(serde_json::json!({
        "id": id,
        "scope": { "kind": "global" },
        "built_at": 1_700_000_000,
        "view": { "id": id, "title": format!("{id} title"), "blocks": blocks },
    }))
    .unwrap()
}

fn linking(to: &[&str]) -> serde_json::Value {
    serde_json::Value::Array(
        to.iter()
            .map(|id| serde_json::json!({ "kind": "embed", "view": id }))
            .collect(),
    )
}

fn sent_saved_views(sent: &[ClientMsg]) -> usize {
    sent.iter().filter(|m| matches!(m, ClientMsg::WatchSavedViews)).count()
}

/// A harness with one live view and the given saved views, the stage open.
fn stage_with(saved_views: Vec<SavedView>) -> Harness {
    let mut h = Harness::new();
    h.app.on_server_msg(ServerMsg::ViewShown(Box::new(argus_protocol::ShownView {
        pane: PaneId(100),
        at: 0,
        subject: None,
        view: serde_json::from_value(serde_json::json!({ "id": "live", "title": "live", "blocks": [] })).unwrap(),
    })));
    h.app.on_server_msg(ServerMsg::SavedViews(saved_views));
    h.app.open_view(View::AgentViews);
    h.sent();
    h
}

fn open_id(h: &Harness) -> Option<String> {
    h.app.saved_views.open_id().map(str::to_owned)
}

#[test]
fn saved_views_are_asked_for_when_the_daemon_can_send_them() {
    let mut h = Harness::new();
    h.app.greeted(Some(&Hello::this_build()));
    assert_eq!(sent_saved_views(&h.sent()), 1);
}

#[test]
fn a_daemon_that_predates_saved_views_is_never_asked() {
    let mut h = Harness::new();
    h.app.greeted(Some(&Hello { capabilities: Vec::new(), ..Hello::this_build() }));
    assert_eq!(sent_saved_views(&h.sent()), 0);
}

#[test]
fn a_new_connection_replays_the_list_rather_than_doubling_it() {
    let mut h = stage_with(vec![saved("a", serde_json::json!([]))]);
    h.app.greeted(Some(&Hello::this_build()));
    assert!(h.app.saved_views.all().is_empty());
}

#[test]
fn a_changed_view_replaces_its_own_and_a_removed_one_goes() {
    let mut h = stage_with(vec![saved("a", serde_json::json!([])), saved("b", serde_json::json!([]))]);
    h.app.on_server_msg(ServerMsg::SavedViewChanged(Box::new(saved("a", linking(&[])))));
    assert_eq!(h.app.saved_views.all().len(), 2);
    h.app.on_server_msg(ServerMsg::SavedViewRemoved { id: "b".into() });
    let ids: Vec<&str> = h.app.saved_views.all().iter().map(|v| v.id.as_str()).collect();
    assert_eq!(ids, vec!["a"]);
}

#[test]
fn a_failed_request_is_an_alert_naming_the_view() {
    let mut h = stage_with(vec![]);
    h.app.on_server_msg(ServerMsg::SavedViewFailed {
        id: "a".into(),
        reason: "the agent is not running".into(),
    });
    assert!(h.app.status_alert);
    assert!(h.app.status.contains("saved view a failed: the agent is not running"));
}

#[test]
fn n_steps_from_the_last_live_view_into_the_saved_ones() {
    let mut h = stage_with(vec![saved("a", serde_json::json!([])), saved("b", serde_json::json!([]))]);
    assert_eq!(open_id(&h), None, "the live view is open");
    h.keys("n");
    assert_eq!(open_id(&h).as_deref(), Some("a"));
    h.keys("nn");
    assert_eq!(open_id(&h).as_deref(), Some("b"), "the walk stops at the last one");
    h.keys("pp");
    assert_eq!(open_id(&h), None, "back into the live view, which is not a saved one");
    assert_eq!(h.app.agent_views.opened().unwrap().view.id, "live");
}

#[test]
fn l_lists_the_links_and_enter_follows_the_one_under_the_cursor_with_a_trail() {
    let mut h = stage_with(vec![
        saved("a", linking(&["b", "c"])),
        saved("b", linking(&["c"])),
        saved("c", serde_json::json!([])),
    ]);
    h.keys("n");
    h.keys("l");
    assert_eq!(h.app.saved_views.links.as_ref().unwrap().ids, vec!["b", "c"]);
    h.key(KeyCode::Down);
    h.key(KeyCode::Enter);
    assert_eq!(open_id(&h).as_deref(), Some("c"));
    assert!(h.app.saved_views.links.is_none(), "the list closes when one is opened");

    h.keys("h");
    assert_eq!(open_id(&h).as_deref(), Some("a"), "h goes back along the trail");
    h.key(KeyCode::Backspace);
    assert_eq!(open_id(&h).as_deref(), Some("a"), "a was reached from the list, not by a link, so the trail is empty");
    assert_eq!(h.app.status, "nothing to go back to");
}

#[test]
fn esc_closes_the_links_list_before_it_leaves_the_stage() {
    let mut h = stage_with(vec![saved("a", linking(&["b"])), saved("b", serde_json::json!([]))]);
    h.keys("n");
    h.keys("l");
    h.key(KeyCode::Esc);
    assert!(h.app.saved_views.links.is_none());
    assert_eq!(h.app.view, View::AgentViews, "the first esc only closes the list");
    h.key(KeyCode::Esc);
    assert_eq!(h.app.view, View::Workspace);
}

#[test]
fn a_view_with_no_links_says_so_rather_than_opening_an_empty_list() {
    let mut h = stage_with(vec![saved("a", serde_json::json!([]))]);
    h.keys("n");
    h.keys("l");
    assert!(h.app.saved_views.links.is_none());
}

#[test]
fn r_and_s_ask_the_daemon_about_the_open_saved_view() {
    let mut h = stage_with(vec![saved("a", serde_json::json!([]))]);
    h.keys("n");
    h.keys("r");
    h.keys("s");
    let sent = h.sent();
    assert!(sent.iter().any(|m| matches!(m, ClientMsg::RefreshSavedView { id } if id == "a")));
    assert!(sent.iter().any(|m| matches!(m, ClientMsg::MarkSavedViewStale { id } if id == "a")));
}

#[test]
fn x_does_not_close_a_saved_view() {
    let mut h = stage_with(vec![saved("a", serde_json::json!([]))]);
    h.keys("n");
    h.keys("x");
    assert!(h.sent().iter().all(|m| !matches!(m, ClientMsg::CloseView { .. })));
    assert_eq!(open_id(&h).as_deref(), Some("a"));
}
