//! Saying something to an agent: what is refused, what waits for the
//! agent's prompt, and what reaches its terminal.

use std::time::Duration;

use argus_protocol::{Sent, MAX_SEND_BYTES};

use super::*;

/// A daemon whose one agent template is `cmd`.
fn daemon_running(dir: &std::path::Path, cmd: &[&str]) -> Arc<Daemon> {
    let mut config = fake_claude_config(dir);
    config.agents[0].cmd = cmd.iter().map(|s| s.to_string()).collect();
    Daemon::new(config)
}

/// Everything on the pane's screen, row after row.
fn screen(d: &Daemon, pane: PaneId) -> String {
    let (_, _, cells, _, _, _, _) = d.subscribe_pane(pane).unwrap();
    cells
        .iter()
        .map(|row| row.iter().map(|c| c.ch.as_str()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Waits up to `limit` for `seen` to hold of the pane's screen.
async fn screen_shows(d: &Daemon, pane: PaneId, text: &str, limit: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + limit;
    while tokio::time::Instant::now() < deadline {
        if screen(d, pane).contains(text) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

fn queued_ids(d: &Daemon, pane: PaneId) -> Vec<u64> {
    pane_info(d, pane).queued.iter().map(|m| m.id).collect()
}

#[tokio::test]
async fn an_empty_or_oversized_message_or_one_to_a_shell_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_fake_claude(dir.path());
    let checkout = only_checkout(&d);
    let agent = d.spawn_agent(checkout, "claude").unwrap();
    let shell = d.spawn_shell(checkout).unwrap();

    assert!(matches!(d.send_to_agent(agent, "  \n", false), Sent::Refused { .. }));
    let huge = "x".repeat(MAX_SEND_BYTES + 1);
    assert!(matches!(d.send_to_agent(agent, &huge, false), Sent::Refused { .. }));
    for now in [false, true] {
        assert!(
            matches!(d.send_to_agent(shell, "ls", now), Sent::Refused { .. }),
            "a shell is not an agent (now: {now})"
        );
    }
    close_all(&d);
}

#[tokio::test]
async fn a_queued_message_is_listed_on_the_pane_until_it_is_taken_back() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_fake_claude(dir.path());
    let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();
    d.report_pane_status(pane, None, PaneStatus::Working, None);

    let Sent::Queued { id } = d.send_to_agent(pane, "also fix the test", false) else {
        panic!("a working agent's message waits");
    };
    let info = pane_info(&d, pane);
    assert_eq!(info.queued.len(), 1);
    assert_eq!(info.queued[0].text, "also fix the test");

    assert!(d.cancel_queued(pane, id));
    assert!(queued_ids(&d, pane).is_empty());
    assert!(!d.cancel_queued(pane, id), "already gone");
    close_all(&d);
}

#[cfg(unix)]
#[tokio::test]
async fn a_message_waits_for_the_agents_prompt_and_is_typed_once_it_is_back() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_running(dir.path(), &["cat"]);
    let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();
    d.report_pane_status(pane, None, PaneStatus::Working, None);

    assert!(matches!(d.send_to_agent(pane, "hello agent", false), Sent::Queued { .. }));
    assert!(
        !screen_shows(&d, pane, "hello agent", Duration::from_millis(800)).await,
        "nothing is typed into a working agent"
    );

    d.report_pane_status(pane, None, PaneStatus::Idle, None);
    assert!(screen_shows(&d, pane, "hello agent", Duration::from_secs(5)).await);
    assert!(queued_ids(&d, pane).is_empty());
    close_all(&d);
}

#[cfg(unix)]
#[tokio::test]
async fn a_waiting_agent_holds_a_message_so_it_cannot_answer_a_dialog() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_running(dir.path(), &["cat"]);
    let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();
    d.report_pane_status(pane, None, PaneStatus::Waiting, Some("allow?".into()));

    assert!(matches!(d.send_to_agent(pane, "yes do it", false), Sent::Queued { .. }));
    assert!(!screen_shows(&d, pane, "yes do it", Duration::from_millis(800)).await);
    assert_eq!(queued_ids(&d, pane).len(), 1);
    close_all(&d);
}

#[cfg(unix)]
#[tokio::test]
async fn send_now_types_straight_into_an_agent_mid_turn() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_running(dir.path(), &["cat"]);
    let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();
    d.report_pane_status(pane, None, PaneStatus::Working, None);

    assert_eq!(d.send_to_agent(pane, "steer left", true), Sent::Typed);
    assert!(screen_shows(&d, pane, "steer left", Duration::from_secs(5)).await);
    close_all(&d);
}

#[cfg(unix)]
#[tokio::test]
async fn queued_messages_go_out_one_per_turn_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_running(dir.path(), &["cat"]);
    let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();
    d.report_pane_status(pane, None, PaneStatus::Working, None);
    let Sent::Queued { id: first } = d.send_to_agent(pane, "first message", false) else {
        panic!()
    };
    let Sent::Queued { id: second } = d.send_to_agent(pane, "second message", false) else {
        panic!()
    };
    assert_eq!(queued_ids(&d, pane), [first, second]);

    d.report_pane_status(pane, None, PaneStatus::Idle, None);
    assert!(screen_shows(&d, pane, "first message", Duration::from_secs(5)).await);
    assert_eq!(queued_ids(&d, pane), [second], "the second waits for the turn it starts");
    assert!(!screen(&d, pane).contains("second message"));

    // The agent takes its turn and comes back.
    d.report_pane_status(pane, None, PaneStatus::Working, None);
    d.report_pane_status(pane, None, PaneStatus::Idle, None);
    assert!(screen_shows(&d, pane, "second message", Duration::from_secs(5)).await);
    close_all(&d);
}

#[cfg(unix)]
#[tokio::test]
async fn stop_types_the_harness_interrupt_key() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_running(dir.path(), &["cat"]);
    let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();
    d.interrupt(pane).unwrap();
    // A terminal echoes Esc as ^[.
    assert!(screen_shows(&d, pane, "^[", Duration::from_secs(5)).await);
    close_all(&d);
}

#[test]
fn every_built_in_harness_is_interrupted_with_esc_never_ctrl_c() {
    for harness in crate::harness::Harness::builtins() {
        assert_eq!(harness.interrupt_keys(), [0x1b], "{}", harness.name);
    }
}
