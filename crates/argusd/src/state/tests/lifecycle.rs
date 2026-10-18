//! What a pane leaves behind when it leaves the tree: nothing, whichever
//! way it went — closed, restarted, replaced after a failed resume, or
//! exited and then taken with a worktree deleted underneath it.

use super::*;

/// A daemon whose claude panes keep running until something ends them.
fn running_claude(dir: &std::path::Path, restart: crate::config::Restart) -> ScriptedDaemon {
    let mut config = fake_claude_config(dir);
    config.agents[0].restart = restart;
    ScriptedDaemon::new(config)
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

/// One tree per change, the owner's call of 2026-10-18: the exit a kill
/// brings about lands on a tree the pane already left, so it changes
/// nothing a client is shown and is not pushed as a second, identical tree.
#[tokio::test]
async fn a_closed_panes_exit_pushes_no_second_tree() {
    let dir = tempfile::tempdir().unwrap();
    let d = running_claude(dir.path(), Default::default());
    let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();
    d.close_pane(pane).unwrap();
    let mut trees = d.subscribe_tree();

    d.exit(pane, None);

    assert!(trees.try_recv().is_err(), "the exit of a pane no tree holds pushes nothing");
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

    d.exit(pane, Some(0));

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
        .start_agent(only_checkout(&d), "claude", Start::Resuming, None, None)
        .unwrap();
    shown(&d, pane);

    d.exit(pane, Some(1));

    let panes = panes_of(&d);
    assert_eq!(panes.len(), 1);
    assert_ne!(panes[0].id, pane, "a fresh agent took its place");
    assert_nothing_left_of(&d, pane);
    close_all(&d);
}

/// Only an exited pane goes with its worktree; a live one moves to the
/// primary checkout instead, which `reconcile` covers.
#[tokio::test]
async fn a_deleted_worktree_leaves_nothing_of_its_exited_panes_behind() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = dir.path().join("wt");
    std::fs::create_dir(&worktree).unwrap();
    let d = running_claude(dir.path(), Default::default());
    let (primary, listed) = (dir.path().to_path_buf(), worktree.clone());
    d.git.lists_worktrees(move |_| vec![primary.clone(), listed.clone()]);
    d.reconcile_worktrees();
    let checkout = d.checkout_at(&worktree).expect("the worktree joined the tree");
    let pane = d.spawn_agent(checkout, "claude").unwrap();
    shown(&d, pane);
    d.exit(pane, Some(0));

    let primary = dir.path().to_path_buf();
    d.git.lists_worktrees(move |_| vec![primary.clone()]);
    d.reconcile_worktrees();

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

#[tokio::test]
async fn a_claude_pane_gets_one_plugin_folder_list_and_keeps_the_rest_of_its_environment() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = fake_claude_config(dir.path());
    config.agents[0].env.insert("CLAUDE_CODE_PLUGIN_DIRS".to_string(), "/opt/their-mods".to_string());
    let d = ScriptedDaemon::new(config);
    let root = tempfile::tempdir().unwrap();
    d.set_plugin_root(root.path().to_path_buf());

    let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();

    let env = d.processes.started(pane).unwrap().env;
    let named = |key: &str| env.iter().filter(|(k, _)| k == key).count();
    assert_eq!(named("CLAUDE_CODE_PLUGIN_DIRS"), 1, "the template's list is replaced, not joined: {env:?}");
    assert_eq!(named(argus_protocol::INSTRUCTIONS_VAR), 1, "and nothing else goes with it");
    close_all(&d);
}
