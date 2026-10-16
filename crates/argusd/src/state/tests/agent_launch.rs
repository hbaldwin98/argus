//! An agent starting another agent: where it lands, which template it
//! gets, and who may ask.

use super::*;
use argus_protocol::{AgentLaunch, LaunchPlace};

fn asking(prompt: &str, place: LaunchPlace) -> AgentLaunch {
    AgentLaunch { template: None, prompt: Some(prompt.to_string()), place }
}

fn checkout_holding(d: &Daemon, pane: PaneId) -> String {
    d.snapshot()[0].repositories[0]
        .checkouts
        .iter()
        .find(|c| c.panes.iter().any(|p| p.id == pane))
        .map(|c| c.path.clone())
        .expect("the pane is in a checkout")
}

#[tokio::test]
async fn an_agent_starts_another_of_its_own_kind_beside_it() {
    let dir = tempfile::tempdir().unwrap();
    let d = Daemon::new(running_claude_config(dir.path()));
    let parent = d.spawn_agent(only_checkout(&d), "claude").unwrap();

    let started = d
        .launch_agent_for_agent(parent, asking("echo hi", LaunchPlace::Here))
        .await
        .unwrap();

    assert_ne!(started.pane, parent);
    assert_eq!(started.template, "claude");
    assert_eq!(checkout_holding(&d, started.pane), checkout_holding(&d, parent));
    assert_eq!(pane_info(&d, started.pane).kind, PaneKind::Agent);
    d.close_pane(started.pane).unwrap();
    d.close_pane(parent).unwrap();
}

#[tokio::test]
async fn an_agent_starts_another_in_a_new_worktree() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    let d = Daemon::new(running_claude_config(dir.path()));
    let parent = d.spawn_agent(only_checkout(&d), "claude").unwrap();

    let place = LaunchPlace::Worktree { branch: "fix-resize".into(), from: None };
    let started = d.launch_agent_for_agent(parent, asking("echo hi", place)).await.unwrap();

    let snapshot = d.snapshot();
    let worktree = snapshot[0].repositories[0]
        .checkouts
        .iter()
        .find(|c| c.name == "fix-resize")
        .expect("the worktree joined the tree");
    assert_eq!(std::path::Path::new(&worktree.path), started.checkout);
    assert!(worktree.panes.iter().any(|p| p.id == started.pane));
    d.close_pane(started.pane).unwrap();
    d.close_pane(parent).unwrap();
}

#[tokio::test]
async fn a_checkout_argus_does_not_show_is_refused_not_added() {
    let dir = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let d = Daemon::new(running_claude_config(dir.path()));
    let parent = d.spawn_agent(only_checkout(&d), "claude").unwrap();

    let place = LaunchPlace::Checkout { path: elsewhere.path().to_path_buf() };
    let refusal = d.launch_agent_for_agent(parent, asking("go", place)).await.unwrap_err();

    assert!(refusal.to_string().contains("shows no checkout"), "{refusal}");
    assert_eq!(d.snapshot()[0].repositories[0].checkouts[0].panes.len(), 1);
    d.close_pane(parent).unwrap();
}

#[tokio::test]
async fn a_shell_cannot_start_an_agent() {
    let dir = tempfile::tempdir().unwrap();
    let d = Daemon::new(running_claude_config(dir.path()));
    let shell = d.spawn_shell(only_checkout(&d)).unwrap();

    let refusal = d
        .launch_agent_for_agent(shell, asking("go", LaunchPlace::Here))
        .await
        .unwrap_err();

    assert!(refusal.to_string().contains("live agent"), "{refusal}");
    d.close_pane(shell).unwrap();
}
