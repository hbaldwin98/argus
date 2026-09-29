//! What a pane leaves behind when it leaves the tree: nothing, whichever
//! way it went — closed, restarted, replaced after a failed resume, or
//! taken with a worktree deleted underneath it.

use crate::state::live::LiveReader;
use super::*;

/// A daemon whose claude template runs live through the tee, and the
/// reader that keeps it live. Every pane it starts has an upstream to
/// forget, and `cat` keeps it running until something ends it.
fn live_claude(dir: &std::path::Path, restart: crate::config::Restart) -> (Arc<Daemon>, LiveReader) {
    let mut config = fake_claude_config(dir);
    config.agents[0].cmd = if cfg!(windows) {
        vec!["cmd".to_string(), "/K".to_string()]
    } else {
        vec!["cat".to_string()]
    };
    config.agents[0].live = true;
    config.agents[0].restart = restart;
    let d = Daemon::new(config);
    d.start_tee().unwrap();
    let reader = d.live_reader();
    (d, reader)
}

/// Watches `pane`'s conversation, as a phone would, and checks the pane
/// holds what its removal has to let go of.
fn watched(d: &Arc<Daemon>, pane: PaneId) -> tokio::sync::broadcast::Receiver<ServerMsg> {
    let watching = d.watch_transcript(pane);
    assert!(d.transcript_watched(pane));
    assert!(d.tee_upstreams.lock().unwrap().contains_key(&pane), "a live Claude pane is proxied");
    watching
}

fn assert_nothing_left_of(d: &Daemon, pane: PaneId) {
    assert!(!d.transcript_watched(pane), "the watch of a gone pane ends");
    assert!(
        !d.tee_upstreams.lock().unwrap().contains_key(&pane),
        "a gone pane's upstream is forgotten"
    );
}

#[tokio::test]
async fn closing_a_pane_leaves_nothing_of_it_behind() {
    let dir = tempfile::tempdir().unwrap();
    let (d, _reader) = live_claude(dir.path(), Default::default());
    let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();
    let _watching = watched(&d, pane);

    d.close_pane(pane).unwrap();

    assert_nothing_left_of(&d, pane);
}

#[tokio::test]
async fn a_restarted_agent_leaves_nothing_of_its_old_pane_behind() {
    let dir = tempfile::tempdir().unwrap();
    let (d, _reader) = live_claude(dir.path(), crate::config::Restart::Always);
    let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();
    let _watching = watched(&d, pane);

    d.mark_pane_exited(pane, Some(0));

    let panes = panes_of(&d);
    assert_eq!(panes.len(), 1);
    assert_ne!(panes[0].id, pane, "it was restarted");
    assert_nothing_left_of(&d, pane);
    close_all(&d);
}

#[tokio::test]
async fn a_failed_resume_leaves_nothing_of_its_old_pane_behind() {
    let dir = tempfile::tempdir().unwrap();
    let (d, _reader) = live_claude(dir.path(), Default::default());
    let pane = d
        .start_agent(only_checkout(&d), "claude", Start::Resuming, None)
        .unwrap();
    let _watching = watched(&d, pane);

    d.mark_pane_exited(pane, Some(1));

    let panes = panes_of(&d);
    assert_eq!(panes.len(), 1);
    assert_ne!(panes[0].id, pane, "a fresh agent took its place");
    assert_nothing_left_of(&d, pane);
    close_all(&d);
}

#[tokio::test]
async fn a_deleted_worktree_leaves_nothing_of_its_panes_behind() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = dir.path().join("wt");
    std::fs::create_dir(&worktree).unwrap();
    let (d, _reader) = live_claude(dir.path(), Default::default());
    d.reconcile_worktrees_with(|_| vec![dir.path().to_path_buf(), worktree.clone()]);
    let checkout = d.checkout_at(&worktree).expect("the worktree joined the tree");
    let pane = d.spawn_agent(checkout, "claude").unwrap();
    let _watching = watched(&d, pane);

    d.reconcile_worktrees_with(|_| vec![dir.path().to_path_buf()]);

    assert!(d.checkout_at(&worktree).is_none(), "the worktree left the tree");
    assert_nothing_left_of(&d, pane);
}
