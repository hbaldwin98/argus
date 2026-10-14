//! Which checkout an agent pane belongs to: following its agent into the
//! worktree a turn ends in, and the move an agent asks for.

use super::*;

/// A running agent in `primary`, and `primary/.argus/worktrees/feat` on
/// disk, which Argus has not discovered yet.
fn agent_beside_an_undiscovered_worktree(
    primary: &std::path::Path,
) -> (Arc<Daemon>, PaneId, PathBuf) {
    let worktree = primary.join(".argus").join("worktrees").join("feat");
    std::fs::create_dir_all(worktree.join("src")).unwrap();
    let d = daemon_with_running_claude(primary);
    let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();
    (d, pane, worktree)
}

fn both(primary: &std::path::Path, worktree: &std::path::Path) -> impl Fn(&std::path::Path) -> Vec<PathBuf> {
    let listed = vec![primary.to_path_buf(), worktree.to_path_buf()];
    move |_| listed.clone()
}

fn checkout_of(d: &Daemon, pane: PaneId) -> String {
    d.snapshot()[0].repositories[0]
        .checkouts
        .iter()
        .find(|c| c.panes.iter().any(|p| p.id == pane))
        .map(|c| c.path.clone())
        .expect("the pane is in a checkout")
}

#[tokio::test]
async fn a_turn_ending_in_a_new_worktree_moves_the_pane_there() {
    let primary = tempfile::tempdir().unwrap();
    let (d, pane, worktree) = agent_beside_an_undiscovered_worktree(primary.path());

    d.follow_agent_directory_with(pane, None, &worktree.join("src"), both(primary.path(), &worktree));

    assert_eq!(checkout_of(&d, pane), worktree.to_string_lossy());
    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn a_turn_ending_in_its_own_checkout_leaves_the_pane_where_it_is() {
    let primary = tempfile::tempdir().unwrap();
    let (d, pane, worktree) = agent_beside_an_undiscovered_worktree(primary.path());

    // The worktree sits inside the primary checkout; only a directory inside
    // the worktree itself is in it.
    d.follow_agent_directory_with(pane, None, primary.path(), both(primary.path(), &worktree));

    assert_eq!(checkout_of(&d, pane), primary.path().to_string_lossy());
    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn a_pane_moved_by_hand_is_not_pulled_back_by_a_directory_already_followed() {
    let primary = tempfile::tempdir().unwrap();
    let (d, pane, worktree) = agent_beside_an_undiscovered_worktree(primary.path());
    d.follow_agent_directory_with(pane, None, &worktree, both(primary.path(), &worktree));

    d.move_agent_to_checkout(pane, primary.path()).unwrap();
    d.follow_agent_directory_with(pane, None, &worktree, both(primary.path(), &worktree));

    assert_eq!(checkout_of(&d, pane), primary.path().to_string_lossy());
    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn a_cli_started_inside_the_pane_cannot_move_it() {
    let primary = tempfile::tempdir().unwrap();
    let (d, pane, worktree) = agent_beside_an_undiscovered_worktree(primary.path());
    d.set_pane_session_id(pane, "parent-session");

    d.follow_agent_directory_with(
        pane,
        Some("child-session"),
        &worktree,
        both(primary.path(), &worktree),
    );

    assert_eq!(checkout_of(&d, pane), primary.path().to_string_lossy());
    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn a_turn_ending_in_another_repository_does_not_move_the_pane() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let d = daemon_with_two_agent_checkouts(first.path(), second.path());
    let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();

    d.follow_agent_directory_with(pane, None, second.path(), |_| Vec::new());

    assert_eq!(d.snapshot()[0].repositories[0].checkouts[0].panes[0].id, pane);
    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn an_agent_asking_to_move_into_a_worktree_just_made_is_moved() {
    let primary = tempfile::tempdir().unwrap();
    let (d, pane, worktree) = agent_beside_an_undiscovered_worktree(primary.path());

    d.move_agent_into_with(pane, &worktree.join("src"), both(primary.path(), &worktree))
        .unwrap();

    assert_eq!(checkout_of(&d, pane), worktree.to_string_lossy());
    d.close_pane(pane).unwrap();
}

#[tokio::test]
async fn an_agent_asking_to_move_outside_its_project_is_told_why_not() {
    let primary = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let (d, pane, worktree) = agent_beside_an_undiscovered_worktree(primary.path());

    let refusal = d
        .move_agent_into_with(pane, elsewhere.path(), both(primary.path(), &worktree))
        .unwrap_err();

    assert!(refusal.to_string().contains("not inside a checkout"), "{refusal}");
    assert_eq!(checkout_of(&d, pane), primary.path().to_string_lossy());
    d.close_pane(pane).unwrap();
}
