//! The views agents show: what the pane API accepts, refuses and holds,
//! and who is told.

use argus_protocol::{AgentView, ServerMsg, ViewChange, MAX_VIEWS_PER_PANE, MAX_VIEW_BYTES};

use super::*;

fn agent(dir: &std::path::Path) -> (Arc<Daemon>, PaneId) {
    let d = daemon_with_running_claude(dir);
    d.start_hook_server().unwrap();
    let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();
    (d, pane)
}

fn show(id: &str, subject: Option<&str>) -> String {
    let view = serde_json::json!({
        "id": id,
        "title": format!("title of {id}"),
        "subject": subject,
        "blocks": [{"kind": "markdown", "text": "hello"}],
    });
    serde_json::json!({"action": "show", "view": view}).to_string()
}

fn text(response: &[u8]) -> String {
    String::from_utf8_lossy(response).to_string()
}

#[tokio::test]
async fn a_shown_view_is_stamped_with_its_pane_and_the_link_its_subject_is() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    let (replay, mut rx) = d.watch_views();
    assert!(replay.is_empty());

    let body = show("eng-1", Some("https://linear.app/acme/issue/ENG-1/fix"));
    let response = text(&post_agent_hook(&d, pane, Endpoint::View, &body).await);
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");

    let ServerMsg::ViewShown(shown) = rx.recv().await.unwrap() else {
        panic!("expected the view");
    };
    assert_eq!(shown.pane, pane);
    assert!(shown.at > 0);
    assert_eq!(shown.view.id, "eng-1");
    assert_eq!(shown.subject.as_ref().map(|l| l.key.as_str()), Some("ENG-1"));
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn a_view_that_breaks_a_limit_is_refused_in_the_limits_own_words() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    let (_, mut rx) = d.watch_views();
    let rows = vec![serde_json::json!([{"kind": "text", "text": "x"}]); 201];
    let view = serde_json::json!({
        "id": "big", "title": "big",
        "blocks": [{"kind": "table", "columns": [{"title": "a"}], "rows": rows}],
    });
    let body = serde_json::json!({"action": "show", "view": view}).to_string();

    let response = text(&post_agent_hook(&d, pane, Endpoint::View, &body).await);

    assert!(response.starts_with("HTTP/1.1 409"), "{response}");
    assert!(response.contains("blocks[0]"), "the path reaches the agent: {response}");
    assert!(rx.try_recv().is_err(), "nothing was shown");
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn a_subject_that_is_not_a_web_address_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    for subject in ["javascript:alert(1)", "file:///etc/passwd", "ENG-1"] {
        let body = show("x", Some(subject));
        let response = text(&post_agent_hook(&d, pane, Endpoint::View, &body).await);
        assert!(response.starts_with("HTTP/1.1 409"), "{subject}: {response}");
        assert!(response.contains("subject"), "{subject}: {response}");
    }
    assert!(d.watch_views().0.is_empty());
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn a_body_past_the_views_bound_is_refused_unread() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    let body = "x".repeat(MAX_VIEW_BYTES + 1);

    let response = text(&post_agent_hook(&d, pane, Endpoint::View, &body).await);

    assert!(response.starts_with("HTTP/1.1 413"), "{response}");
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn a_body_that_is_not_a_view_change_says_what_was_wrong() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    let body = r#"{"action":"show","view":{"id":"a","title":"a"}}"#;

    let response = text(&post_agent_hook(&d, pane, Endpoint::View, body).await);

    assert!(response.starts_with("HTTP/1.1 400"), "{response}");
    assert!(response.contains("blocks"), "{response}");
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn a_block_kind_this_argus_does_not_know_is_refused_to_the_agent_by_its_path() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    let body = r#"{"action":"show","view":{"id":"a","title":"a","blocks":[
        {"kind":"markdown","text":"x"},{"kind":"hologram"}]}}"#;

    let response = text(&post_agent_hook(&d, pane, Endpoint::View, body).await);

    assert!(response.starts_with("HTTP/1.1 409"), "{response}");
    assert!(response.contains("blocks[1]: no block kind by that name"), "{response}");
    assert!(d.watch_views().0.is_empty());
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn only_an_agent_may_show_a_view() {
    let dir = tempfile::tempdir().unwrap();
    let (d, agent_pane) = agent(dir.path());
    let shell = d.spawn_shell(only_checkout(&d)).unwrap();

    let response = text(&post_agent_hook(&d, shell, Endpoint::View, &show("a", None)).await);

    assert!(response.starts_with("HTTP/1.1 409"), "{response}");
    assert!(d.watch_views().0.is_empty());
    let _ = d.close_pane(shell);
    let _ = d.close_pane(agent_pane);
}

#[tokio::test]
async fn closing_a_view_tells_watchers_and_closing_what_is_gone_is_no_error() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    post_agent_hook(&d, pane, Endpoint::View, &show("a", None)).await;
    let (replay, mut rx) = d.watch_views();
    assert_eq!(replay.len(), 1);

    let close = serde_json::to_string(&ViewChange::Close { id: "a".into() }).unwrap();
    let first = text(&post_agent_hook(&d, pane, Endpoint::View, &close).await);
    let again = text(&post_agent_hook(&d, pane, Endpoint::View, &close).await);

    assert!(first.starts_with("HTTP/1.1 200") && again.starts_with("HTTP/1.1 200"));
    let closed = rx.recv().await.unwrap();
    assert!(matches!(closed, ServerMsg::ViewClosed { pane: p, id } if p == pane && id == "a"));
    assert!(rx.try_recv().is_err(), "the second close changed nothing");
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn showing_past_the_limit_closes_the_oldest_for_watchers() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    let (_, mut rx) = d.watch_views();
    for n in 0..=MAX_VIEWS_PER_PANE {
        post_agent_hook(&d, pane, Endpoint::View, &show(&format!("v{n}"), None)).await;
    }
    let mut closed = Vec::new();
    while let Ok(msg) = rx.try_recv() {
        if let ServerMsg::ViewClosed { id, .. } = msg {
            closed.push(id);
        }
    }
    assert_eq!(closed, ["v0"]);
    assert_eq!(d.watch_views().0.len(), MAX_VIEWS_PER_PANE);
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn a_pane_that_is_closed_takes_its_views_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    post_agent_hook(&d, pane, Endpoint::View, &show("a", None)).await;
    post_agent_hook(&d, pane, Endpoint::View, &show("b", None)).await;
    let (_, mut rx) = d.watch_views();

    d.close_pane(pane).unwrap();

    let mut closed = Vec::new();
    while let Ok(ServerMsg::ViewClosed { id, .. }) = rx.try_recv() {
        closed.push(id);
    }
    assert_eq!(closed, ["a", "b"]);
    assert!(d.watch_views().0.is_empty());
}

#[tokio::test]
async fn a_replay_is_oldest_first_and_a_replaced_view_keeps_its_place() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    for id in ["a", "b", "a"] {
        post_agent_hook(&d, pane, Endpoint::View, &show(id, None)).await;
    }
    let ids: Vec<_> = d.watch_views().0.into_iter().map(|v| v.view.id).collect();
    assert_eq!(ids, ["a", "b"]);
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn asking_for_a_refresh_tells_the_agent_to_read_the_subject_again() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    d.set_pane_session_id(pane, "s1");
    let body = show("eng-1", Some("https://linear.app/acme/issue/ENG-1/fix"));
    post_agent_hook(&d, pane, Endpoint::View, &body).await;
    let (_, mut inbox) = super::inbox::open_inbox(&d, pane, "s1").await;

    d.refresh_view(pane, "eng-1");

    let argus_protocol::InboxItem::Message { text } = super::inbox::next_item(&mut inbox).await;
    assert!(text.contains("`eng-1`") && text.contains("linear.app/acme/issue/ENG-1"), "{text}");
    assert!(text.contains("view show --stdin"), "{text}");

    // A view the pane does not have is not asked after.
    d.refresh_view(pane, "nope");
    let nothing = tokio::time::timeout(
        Duration::from_millis(300),
        super::inbox::next_item(&mut inbox),
    )
    .await;
    assert!(nothing.is_err(), "nothing was sent");
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn pressing_an_action_tells_the_agent_and_an_unknown_one_is_not_relayed() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    d.set_pane_session_id(pane, "s1");
    let view = serde_json::json!({
        "id": "t", "title": "t",
        "actions": [{"id": "retry", "label": "Retry"}],
        "blocks": [{"kind": "markdown", "text": "hello"}],
    });
    let body = serde_json::json!({"action": "show", "view": view}).to_string();
    post_agent_hook(&d, pane, Endpoint::View, &body).await;
    let (_, mut inbox) = super::inbox::open_inbox(&d, pane, "s1").await;
    let press = |action: &str| argus_protocol::ViewPress {
        view: "t".into(),
        action: action.into(),
        rows: vec![],
        form: None,
        answers: vec![],
    };

    assert!(d.press_view(pane, press("rm-rf")).is_err());
    d.press_view(pane, press("retry")).unwrap();

    let argus_protocol::InboxItem::Message { text } = super::inbox::next_item(&mut inbox).await;
    assert!(text.contains("`retry`") && text.contains("Retry") && text.contains("`t`"), "{text}");
    let nothing = tokio::time::timeout(
        Duration::from_millis(300),
        super::inbox::next_item(&mut inbox),
    )
    .await;
    assert!(nothing.is_err(), "the unknown action was not relayed");
    let _ = d.close_pane(pane);
}

#[test]
fn a_view_change_round_trips_as_the_agent_writes_it() {
    let view = AgentView {
        id: "a".into(),
        title: "t".into(),
        ..Default::default()
    };
    let json = serde_json::to_string(&ViewChange::Show { view: view.clone() }).unwrap();
    assert_eq!(serde_json::from_str::<ViewChange>(&json).unwrap(), ViewChange::Show { view });
}

fn show_refreshing(id: &str, every: Option<u32>) -> String {
    let view = serde_json::json!({
        "id": id,
        "title": "t",
        "refresh_every": every,
        "blocks": [{"kind": "markdown", "text": "hello"}],
    });
    serde_json::json!({"action": "show", "view": view}).to_string()
}

fn refreshing(d: &Daemon, pane: PaneId, id: &str) -> bool {
    d.agent_views.lock().unwrap().timers.is_refreshing(pane, id)
}

#[tokio::test]
async fn a_view_with_an_interval_is_refreshed_until_it_is_shown_without_one_or_closed() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    post_agent_hook(&d, pane, Endpoint::View, &show_refreshing("a", Some(60))).await;
    post_agent_hook(&d, pane, Endpoint::View, &show_refreshing("b", None)).await;
    assert!(refreshing(&d, pane, "a"));
    assert!(!refreshing(&d, pane, "b"));

    // Shown again with an interval it still has one, and one only.
    post_agent_hook(&d, pane, Endpoint::View, &show_refreshing("a", Some(120))).await;
    assert!(refreshing(&d, pane, "a"));
    post_agent_hook(&d, pane, Endpoint::View, &show_refreshing("a", None)).await;
    assert!(!refreshing(&d, pane, "a"));

    post_agent_hook(&d, pane, Endpoint::View, &show_refreshing("a", Some(60))).await;
    d.close_view(pane, "a");
    assert!(!refreshing(&d, pane, "a"));

    post_agent_hook(&d, pane, Endpoint::View, &show_refreshing("c", Some(60))).await;
    let _ = d.close_pane(pane);
    assert!(!refreshing(&d, pane, "c"));
}

fn patch(id: &str, path: &str, block: serde_json::Value) -> String {
    serde_json::json!({"action": "patch", "id": id, "path": path, "block": block}).to_string()
}

fn timer_of(d: &Daemon, pane: PaneId, id: &str) -> Option<u64> {
    d.agent_views.lock().unwrap().timers.generation(pane, id)
}

#[tokio::test]
async fn a_patch_keeps_the_views_refresh_timer_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    post_agent_hook(&d, pane, Endpoint::View, &show_refreshing("a", Some(60))).await;
    let timer = timer_of(&d, pane, "a");
    assert!(timer.is_some());
    let new = serde_json::json!({"kind": "markdown", "text": "newer"});
    let response = text(&post_agent_hook(&d, pane, Endpoint::View, &patch("a", "blocks[0]", new)).await);
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert_eq!(timer_of(&d, pane, "a"), timer, "the same timer task, not a restarted one");
    let shown = d.watch_views().0;
    assert!(matches!(&shown[0].view.blocks[0], argus_protocol::Block::Markdown { text } if text == "newer"));

    let bad = serde_json::json!({"kind": "markdown", "text": "x"});
    let response = text(&post_agent_hook(&d, pane, Endpoint::View, &patch("a", "blocks[3]", bad)).await);
    assert!(response.starts_with("HTTP/1.1 409") && response.contains("blocks[3]"), "{response}");
    assert_eq!(timer_of(&d, pane, "a"), timer);
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn showing_a_view_again_keeps_its_timer_unless_the_interval_changed() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    post_agent_hook(&d, pane, Endpoint::View, &show_refreshing("a", Some(60))).await;
    let first = timer_of(&d, pane, "a");

    post_agent_hook(&d, pane, Endpoint::View, &show_refreshing("a", Some(60))).await;
    assert_eq!(timer_of(&d, pane, "a"), first, "an unchanged interval keeps the clock");

    post_agent_hook(&d, pane, Endpoint::View, &show_refreshing("a", Some(120))).await;
    let second = timer_of(&d, pane, "a");
    assert!(second.is_some() && second != first, "a new interval restarts it");
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn patches_made_together_are_all_applied() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    let blocks: Vec<_> = (0..8).map(|n| serde_json::json!({"kind": "markdown", "text": format!("b{n}")})).collect();
    let view = serde_json::json!({"id": "a", "title": "t", "refresh_every": 60, "blocks": blocks});
    let body = serde_json::json!({"action": "show", "view": view}).to_string();
    post_agent_hook(&d, pane, Endpoint::View, &body).await;
    let timer = timer_of(&d, pane, "a");

    let mut tasks = Vec::new();
    for n in 0..8 {
        let d = d.clone();
        tasks.push(tokio::task::spawn_blocking(move || {
            let block = serde_json::from_value(serde_json::json!({"kind": "markdown", "text": format!("p{n}")}));
            let change = ViewChange::Patch { id: "a".into(), path: format!("blocks[{n}]"), block: block.unwrap() };
            d.change_view_for_agent(pane, change)
        }));
    }
    for task in tasks {
        task.await.unwrap().unwrap();
    }

    let shown = d.watch_views().0;
    let texts: Vec<_> = shown[0]
        .view
        .blocks
        .iter()
        .map(|b| match b {
            argus_protocol::Block::Markdown { text } => text.clone(),
            _ => "?".into(),
        })
        .collect();
    assert_eq!(texts, ["p0", "p1", "p2", "p3", "p4", "p5", "p6", "p7"], "no patch overwrote another");
    assert_eq!(timer_of(&d, pane, "a"), timer);
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn a_view_shown_as_its_pane_is_removed_is_not_left_behind() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    let view: ViewChange = serde_json::from_str(&show_refreshing("a", Some(60))).unwrap();

    // The show passes its first look at the pane and waits for the views
    // lock; the pane is then removed, its views dropped as soon as the
    // lock allows.
    let held = d.agent_views.lock().unwrap();
    let shower = {
        let d = d.clone();
        std::thread::spawn(move || d.change_view_for_agent(pane, view))
    };
    std::thread::sleep(Duration::from_millis(150));
    let closer = {
        let d = d.clone();
        std::thread::spawn(move || d.close_pane(pane))
    };
    std::thread::sleep(Duration::from_millis(150));
    drop(held);
    let _ = shower.join().unwrap();
    closer.join().unwrap().unwrap();

    assert!(d.watch_views().0.is_empty(), "no view outlives its pane");
    assert!(!refreshing(&d, pane, "a"), "nor a timer");
}

#[tokio::test]
async fn a_press_that_cannot_be_relayed_says_why() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    d.set_pane_session_id(pane, "s1");
    let view = serde_json::json!({
        "id": "t", "title": "t",
        "actions": [{"id": "retry", "label": "Retry"}],
        "blocks": [{"kind": "markdown", "text": "hello"}],
    });
    let body = serde_json::json!({"action": "show", "view": view}).to_string();
    post_agent_hook(&d, pane, Endpoint::View, &body).await;
    let press = |view: &str, action: &str| argus_protocol::ViewPress {
        view: view.into(),
        action: action.into(),
        rows: vec![],
        form: None,
        answers: vec![],
    };

    assert!(d.press_view(pane, press("gone", "retry")).unwrap_err().contains("no longer shown"));
    assert!(d.press_view(pane, press("t", "rm-rf")).unwrap_err().contains("no longer offers"));

    d.mark_pane_exited(pane, Some(0));
    assert!(d.press_view(pane, press("t", "retry")).unwrap_err().contains("not running"));
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn the_refresh_timer_asks_the_agent_each_interval_until_the_view_goes() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    d.set_pane_session_id(pane, "s1");
    post_agent_hook(&d, pane, Endpoint::View, &show_refreshing("a", None)).await;
    let (_, mut inbox) = super::inbox::open_inbox(&d, pane, "s1").await;

    // The shortest interval the protocol allows is a minute, so the loop
    // itself is run at a pace a test can wait for.
    let timer = tokio::spawn(super::view_timers::keep_refreshing(
        Arc::downgrade(&d),
        pane,
        "a".into(),
        Duration::from_millis(50),
    ));
    for _ in 0..2 {
        let argus_protocol::InboxItem::Message { text } = super::inbox::next_item(&mut inbox).await;
        assert!(text.contains("`a`") && text.contains("view show --stdin"), "{text}");
    }
    d.close_view(pane, "a");
    tokio::time::timeout(Duration::from_secs(5), timer).await.expect("the timer ends").unwrap();
    let _ = d.close_pane(pane);
}
