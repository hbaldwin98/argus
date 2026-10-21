//! Which checkout an agent pane belongs to: following its agent into the
//! worktree a turn ends in, and the move an agent asks for.

use super::*;
use argus_protocol::Report;

/// A running agent in `primary`, and `primary/.argus/worktrees/feat` on
/// disk, which Argus has not discovered yet but Git would list.
fn agent_beside_an_undiscovered_worktree(
    primary: &std::path::Path,
) -> (Arc<Daemon>, PaneId, PathBuf) {
    let worktree = primary.join(".argus").join("worktrees").join("feat");
    std::fs::create_dir_all(worktree.join("src")).unwrap();
    let (d, git) = Daemon::scripted(running_claude_config(primary));
    let listed = vec![primary.to_path_buf(), worktree.clone()];
    git.lists_worktrees(move |_| listed.clone());
    let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();
    (d, pane, worktree)
}

fn turn_ended_in(d: &Daemon, pane: PaneId, reporter: Option<&str>, directory: &std::path::Path) {
    d.follow_status_report(pane, reporter, Report::Idle, Some(directory));
}

fn checkout_of(d: &Daemon, pane: PaneId) -> String {
    d.snapshot()[0].repositories[0]
        .checkouts
        .iter()
        .find(|c| c.panes.iter().any(|p| p.id == pane))
        .map(|c| c.path.clone())
        .expect("the pane is in a checkout")
}

/// Posts a status report the way `argus-hook` does from an agent working
/// in `directory`.
async fn report_from(d: &Arc<Daemon>, pane: PaneId, report: Report, directory: &std::path::Path) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let request = format!(
        "POST {} HTTP/1.1\r\nAuthorization: Bearer {}\r\n{}: {}\r\nContent-Length: 0\r\n\r\n",
        argus_protocol::pane_path(pane, Endpoint::Status(report)),
        d.hook_token,
        argus_protocol::DIRECTORY_HEADER,
        directory.display(),
    );
    let port = d.hook_port.load(std::sync::atomic::Ordering::Relaxed);
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    assert!(response.starts_with(b"HTTP/1.1 200"), "{}", String::from_utf8_lossy(&response));
}

#[tokio::test]
async fn a_tool_call_in_another_worktree_does_not_move_the_pane() {
    let primary = tempfile::tempdir().unwrap();
    let (d, pane, worktree) = agent_beside_an_undiscovered_worktree(primary.path());
    d.start_hook_server().unwrap();

    report_from(&d, pane, Report::Working, &worktree.join("src")).await;

    assert_eq!(checkout_of(&d, pane), primary.path().to_string_lossy());
    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn a_turn_reported_ending_in_another_worktree_moves_the_pane_there() {
    let primary = tempfile::tempdir().unwrap();
    let (d, pane, worktree) = agent_beside_an_undiscovered_worktree(primary.path());
    d.start_hook_server().unwrap();

    report_from(&d, pane, Report::Idle, &worktree.join("src")).await;

    assert_eq!(checkout_of(&d, pane), worktree.to_string_lossy());
    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn a_turn_ending_in_a_new_worktree_moves_the_pane_there() {
    let primary = tempfile::tempdir().unwrap();
    let (d, pane, worktree) = agent_beside_an_undiscovered_worktree(primary.path());

    turn_ended_in(&d, pane, None, &worktree.join("src"));

    assert_eq!(checkout_of(&d, pane), worktree.to_string_lossy());
    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn a_turn_ending_in_its_own_checkout_leaves_the_pane_where_it_is() {
    let primary = tempfile::tempdir().unwrap();
    let (d, pane, _worktree) = agent_beside_an_undiscovered_worktree(primary.path());

    // The worktree sits inside the primary checkout; only a directory inside
    // the worktree itself is in it.
    turn_ended_in(&d, pane, None, primary.path());

    assert_eq!(checkout_of(&d, pane), primary.path().to_string_lossy());
    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn a_pane_moved_by_hand_is_not_pulled_back_by_a_directory_already_followed() {
    let primary = tempfile::tempdir().unwrap();
    let (d, pane, worktree) = agent_beside_an_undiscovered_worktree(primary.path());
    turn_ended_in(&d, pane, None, &worktree);

    d.move_agent_to_checkout(pane, primary.path()).unwrap();
    turn_ended_in(&d, pane, None, &worktree);

    assert_eq!(checkout_of(&d, pane), primary.path().to_string_lossy());
    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn a_cli_started_inside_the_pane_cannot_move_it() {
    let primary = tempfile::tempdir().unwrap();
    let (d, pane, worktree) = agent_beside_an_undiscovered_worktree(primary.path());
    d.set_pane_session_id(pane, "parent-session");

    turn_ended_in(&d, pane, Some("child-session"), &worktree);
    d.move_agent_into(pane, Some("child-session"), &worktree).unwrap();

    assert_eq!(checkout_of(&d, pane), primary.path().to_string_lossy());
    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn a_turn_ending_in_another_repository_does_not_move_the_pane() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let d = daemon_with_two_agent_checkouts(first.path(), second.path());
    let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();

    turn_ended_in(&d, pane, None, second.path());

    assert_eq!(d.snapshot()[0].repositories[0].checkouts[0].panes[0].id, pane);
    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn an_agent_asking_to_move_into_a_worktree_just_made_is_moved() {
    let primary = tempfile::tempdir().unwrap();
    let (d, pane, worktree) = agent_beside_an_undiscovered_worktree(primary.path());

    d.move_agent_into(pane, None, &worktree.join("src")).unwrap();

    assert_eq!(checkout_of(&d, pane), worktree.to_string_lossy());
    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn an_agent_asking_to_move_outside_its_project_is_told_why_not() {
    let primary = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let (d, pane, _worktree) = agent_beside_an_undiscovered_worktree(primary.path());

    let refusal = d.move_agent_into(pane, None, elsewhere.path()).unwrap_err();

    assert!(refusal.to_string().contains("not inside a checkout"), "{refusal}");
    assert_eq!(checkout_of(&d, pane), primary.path().to_string_lossy());
    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn an_agents_browser_pane_follows_it_into_another_checkout() {
    // A browser pane lives in its owner's checkout; left behind, removing
    // the old worktree would close a browser whose agent lives on.
    let primary = tempfile::tempdir().unwrap();
    let (d, pane, worktree) = agent_beside_an_undiscovered_worktree(primary.path());
    let (_reports, heard) = tokio::sync::mpsc::unbounded_channel();
    let browser = d.open_browser_pane(pane, crate::browser::Browser::idle(), heard).unwrap();

    d.move_agent_into(pane, None, &worktree).unwrap();

    assert_eq!(checkout_of(&d, pane), worktree.to_string_lossy());
    assert_eq!(checkout_of(&d, browser), worktree.to_string_lossy());
    d.close_pane(pane).unwrap();
}
