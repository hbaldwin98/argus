//! The views agents keep: what the pane API accepts, refuses and reads
//! back, what a client is told, and what asking an agent to refresh one
//! types to it.

use std::time::Duration;

use argus_protocol::{
    CommitInfo, Endpoint, FeatureWrite, GitStatus, SavedView, SavedViewEntry, SavedViewRecord,
    ViewScope, ViewSource, WriteScope,
};

use super::*;

fn agent(dir: &std::path::Path) -> (Arc<Daemon>, PaneId) {
    let d = daemon_with_running_claude(dir);
    d.start_hook_server().unwrap();
    let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();
    (d, pane)
}

fn text(response: &[u8]) -> String {
    String::from_utf8_lossy(response).to_string()
}

/// The body of a response, after the headers.
fn body(response: &[u8]) -> String {
    let text = text(response);
    text.split_once("\r\n\r\n").map(|(_, body)| body.to_string()).unwrap_or_default()
}

fn save(id: &str, extra: serde_json::Value) -> String {
    let mut write = serde_json::json!({
        "view": {"id": id, "title": format!("title of {id}"), "blocks": [{"kind": "markdown", "text": "hello"}]},
    });
    for (key, value) in extra.as_object().unwrap() {
        write[key] = value.clone();
    }
    serde_json::json!({"action": "save", "view": write}).to_string()
}

fn change(action: &str, id: &str) -> String {
    serde_json::json!({"action": action, "id": id}).to_string()
}

async fn post(d: &Arc<Daemon>, pane: PaneId, body: &str) -> Vec<u8> {
    post_agent_hook(d, pane, Endpoint::SavedView, body).await
}

fn board_of(d: &Daemon) -> String {
    let path = d.snapshot()[0].repositories[0].checkouts[0].path.clone();
    d.board_key(std::path::Path::new(&path))
}

fn changed(news: SavedViewNews) -> SavedView {
    match news {
        SavedViewNews::Changed(view) => *view,
        other => panic!("expected a saved view, got {other:?}"),
    }
}

#[tokio::test]
async fn a_saved_view_is_kept_in_the_repository_and_told_to_the_watchers() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    let (replay, mut rx) = d.watch_saved_views();
    assert!(replay.is_empty());

    let extra = serde_json::json!({
        "recipe": "read the lockfile",
        "sources": [{"kind": "file", "path": "Cargo.lock"}],
    });
    let response = post(&d, pane, &save("deps", extra)).await;
    assert!(text(&response).starts_with("HTTP/1.1 200 OK"), "{}", text(&response));

    let saved = changed(rx.recv().await.unwrap());
    let repository = d.snapshot()[0].repositories[0].id;
    assert_eq!(saved.id, "deps");
    assert_eq!(saved.view.id, "deps");
    assert_eq!(saved.scope, ViewScope::Repository { repository });
    assert_eq!(saved.recipe.as_deref(), Some("read the lockfile"));
    assert_eq!(saved.sources, [ViewSource::File { path: "Cargo.lock".into() }]);
    assert!(saved.built_at > 0 && !saved.stale);

    // It is in the store too, so it outlives this daemon.
    let stored = d.store.saved_views().unwrap();
    assert_eq!((stored.len(), stored[0].board.as_deref()), (1, Some(board_of(&d).as_str())));
    assert_eq!(stored[0].saved_by, Some(pane));
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn a_watcher_that_arrives_later_is_replayed_what_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    post(&d, pane, &save("a", serde_json::json!({"scope": {"kind": "global"}}))).await;
    post(&d, pane, &save("b", serde_json::json!({}))).await;

    let (replay, _rx) = d.watch_saved_views();

    let ids: Vec<&str> = replay.iter().map(|v| v.id.as_str()).collect();
    assert_eq!(ids, ["a", "b"]);
    assert_eq!(replay[0].scope, ViewScope::Global);
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn saving_an_id_again_overwrites_it_and_clears_the_mark() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    let (_, mut rx) = d.watch_saved_views();
    post(&d, pane, &save("deps", serde_json::json!({}))).await;
    changed(rx.recv().await.unwrap());

    post(&d, pane, &change("stale", "deps")).await;
    assert!(changed(rx.recv().await.unwrap()).stale);

    let mut again = serde_json::json!({"recipe": "newer"});
    again["view"] = serde_json::json!({"id": "deps", "title": "newer title", "blocks": []});
    let body = serde_json::json!({"action": "save", "view": again}).to_string();
    post(&d, pane, &body).await;
    let saved = changed(rx.recv().await.unwrap());
    assert!(!saved.stale, "a save clears it");
    assert_eq!((saved.view.title.as_str(), saved.recipe.as_deref()), ("newer title", Some("newer")));
    assert_eq!(d.store.saved_views().unwrap().len(), 1, "in place, no versions");
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn a_refusal_is_the_limits_own_words() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    let (_, mut rx) = d.watch_saved_views();

    for (extra, reason) in [
        (serde_json::json!({"recipe": "x".repeat(4097)}), "recipe"),
        (serde_json::json!({"sources": [{"kind": "url", "url": "file:///etc/passwd"}]}), "sources[0]"),
        (serde_json::json!({"features": [9]}), "no feature #9"),
        (serde_json::json!({"scope": {"kind": "feature", "number": 4}}), "no feature #4"),
    ] {
        let response = text(&post(&d, pane, &save("a", extra)).await);
        assert!(response.starts_with("HTTP/1.1 409"), "{reason}: {response}");
        assert!(response.contains(reason), "{reason}: {response}");
    }
    assert!(rx.try_recv().is_err(), "nothing was kept");
    assert!(d.store.saved_views().unwrap().is_empty());
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn a_view_that_embeds_itself_is_refused_to_the_agent() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    let write = serde_json::json!({"view": {"id": "a", "title": "a", "blocks": [{"kind": "embed", "view": "a"}]}});
    let body = serde_json::json!({"action": "save", "view": write}).to_string();

    let response = text(&post(&d, pane, &body).await);

    assert!(response.starts_with("HTTP/1.1 409") && response.contains("embed itself"), "{response}");
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn an_id_cannot_be_taken_over_from_another_scope() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    post(&d, pane, &save("shared", serde_json::json!({"scope": {"kind": "global"}}))).await;

    let response = text(&post(&d, pane, &save("shared", serde_json::json!({}))).await);

    assert!(response.starts_with("HTTP/1.1 409") && response.contains("belongs somewhere else"), "{response}");
    assert!(d.store.saved_views().unwrap()[0].board.is_none(), "the global view is untouched");
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn the_store_refuses_the_view_past_the_limit_with_a_reason() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    for n in 0..argus_protocol::MAX_SAVED_VIEWS {
        let stored = crate::store::StoredView {
            id: format!("v{n}"),
            board: None,
            feature: None,
            attached: Vec::new(),
            recipe: None,
            sources: Vec::new(),
            built_at: 1,
            built_commit: None,
            checkout: None,
            stale: false,
            saved_by: None,
            view: argus_protocol::AgentView { id: format!("v{n}"), title: "t".into(), ..Default::default() },
        };
        d.store.save_view(&stored).unwrap();
    }
    *d.saved_views.lock().unwrap() = crate::state::views::saved::Held::load(d.store.saved_views().unwrap());

    let response = text(&post(&d, pane, &save("one-more", serde_json::json!({}))).await);
    assert!(response.starts_with("HTTP/1.1 409") && response.contains("delete one"), "{response}");

    // An id already kept can still be saved over.
    let response = text(&post(&d, pane, &save("v3", serde_json::json!({"scope": {"kind": "global"}}))).await);
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn an_agent_lists_and_reads_the_views_it_reaches() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    let board = board_of(&d);
    d.store.add_feature(&board, &FeatureWrite { title: "Dark mode".into(), body: None }, None, None, 1, None).unwrap();
    let extra = serde_json::json!({
        "scope": {"kind": "feature", "number": 1}, "features": [1], "recipe": "r",
    });
    post(&d, pane, &save("a", extra)).await;
    post(&d, pane, &save("g", serde_json::json!({"scope": {"kind": "global"}}))).await;
    // Another repository's view is out of its reach.
    let mut elsewhere = d.store.saved_views().unwrap().remove(0);
    elsewhere.id = "elsewhere".into();
    elsewhere.view.id = "elsewhere".into();
    elsewhere.board = Some("repository\0/not/here".into());
    elsewhere.feature = None;
    elsewhere.attached.clear();
    d.store.save_view(&elsewhere).unwrap();
    *d.saved_views.lock().unwrap() = crate::state::views::saved::Held::load(d.store.saved_views().unwrap());

    let listed: Vec<SavedViewEntry> = serde_json::from_str(&body(&post(&d, pane, r#"{"action":"list"}"#).await)).unwrap();
    let ids: Vec<&str> = listed.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["a", "g"]);
    assert_eq!(listed[0].scope, WriteScope::Feature { number: 1 });

    let record: SavedViewRecord = serde_json::from_str(&body(&post(&d, pane, &change("get", "a")).await)).unwrap();
    assert_eq!((record.features.as_slice(), record.recipe.as_deref()), (&[1][..], Some("r")));
    let response = text(&post(&d, pane, &change("get", "elsewhere")).await);
    assert!(response.starts_with("HTTP/1.1 409") && response.contains("can reach"), "{response}");
    let response = text(&post(&d, pane, &change("delete", "elsewhere")).await);
    assert!(response.starts_with("HTTP/1.1 409"), "{response}");
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn deleting_tells_the_watchers_and_forgets_the_view() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    let (_, mut rx) = d.watch_saved_views();
    post(&d, pane, &save("deps", serde_json::json!({}))).await;
    changed(rx.recv().await.unwrap());

    let response = text(&post(&d, pane, &change("delete", "deps")).await);

    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(matches!(rx.recv().await.unwrap(), SavedViewNews::Removed { id } if id == "deps"));
    assert!(d.store.saved_views().unwrap().is_empty());
    // Gone is what was asked for.
    let response = text(&post(&d, pane, &change("delete", "deps")).await);
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn a_feature_that_goes_leaves_its_views_in_the_repository() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    let board = board_of(&d);
    d.store.add_feature(&board, &FeatureWrite { title: "Dark mode".into(), body: None }, None, None, 1, None).unwrap();
    post(&d, pane, &save("a", serde_json::json!({"scope": {"kind": "feature", "number": 1}, "features": [1]}))).await;
    let (replay, mut rx) = d.watch_saved_views();
    assert!(matches!(replay[0].scope, ViewScope::Feature { number: 1, .. }));
    assert_eq!(replay[0].features.len(), 1);

    d.store.remove_feature(&board, "dark-mode").unwrap();
    *d.saved_views.lock().unwrap() = crate::state::views::saved::Held::load(d.store.saved_views().unwrap());
    d.push_saved_views();

    let saved = changed(rx.recv().await.unwrap());
    assert!(matches!(saved.scope, ViewScope::Repository { .. }), "{:?}", saved.scope);
    assert!(saved.features.is_empty());
    let _ = d.close_pane(pane);
}

/// Says the checkout is on `oid`, as a status poll would.
fn head_is(d: &Daemon, oid: &str) {
    let mut inner = d.inner.lock().unwrap();
    for checkout in super::super::tree::checkouts_mut(&mut inner.projects) {
        checkout.git = Some(GitStatus {
            branch: Some("main".into()),
            dirty: false,
            changed_files: 0,
            staged: 0,
            unstaged: 0,
            ahead: 0,
            behind: 0,
            upstream: None,
            last_commit: Some(CommitInfo {
                oid: oid.into(),
                short: oid.chars().take(7).collect(),
                summary: String::new(),
                author: String::new(),
                time: 0,
            }),
            changes: 0,
        });
    }
}

#[tokio::test]
async fn a_checkout_that_moves_past_the_build_marks_the_view_stale_until_it_is_saved_again() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    head_is(&d, "aaaa111");
    post(&d, pane, &save("deps", serde_json::json!({}))).await;
    // The temporary directory is no repository, so the build recorded none;
    // say what it would have been.
    {
        let mut held = d.saved_views.lock().unwrap();
        let mut stored = held.rows[0].stored.clone();
        stored.built_commit = Some("aaaa111".into());
        d.store.save_view(&stored).unwrap();
        held.put(stored);
    }
    let (replay, mut rx) = d.watch_saved_views();
    assert!(!replay[0].stale);

    d.push_saved_views();
    assert!(rx.try_recv().is_err(), "a tree that changes nothing it names sends nothing");

    head_is(&d, "bbbb222");
    d.push_saved_views();
    assert!(changed(rx.recv().await.unwrap()).stale);
    d.push_saved_views();
    assert!(rx.try_recv().is_err(), "and tells it once");

    head_is(&d, "aaaa111");
    d.push_saved_views();
    assert!(!changed(rx.recv().await.unwrap()).stale, "back where it was built, it is current");
    assert!(!d.store.saved_views().unwrap()[0].stale, "the move itself is never stored");
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn a_person_marking_stale_and_deleting_is_told_to_the_watchers() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    let (_, mut rx) = d.watch_saved_views();
    post(&d, pane, &save("deps", serde_json::json!({}))).await;
    changed(rx.recv().await.unwrap());

    d.mark_saved_view_stale("deps").unwrap();
    assert!(changed(rx.recv().await.unwrap()).stale);
    assert!(d.mark_saved_view_stale("nope").unwrap_err().contains("no longer saved"));

    d.delete_saved_view("deps").unwrap();
    assert!(matches!(rx.recv().await.unwrap(), SavedViewNews::Removed { .. }));
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn a_refresh_types_the_recipe_and_sources_to_the_agent_that_saved_it() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    d.set_pane_session_id(pane, "s1");
    let extra = serde_json::json!({
        "recipe": "run cargo tree", "sources": [{"kind": "file", "path": "Cargo.lock"}],
    });
    post(&d, pane, &save("deps", extra)).await;
    let (_, mut inbox) = super::inbox::open_inbox(&d, pane, "s1").await;

    d.refresh_saved_view("deps").unwrap();

    let argus_protocol::InboxItem::Message { text } = super::inbox::next_item(&mut inbox).await;
    assert!(text.contains("`deps`") && text.contains("run cargo tree") && text.contains("file Cargo.lock"), "{text}");
    assert!(text.contains("view save --stdin") && !text.contains('\n'), "{text}");

    assert!(d.refresh_saved_view("nope").unwrap_err().contains("no longer saved"));
    let nothing = tokio::time::timeout(Duration::from_millis(300), super::inbox::next_item(&mut inbox)).await;
    assert!(nothing.is_err(), "nothing was sent for a view that is not kept");
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn a_refresh_is_refused_with_a_reason_when_no_agent_can_be_asked() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    post(&d, pane, &save("deps", serde_json::json!({}))).await;

    d.mark_pane_exited(pane, Some(0));
    assert!(d.refresh_saved_view("deps").unwrap_err().contains("not running"));

    // A restart forgets which pane saved it; nobody is left to ask.
    let mut forgotten = d.store.saved_views().unwrap().remove(0);
    forgotten.saved_by = None;
    d.store.save_view(&forgotten).unwrap();
    *d.saved_views.lock().unwrap() = crate::state::views::saved::Held::load(d.store.saved_views().unwrap());
    assert!(d.refresh_saved_view("deps").unwrap_err().contains("since Argus started"));
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn a_press_in_a_saved_view_is_relayed_only_if_the_view_offers_it() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    d.set_pane_session_id(pane, "s1");
    let write = serde_json::json!({"view": {
        "id": "t", "title": "t", "actions": [{"id": "retry", "label": "Retry"}], "blocks": [],
    }});
    post(&d, pane, &serde_json::json!({"action": "save", "view": write}).to_string()).await;
    let (_, mut inbox) = super::inbox::open_inbox(&d, pane, "s1").await;
    let press = |action: &str| argus_protocol::ViewPress {
        view: "t".into(),
        action: action.into(),
        rows: vec![],
        form: None,
        answers: vec![],
    };

    assert!(d.press_saved_view("t", press("rm-rf")).unwrap_err().contains("no longer offers"));
    assert!(d.press_saved_view("gone", press("retry")).unwrap_err().contains("no longer saved"));
    d.press_saved_view("t", press("retry")).unwrap();

    let argus_protocol::InboxItem::Message { text } = super::inbox::next_item(&mut inbox).await;
    assert!(text.contains("`retry`") && text.contains("Retry"), "{text}");
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn a_saved_view_whose_repository_is_not_in_the_tree_is_kept_and_not_sent() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    let stored = crate::store::StoredView {
        id: "orphan".into(),
        board: Some("repository\0/gone".into()),
        feature: None,
        attached: Vec::new(),
        recipe: None,
        sources: Vec::new(),
        built_at: 1,
        built_commit: None,
        checkout: None,
        stale: false,
        saved_by: None,
        view: argus_protocol::AgentView { id: "orphan".into(), title: "t".into(), ..Default::default() },
    };
    d.store.save_view(&stored).unwrap();
    *d.saved_views.lock().unwrap() = crate::state::views::saved::Held::load(d.store.saved_views().unwrap());

    assert!(d.watch_saved_views().0.is_empty());
    assert_eq!(d.store.saved_views().unwrap().len(), 1);
    let _ = d.close_pane(pane);
}
