//! What a pane leaves behind when it leaves the tree: nothing, whichever
//! way it went — closed, restarted, replaced after a failed resume, or
//! taken with a worktree deleted underneath it.

use super::*;

/// A daemon whose claude template runs `cat`, which keeps a pane running
/// until something ends it.
fn running_claude(dir: &std::path::Path, restart: crate::config::Restart) -> Arc<Daemon> {
    let mut config = fake_claude_config(dir);
    config.agents[0].cmd = if cfg!(windows) {
        vec!["cmd".to_string(), "/K".to_string()]
    } else {
        vec!["cat".to_string()]
    };
    config.agents[0].restart = restart;
    Daemon::new(config)
}

/// Shows `pane` at a size, as a client would, and checks the pane holds
/// what its removal has to let go of.
fn shown(d: &Arc<Daemon>, pane: PaneId) {
    d.resize_pane(d.new_viewer(), pane, 30, 100).unwrap();
    assert!(d.holds_pane_size(pane));
}

fn assert_nothing_left_of(d: &Daemon, pane: PaneId) {
    assert!(!d.holds_pane_size(pane), "the sizes asked of a gone pane are forgotten");
}

#[tokio::test]
async fn closing_a_pane_leaves_nothing_of_it_behind() {
    let dir = tempfile::tempdir().unwrap();
    let d = running_claude(dir.path(), Default::default());
    let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();
    shown(&d, pane);

    d.close_pane(pane).unwrap();

    assert_nothing_left_of(&d, pane);
}

#[tokio::test]
async fn closing_a_pane_marks_its_repository_as_used_then() {
    let dir = tempfile::tempdir().unwrap();
    let d = running_claude(dir.path(), Default::default());
    let last_used = |d: &Daemon| d.snapshot()[0].repositories[0].last_used;
    let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();
    assert_eq!(last_used(&d), None, "starting a pane is not what is recorded");

    d.close_pane(pane).unwrap();

    assert!(last_used(&d).is_some_and(|at| at > 0));
}

#[tokio::test]
async fn a_restarted_agent_leaves_nothing_of_its_old_pane_behind() {
    let dir = tempfile::tempdir().unwrap();
    let d = running_claude(dir.path(), crate::config::Restart::Always);
    let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();
    shown(&d, pane);

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
    let d = running_claude(dir.path(), Default::default());
    let pane = d
        .start_agent(only_checkout(&d), "claude", Start::Resuming, None)
        .unwrap();
    shown(&d, pane);

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
    let d = running_claude(dir.path(), Default::default());
    d.reconcile_worktrees_with(|_| vec![dir.path().to_path_buf(), worktree.clone()]);
    let checkout = d.checkout_at(&worktree).expect("the worktree joined the tree");
    let pane = d.spawn_agent(checkout, "claude").unwrap();
    shown(&d, pane);

    d.reconcile_worktrees_with(|_| vec![dir.path().to_path_buf()]);

    assert!(d.checkout_at(&worktree).is_none(), "the worktree left the tree");
    assert_nothing_left_of(&d, pane);
}

#[test]
fn a_claude_pane_is_pointed_at_its_mod_after_any_folder_already_named() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_running_claude(dir.path());
    let claude = crate::harness::Harness::claude();
    assert_eq!(d.plugin_dir_env(&claude, &[]), None, "a daemon that names no root writes no plugin");

    let root = tempfile::tempdir().unwrap();
    d.set_plugin_root(root.path().to_path_buf());
    let theirs = std::env::join_paths(["/opt/their-mods"]).unwrap().into_string().unwrap();
    let (var, value) = d
        .plugin_dir_env(&claude, &[("CLAUDE_CODE_PLUGIN_DIRS".to_string(), theirs)])
        .unwrap();

    assert_eq!(var, "CLAUDE_CODE_PLUGIN_DIRS");
    let mod_dir = root.path().join("claude-mod");
    assert_eq!(
        std::env::split_paths(&value).collect::<Vec<_>>(),
        vec![std::path::PathBuf::from("/opt/their-mods"), mod_dir.clone()]
    );
    assert!(mod_dir.join("hooks/register.ts").is_file());
    assert_eq!(d.plugin_dir_env(&crate::harness::Harness::codex(), &[]), None);
}
