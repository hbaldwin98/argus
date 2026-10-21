//! A person moving a pane to another checkout: the process goes with the
//! row, or the move is refused and the row stays.

use super::*;
use crate::pty::{Shell, ShellState};

/// A scripted daemon over `dir` with a linked worktree `dir/wt` in its
/// tree, and that worktree's checkout.
fn with_a_worktree(dir: &std::path::Path) -> (ScriptedDaemon, PathBuf, CheckoutId) {
    let worktree = dir.join("wt");
    std::fs::create_dir(&worktree).unwrap();
    let d = ScriptedDaemon::new(fake_claude_config(dir));
    let (primary, listed) = (dir.to_path_buf(), worktree.clone());
    d.git.lists_worktrees(move |_| vec![primary.clone(), listed.clone()]);
    d.reconcile_worktrees();
    let checkout = d.checkout_at(&worktree).expect("the worktree joined the tree");
    (d, worktree, checkout)
}

fn checkout_of(d: &Daemon, pane: PaneId) -> Option<CheckoutId> {
    d.snapshot()[0].repositories[0]
        .checkouts
        .iter()
        .find(|c| c.panes.iter().any(|p| p.id == pane))
        .map(|c| c.id)
}

#[tokio::test]
async fn a_shell_at_its_prompt_is_sent_a_cd_and_its_row_moves_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let (d, worktree, checkout) = with_a_worktree(dir.path());
    let pane = d.spawn_shell(only_checkout(&d)).unwrap();

    d.move_to_checkout(pane, checkout).unwrap();

    assert_eq!(d.processes.written(pane), Shell::Posix.cd(&worktree).unwrap());
    assert_eq!(checkout_of(&d, pane), Some(checkout));
    close_all(&d);
}

#[tokio::test]
async fn a_shell_running_something_is_refused_and_typed_nothing() {
    // A `cd` typed now would be input to whatever runs in the foreground.
    let dir = tempfile::tempdir().unwrap();
    let (d, _worktree, checkout) = with_a_worktree(dir.path());
    let home = only_checkout(&d);
    let pane = d.spawn_shell(home).unwrap();
    d.processes.set_shell(pane, Some(ShellState::Running));

    let refused = d.move_to_checkout(pane, checkout).unwrap_err();

    assert!(refused.to_string().contains("back at its prompt"), "{refused}");
    assert!(d.processes.written(pane).is_empty());
    assert_eq!(checkout_of(&d, pane), Some(home));
    close_all(&d);
}

#[tokio::test]
async fn a_shell_whose_syntax_argus_does_not_know_is_refused_by_name() {
    let dir = tempfile::tempdir().unwrap();
    let (d, _worktree, checkout) = with_a_worktree(dir.path());
    let home = only_checkout(&d);
    let pane = d.spawn_shell(home).unwrap();
    d.processes.set_shell(pane, Some(ShellState::Unknown("nu".into())));

    let refused = d.move_to_checkout(pane, checkout).unwrap_err();

    assert!(refused.to_string().contains("nu"), "{refused}");
    assert!(d.processes.written(pane).is_empty());
    assert_eq!(checkout_of(&d, pane), Some(home));
    close_all(&d);
}

#[tokio::test]
async fn an_exited_shell_stays_where_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let (d, _worktree, checkout) = with_a_worktree(dir.path());
    let home = only_checkout(&d);
    let pane = d.spawn_shell(home).unwrap();
    d.exit(pane, Some(0));

    assert!(d.move_to_checkout(pane, checkout).is_err());
    assert_eq!(checkout_of(&d, pane), Some(home));
    close_all(&d);
}

#[tokio::test]
async fn an_agent_moved_by_a_person_is_told_where_it_works_now() {
    // Its row follows where its turns end, so without being told it would
    // go on working in the old checkout under the new row.
    let dir = tempfile::tempdir().unwrap();
    let (d, worktree, checkout) = with_a_worktree(dir.path());
    let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();

    d.move_to_checkout(pane, checkout).unwrap();

    let told = String::from_utf8(d.processes.written(pane)).unwrap();
    assert!(told.contains(&worktree.display().to_string()), "{told:?}");
    assert!(told.contains("checkout"), "{told:?}");
    assert_eq!(checkout_of(&d, pane), Some(checkout));
    close_all(&d);
}

#[tokio::test]
async fn a_pane_dropped_on_its_own_checkout_is_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let (d, _worktree, _checkout) = with_a_worktree(dir.path());
    let home = only_checkout(&d);
    let agent = d.spawn_agent(home, "claude").unwrap();
    let shell = d.spawn_shell(home).unwrap();

    d.move_to_checkout(agent, home).unwrap();
    d.move_to_checkout(shell, home).unwrap();

    assert!(d.processes.written(agent).is_empty());
    assert!(d.processes.written(shell).is_empty());
    close_all(&d);
}
