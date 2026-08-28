//! The pane API: what an agent reports about itself, what a child
//! agent may and may not change, and how a pane is sized and restarted.

use super::*;
// --- the pane API ------------------------------------------------------

#[test]
fn a_title_from_a_model_is_flattened_and_cut_to_fit_a_row() {
    assert_eq!(clean_title("  fixing\n the   pty  "), "fixing the pty");
    let long = clean_title(&"x".repeat(200));
    assert!(
        long.chars().count() <= 49,
        "got {} chars",
        long.chars().count()
    );
    assert!(long.ends_with('…'));
    assert_eq!(clean_title("   "), "");
}

#[test]
fn session_ids_are_validated_without_restricting_cli_specific_syntax() {
    assert_eq!(
        valid_session_id("  thread/abc:123  ").as_deref(),
        Some("thread/abc:123")
    );
    assert!(valid_session_id("").is_none());
    assert!(valid_session_id("bad\nid").is_none());
    assert!(valid_session_id(&"x".repeat(513)).is_none());
}

/// A daemon holding one live agent pane, and that pane's id.
async fn daemon_with_an_agent(dir: &std::path::Path) -> (Arc<Daemon>, PaneId) {
    let d = daemon_with_fake_claude(dir);
    let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();
    (d, pane)
}

#[tokio::test]
async fn an_agent_can_rename_its_own_row() {
    // The feature: four rows all reading "claude" say nothing about
    // which one is worth looking at.
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = daemon_with_an_agent(dir.path()).await;
    assert_eq!(pane_info(&d, pane).title, "claude");

    d.set_pane_title(pane, "fixing the pty deadlock");
    assert_eq!(pane_info(&d, pane).title, "fixing the pty deadlock");

    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn an_agent_spawned_inside_a_pane_reports_as_a_child_of_it() {
    // The bug: a CLI started from inside a pane inherits that pane's
    // hook URL and token, so every turn it takes used to overwrite the
    // row belonging to the agent that started it.
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = daemon_with_an_agent(dir.path()).await;
    d.set_pane_session_id(pane, "parent-session");
    d.set_pane_title(pane, "fixing the pty deadlock");
    d.report_pane_status(pane, Some("parent-session"), PaneStatus::Working, None);

    d.report_pane_title(pane, Some("child-session"), "reading the hook table");
    d.report_pane_status(
        pane,
        Some("child-session"),
        PaneStatus::Waiting,
        Some("needs a password".into()),
    );

    let info = pane_info(&d, pane);
    assert_eq!(info.title, "fixing the pty deadlock", "the parent's row");
    assert_eq!(info.status, PaneStatus::Working);
    assert_eq!(info.note, None);
    assert_eq!(info.children.len(), 1);
    assert_eq!(info.children[0].label, "reading the hook table");
    assert_eq!(info.children[0].status, PaneStatus::Waiting);
    assert_eq!(info.children[0].note.as_deref(), Some("needs a password"));

    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn a_child_that_has_finished_stops_being_listed() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = daemon_with_an_agent(dir.path()).await;
    d.set_pane_session_id(pane, "parent-session");
    d.report_pane_status(pane, Some("child-session"), PaneStatus::Working, None);
    assert_eq!(pane_info(&d, pane).children.len(), 1);

    d.report_pane_status(pane, Some("child-session"), PaneStatus::Idle, None);
    assert!(pane_info(&d, pane).children.is_empty());

    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn an_exited_parent_forgets_its_children() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = daemon_with_an_agent(dir.path()).await;
    d.set_pane_session_id(pane, "parent-session");
    d.report_pane_status(pane, Some("child-session"), PaneStatus::Waiting, None);
    assert_eq!(pane_info(&d, pane).children.len(), 1);

    d.clone().mark_pane_exited(pane, Some(1));

    let info = pane_info(&d, pane);
    assert_eq!(info.status, PaneStatus::Exited { code: Some(1) });
    assert!(info.children.is_empty());
    d.close_pane(pane).unwrap();
}
#[tokio::test]
async fn a_parent_going_idle_forgets_what_ran_under_it() {
    // Most children never report finishing: a subagent's harness fires
    // the parent's hooks, not its own. The turn ending is what says
    // they are done, so the row must not keep claiming they are working.
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = daemon_with_an_agent(dir.path()).await;
    d.set_pane_session_id(pane, "parent-session");
    d.report_pane_status(pane, Some("parent-session"), PaneStatus::Working, None);
    d.report_pane_status(pane, Some("child-session"), PaneStatus::Working, None);
    assert_eq!(pane_info(&d, pane).children.len(), 1);

    d.report_pane_status(pane, Some("parent-session"), PaneStatus::Idle, None);
    assert!(pane_info(&d, pane).children.is_empty());

    // A background agent that outlives the turn is not lost by this:
    // the next thing it reports lists it again.
    d.report_pane_status(pane, Some("child-session"), PaneStatus::Working, None);
    assert_eq!(pane_info(&d, pane).children.len(), 1);

    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn a_child_that_goes_quiet_stops_being_listed() {
    // The backstop for a child that is killed or crashes mid-turn:
    // nothing reports its ending, and the parent keeps working, so
    // without this its row would sit there indefinitely.
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = daemon_with_an_agent(dir.path()).await;
    d.set_pane_session_id(pane, "parent-session");
    d.report_pane_status(pane, Some("parent-session"), PaneStatus::Working, None);
    d.report_pane_status(pane, Some("live-child"), PaneStatus::Working, None);
    d.report_pane_status(pane, Some("dead-child"), PaneStatus::Working, None);

    // Age one of them past the silence the sweep allows.
    {
        let mut inner = d.inner.lock().unwrap();
        let p = find_pane(&mut inner.projects, pane).unwrap();
        let child = p
            .children
            .iter_mut()
            .find(|c| c.session_id == "dead-child")
            .unwrap();
        child.at = std::time::Instant::now() - CHILD_SILENCE - Duration::from_secs(1);
    }
    d.drop_silent_children();

    let listed = pane_info(&d, pane).children;
    assert_eq!(listed.len(), 1, "the quiet one is gone");
    assert_eq!(listed[0].label, "agent");

    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn one_client_sizes_a_pane_to_exactly_what_it_asked_for() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_fake_claude(dir.path());
    let pane = d.spawn_shell(only_checkout(&d)).unwrap();
    let alone = d.new_viewer();

    d.resize_pane(alone, pane, 40, 120).unwrap();

    assert_eq!(pane_size(&d, pane), (40, 120));
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn two_clients_get_a_pane_that_fits_in_both_of_their_windows() {
    // Sizing to the later request instead would leave the client that
    // asked first drawing a grid wider or taller than its own box.
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_fake_claude(dir.path());
    let pane = d.spawn_shell(only_checkout(&d)).unwrap();
    let (tall, wide) = (d.new_viewer(), d.new_viewer());

    d.resize_pane(tall, pane, 60, 80).unwrap();
    d.resize_pane(wide, pane, 30, 200).unwrap();

    assert_eq!(pane_size(&d, pane), (30, 80));
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn a_pane_grows_back_when_the_client_holding_it_small_leaves() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_fake_claude(dir.path());
    let pane = d.spawn_shell(only_checkout(&d)).unwrap();
    let (big, small) = (d.new_viewer(), d.new_viewer());
    d.resize_pane(big, pane, 60, 200).unwrap();
    d.resize_pane(small, pane, 20, 80).unwrap();

    d.release_viewer(small);

    assert_eq!(pane_size(&d, pane), (60, 200));
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn a_pane_the_last_client_stopped_showing_keeps_its_size() {
    // Nobody is watching, so there is no size that would be better —
    // and reflowing a running program's output for no reader only
    // damages what it has already drawn.
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_fake_claude(dir.path());
    let pane = d.spawn_shell(only_checkout(&d)).unwrap();
    let only = d.new_viewer();
    d.resize_pane(only, pane, 44, 111).unwrap();

    d.release_pane_size(only, pane);

    assert_eq!(pane_size(&d, pane), (44, 111));
    let _ = d.close_pane(pane);
}


#[tokio::test]
async fn a_pane_lists_only_so_many_children() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = daemon_with_an_agent(dir.path()).await;
    d.set_pane_session_id(pane, "parent-session");
    for i in 0..MAX_CHILDREN + 4 {
        d.report_pane_status(pane, Some(&format!("child-{i}")), PaneStatus::Working, None);
    }
    assert_eq!(pane_info(&d, pane).children.len(), MAX_CHILDREN);
    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn a_session_started_mid_turn_cannot_take_over_the_row() {
    // A nested CLI announces its own session start while its parent is
    // working; letting that claim stick would leave the row resuming
    // the wrong conversation.
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = daemon_with_an_agent(dir.path()).await;
    d.set_pane_session_id(pane, "parent-session");
    d.report_pane_status(pane, Some("parent-session"), PaneStatus::Working, None);

    d.set_pane_session_id(pane, "nested-session");

    assert_eq!(
        d.session_panes()[0].harness_session_id.as_deref(),
        Some("parent-session")
    );
    assert_eq!(pane_info(&d, pane).children.len(), 1);

    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn the_panes_own_agent_can_still_start_a_new_conversation() {
    // `/clear` gives the pane's agent a new session id, and it arrives
    // while the pane is idle. That is the row's own agent, so it keeps
    // the row — and whatever ran under the old conversation is gone.
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = daemon_with_an_agent(dir.path()).await;
    d.set_pane_session_id(pane, "first-session");
    d.report_pane_status(pane, Some("child-session"), PaneStatus::Working, None);

    d.set_pane_session_id(pane, "second-session");

    assert_eq!(
        d.session_panes()[0].harness_session_id.as_deref(),
        Some("second-session")
    );
    assert!(pane_info(&d, pane).children.is_empty());

    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn a_report_with_no_session_at_all_is_the_panes_own() {
    // `argus-hook status` typed by an agent carries no session id, and
    // must keep working as the pane's own voice.
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = daemon_with_an_agent(dir.path()).await;
    d.set_pane_session_id(pane, "parent-session");

    d.report_pane_title(pane, None, "renamed by hand");
    d.report_pane_status(pane, None, PaneStatus::NeedsReview, None);

    let info = pane_info(&d, pane);
    assert_eq!(info.title, "renamed by hand");
    assert_eq!(info.status, PaneStatus::NeedsReview);
    assert!(info.children.is_empty());

    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn an_empty_rename_leaves_the_row_alone() {
    // Better the agent's name than a blank row.
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = daemon_with_an_agent(dir.path()).await;
    d.set_pane_title(pane, "   \n  ");
    assert_eq!(pane_info(&d, pane).title, "claude");
    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn a_stalled_pane_says_what_it_is_stalled_on() {
    // The reason to have a note at all: knowing a pane needs you is
    // only half of it.
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = daemon_with_an_agent(dir.path()).await;

    d.set_pane_hook_status(
        pane,
        PaneStatus::Waiting,
        Some("needs the staging password".to_string()),
    );
    let info = pane_info(&d, pane);
    assert_eq!(info.status, PaneStatus::Waiting);
    assert_eq!(info.note.as_deref(), Some("needs the staging password"));

    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn the_note_goes_away_with_the_state_it_explained() {
    // A stale "waiting on a password" under a working row is worse than
    // no note at all.
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = daemon_with_an_agent(dir.path()).await;

    d.set_pane_hook_status(pane, PaneStatus::Waiting, Some("needs a password".into()));
    d.set_pane_hook_status(pane, PaneStatus::Working, None);

    let info = pane_info(&d, pane);
    assert_eq!(info.status, PaneStatus::Working);
    assert_eq!(info.note, None);

    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn a_failure_keeps_the_pane_alive_and_says_why() {
    // Distinct from an exit: the process is still there to answer.
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = daemon_with_an_agent(dir.path()).await;

    d.set_pane_hook_status(
        pane,
        PaneStatus::Failed,
        Some("cargo test won't build".into()),
    );
    let info = pane_info(&d, pane);
    assert_eq!(info.status, PaneStatus::Failed);
    assert!(info.note.is_some());

    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn automatic_idle_does_not_erase_an_explicit_completion_state() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = daemon_with_an_agent(dir.path()).await;

    for status in [
        PaneStatus::Waiting,
        PaneStatus::NeedsReview,
        PaneStatus::Done,
        PaneStatus::Failed,
    ] {
        d.set_pane_hook_status(pane, status, Some("still relevant".into()));
        d.set_pane_hook_status(pane, PaneStatus::Idle, None);
        let info = pane_info(&d, pane);
        assert_eq!(info.status, status);
        assert_eq!(info.note.as_deref(), Some("still relevant"));
        d.set_pane_hook_status(pane, PaneStatus::Working, None);
    }

    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn an_agent_that_exits_leaves_its_row_alone_by_default() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = daemon_with_an_agent(dir.path()).await;

    d.mark_pane_exited(pane, Some(1));

    let panes = panes_of(&d);
    assert_eq!(panes.len(), 1);
    assert_eq!(panes[0].id, pane, "the same dead row, for reading");
    assert_eq!(panes[0].status, PaneStatus::Exited { code: Some(1) });
}

#[tokio::test]
async fn on_failure_starts_the_agent_again_and_a_clean_exit_does_not() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_a_restarting_agent(dir.path(), crate::config::Restart::OnFailure);
    let checkout = only_checkout(&d);
    let first = d.spawn_agent(checkout, "claude").unwrap();

    d.mark_pane_exited(first, Some(1));

    let panes = panes_of(&d);
    assert_eq!(panes.len(), 1, "the dead row is replaced, not joined");
    let second = panes[0].id;
    assert_ne!(second, first, "a new pane is running");
    assert_ne!(panes[0].status, PaneStatus::Exited { code: Some(1) });

    d.mark_pane_exited(second, Some(0));

    let panes = panes_of(&d);
    assert_eq!(panes[0].id, second, "a clean exit is the agent finishing");
    assert_eq!(panes[0].status, PaneStatus::Exited { code: Some(0) });
    close_all(&d);
}

#[tokio::test]
async fn always_starts_the_agent_again_however_it_ended() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_a_restarting_agent(dir.path(), crate::config::Restart::Always);
    let checkout = only_checkout(&d);
    let first = d.spawn_agent(checkout, "claude").unwrap();

    d.mark_pane_exited(first, Some(0));

    let panes = panes_of(&d);
    assert_eq!(panes.len(), 1);
    assert_ne!(panes[0].id, first);
    close_all(&d);
}

#[tokio::test]
async fn a_cli_that_dies_on_every_start_is_left_where_the_operator_can_read_it() {
    // Restarting forever spends the machine on a row nobody ever sees.
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_a_restarting_agent(dir.path(), crate::config::Restart::Always);
    let checkout = only_checkout(&d);
    let mut pane = d.spawn_agent(checkout, "claude").unwrap();

    for _ in 0..6 {
        d.mark_pane_exited(pane, Some(1));
        pane = panes_of(&d)[0].id;
    }

    let panes = panes_of(&d);
    assert_eq!(panes.len(), 1);
    assert_eq!(
        panes[0].status,
        PaneStatus::Exited { code: Some(1) },
        "it gave up and left the exit visible"
    );
    close_all(&d);
}

#[tokio::test]
async fn closing_a_pane_is_never_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_a_restarting_agent(dir.path(), crate::config::Restart::Always);
    let checkout = only_checkout(&d);
    let pane = d.spawn_agent(checkout, "claude").unwrap();

    d.close_pane(pane).unwrap();
    // Whatever the process does on its way out arrives after the row
    // has already gone.
    d.mark_pane_exited(pane, Some(1));

    assert!(panes_of(&d).is_empty(), "closing means closing");
}

#[tokio::test]
async fn a_report_never_resurrects_an_exited_pane() {
    // A Stop hook racing a crash must not relabel a dead row as idle.
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = daemon_with_an_agent(dir.path()).await;
    d.mark_pane_exited(pane, Some(1));

    d.set_pane_hook_status(pane, PaneStatus::Idle, Some("all done".into()));

    let info = pane_info(&d, pane);
    assert_eq!(info.status, PaneStatus::Exited { code: Some(1) });
    assert_eq!(info.note, None, "an exited pane explains nothing");

    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn a_rename_will_not_relabel_a_dead_row() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = daemon_with_an_agent(dir.path()).await;
    d.mark_pane_exited(pane, Some(0));

    d.set_pane_title(pane, "still working on it");
    assert_eq!(pane_info(&d, pane).title, "claude");

    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn every_agent_pane_is_handed_the_hook_environment() {
    // The universal floor: a harness Argus knows nothing about can still
    // report, because the variables are always there.
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_fake_claude(dir.path());
    d.start_hook_server().unwrap();
    let port = d.hook_port.load(std::sync::atomic::Ordering::Relaxed);
    assert_ne!(port, 0);

    let env = crate::harness::env(PaneId(1), port, &d.hook_token);
    let url = env
        .iter()
        .find(|(k, _)| k == argus_protocol::URL_VAR)
        .map(|(_, v)| v.clone())
        .unwrap();
    assert!(url.contains(&port.to_string()));
    assert!(parse_pane_path("/pane/1/title").is_some());
}

#[tokio::test]
async fn an_agent_can_move_its_live_pane_to_another_checkout() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let d = daemon_with_two_agent_checkouts(first.path(), second.path());
    d.start_hook_server().unwrap();
    let source = d.snapshot()[0].repositories[0].checkouts[0].id;
    let pane = d.spawn_agent(source, "claude").unwrap();

    d.move_agent_to_checkout(pane, second.path()).unwrap();

    let tree = d.snapshot();
    assert!(tree[0].repositories[0].checkouts[0].panes.is_empty());
    assert_eq!(tree[0].repositories[1].checkouts[0].panes[0].id, pane);
    assert_eq!(d.session_panes()[0].checkout_path, second.path());
    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn an_authorized_checkout_hook_moves_the_agent_pane() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let d = daemon_with_two_agent_checkouts(first.path(), second.path());
    d.start_hook_server().unwrap();
    let source = d.snapshot()[0].repositories[0].checkouts[0].id;
    let pane = d.spawn_agent(source, "claude").unwrap();
    let body = second.path().to_string_lossy();
    let request = format!(
        "POST /pane/{}/checkout HTTP/1.1\r\nAuthorization: Bearer {}\r\nContent-Length: {}\r\n\r\n{}",
        pane.0,
        d.hook_token,
        body.len(),
        body
    );

    let port = d.hook_port.load(std::sync::atomic::Ordering::Relaxed);
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();

    assert!(response.starts_with(b"HTTP/1.1 200 OK"));
    assert_eq!(
        d.snapshot()[0].repositories[1].checkouts[0].panes[0].id,
        pane
    );
    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn an_authorized_session_hook_records_exact_identity() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_claude_aliases(dir.path(), &["claude"]);
    d.start_hook_server().unwrap();
    let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();
    let body = "session-123";
    let request = format!(
        "POST /pane/{}/session HTTP/1.1\r\nAuthorization: Bearer {}\r\nContent-Length: {}\r\n\r\n{}",
        pane.0,
        d.hook_token,
        body.len(),
        body
    );
    let port = d.hook_port.load(std::sync::atomic::Ordering::Relaxed);
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();

    assert!(response.starts_with(b"HTTP/1.1 200 OK"));
    assert_eq!(
        d.session_panes()[0].harness_session_id.as_deref(),
        Some(body)
    );
    d.close_pane(pane).unwrap();
}

#[test]
fn session_identity_arriving_before_pane_registration_is_retained() {
    let d = daemon_with_primary("/repo");
    let pane = PaneId(42);
    d.starting_agents
        .lock()
        .unwrap()
        .insert(pane, PendingStart::default());

    d.set_pane_session_id(pane, "session-early");

    assert_eq!(
        d.starting_agents
            .lock()
            .unwrap()
            .get(&pane)
            .and_then(|pending| pending.harness_session_id.as_deref()),
        Some("session-early")
    );
}

#[test]
fn status_arriving_before_pane_registration_is_retained() {
    let d = daemon_with_primary("/repo");
    let pane = PaneId(42);
    d.starting_agents
        .lock()
        .unwrap()
        .insert(pane, PendingStart::default());

    d.set_pane_hook_status(pane, PaneStatus::Working, None);
    d.set_pane_hook_status(
        pane,
        PaneStatus::Waiting,
        Some(" needs the database password ".to_string()),
    );

    assert_eq!(
        d.starting_agents.lock().unwrap()[&pane].status,
        Some((
            PaneStatus::Waiting,
            Some("needs the database password".to_string())
        ))
    );
}

#[test]
fn title_arriving_before_pane_registration_is_retained() {
    let d = daemon_with_primary("/repo");
    let pane = PaneId(42);
    d.starting_agents
        .lock()
        .unwrap()
        .insert(pane, PendingStart::default());

    d.set_pane_title(pane, "starting up");
    d.set_pane_title(pane, " fixing the pty deadlock ");

    assert_eq!(
        d.starting_agents.lock().unwrap()[&pane].title.as_deref(),
        Some("fixing the pty deadlock")
    );
}

#[test]
fn child_reports_arriving_before_pane_registration_are_retained() {
    let d = daemon_with_primary("/repo");
    let pane = PaneId(42);
    d.starting_agents.lock().unwrap().insert(
        pane,
        PendingStart {
            harness_session_id: Some("parent-session".to_string()),
            ..PendingStart::default()
        },
    );

    d.report_pane_status(
        pane,
        Some("child-session"),
        PaneStatus::Waiting,
        Some("needs permission".to_string()),
    );
    d.report_pane_title(pane, Some("child-session"), "test runner");

    let starting = d.starting_agents.lock().unwrap();
    let children = &starting[&pane].children;
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].label.as_deref(), Some("test runner"));
    assert_eq!(children[0].status, PaneStatus::Waiting);
    assert_eq!(children[0].note.as_deref(), Some("needs permission"));
}

#[test]
fn a_working_pending_parent_keeps_ownership_from_a_child() {
    let d = daemon_with_primary("/repo");
    let pane = PaneId(42);
    d.starting_agents.lock().unwrap().insert(
        pane,
        PendingStart {
            harness_session_id: Some("parent-session".to_string()),
            status: Some((PaneStatus::Working, None)),
            ..PendingStart::default()
        },
    );

    d.set_pane_session_id(pane, "child-session");

    let starting = d.starting_agents.lock().unwrap();
    let pending = &starting[&pane];
    assert_eq!(
        pending.harness_session_id.as_deref(),
        Some("parent-session")
    );
    assert_eq!(pending.children.len(), 1);
    assert_eq!(pending.children[0].session_id, "child-session");
}

#[test]
fn pending_parent_lifecycle_changes_clear_children() {
    let d = daemon_with_primary("/repo");
    let pane = PaneId(42);
    d.starting_agents.lock().unwrap().insert(
        pane,
        PendingStart {
            harness_session_id: Some("parent-session".to_string()),
            ..PendingStart::default()
        },
    );
    d.report_pane_status(pane, Some("child-session"), PaneStatus::Working, None);

    d.report_pane_status(pane, Some("parent-session"), PaneStatus::Idle, None);
    assert!(d.starting_agents.lock().unwrap()[&pane].children.is_empty());

    d.report_pane_status(pane, Some("child-session"), PaneStatus::Working, None);
    d.set_pane_session_id(pane, "replacement-session");
    let starting = d.starting_agents.lock().unwrap();
    let pending = &starting[&pane];
    assert_eq!(
        pending.harness_session_id.as_deref(),
        Some("replacement-session")
    );
    assert!(pending.children.is_empty());
}

#[tokio::test]
async fn startup_reports_outrank_saved_restore_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = daemon_with_an_agent(dir.path()).await;
    d.restoring
        .store(true, std::sync::atomic::Ordering::Relaxed);
    d.set_pane_hook_status(pane, PaneStatus::Working, Some("new turn".to_string()));
    d.set_pane_title(pane, "current task");

    d.restore_pane_metadata(
        pane,
        &crate::store::SessionPane {
            checkout_path: dir.path().to_path_buf(),
            kind: PaneKind::Agent,
            title: "previous task".to_string(),
            template: Some("claude".to_string()),
            status: PaneStatus::NeedsReview,
            note: Some("old review".to_string()),
            harness_session_id: None,
            harness: None,
        },
    );
    d.restoring
        .store(false, std::sync::atomic::Ordering::Relaxed);

    let info = pane_info(&d, pane);
    assert_eq!(info.title, "current task");
    assert_eq!(info.status, PaneStatus::Working);
    assert_eq!(info.note.as_deref(), Some("new turn"));
    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn saved_restore_metadata_does_not_resurrect_an_exited_pane() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = daemon_with_an_agent(dir.path()).await;
    d.mark_pane_exited(pane, Some(1));

    d.restore_pane_metadata(
        pane,
        &crate::store::SessionPane {
            checkout_path: dir.path().to_path_buf(),
            kind: PaneKind::Agent,
            title: "previous task".to_string(),
            template: Some("claude".to_string()),
            status: PaneStatus::Working,
            note: Some("old turn".to_string()),
            harness_session_id: None,
            harness: None,
        },
    );

    let info = pane_info(&d, pane);
    assert_eq!(info.status, PaneStatus::Exited { code: Some(1) });
    assert_eq!(info.note, None);
    d.close_pane(pane).unwrap();
}
#[test]
fn session_identity_for_an_unknown_pane_is_ignored() {
    let d = daemon_with_primary("/repo");

    d.set_pane_session_id(PaneId(42), "session-unowned");

    assert!(d.starting_agents.lock().unwrap().is_empty());
}

#[tokio::test]
async fn moving_the_last_agent_moves_managed_hook_routing_too() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let d = daemon_with_two_agent_checkouts(first.path(), second.path());
    d.start_hook_server().unwrap();
    let source = d.snapshot()[0].repositories[0].checkouts[0].id;
    let pane = d.spawn_agent(source, "claude").unwrap();
    assert!(settings_of(first.path()).exists());

    d.move_agent_to_checkout(pane, second.path()).unwrap();

    assert!(!settings_of(first.path()).exists());
    assert!(settings_of(second.path()).exists());
    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn a_pane_cannot_move_to_an_unknown_directory() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let unknown = tempfile::tempdir().unwrap();
    let d = daemon_with_two_agent_checkouts(first.path(), second.path());
    let source = d.snapshot()[0].repositories[0].checkouts[0].id;
    let pane = d.spawn_agent(source, "claude").unwrap();

    assert!(d.move_agent_to_checkout(pane, unknown.path()).is_err());
    assert_eq!(
        d.snapshot()[0].repositories[0].checkouts[0].panes[0].id,
        pane
    );
    d.close_pane(pane).unwrap();
}
