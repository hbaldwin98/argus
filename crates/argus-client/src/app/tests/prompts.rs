//! The modal prompts, and what confirming one asks the daemon for.

use super::*;
// --- prompts -----------------------------------------------------------

#[test]
fn n_in_the_projects_column_opens_the_directory_browser() {
    let mut h = Harness::new();
    h.key(KeyCode::Char('n'));
    assert!(h.app.dir_picker.is_some());
    // The browser asks where to start rather than guessing: only the
    // daemon knows what it can see.
    match &h.sent()[0] {
        ClientMsg::ListDirectories { path, .. } => assert_eq!(path, ""),
        other => panic!("unexpected {other:?}"),
    }

    h.browse("/some", Some("/"), &[("dir", false)]);
    h.keys("dir");
    h.key(KeyCode::Enter);
    match &h.sent()[0] {
        ClientMsg::AddProject { path } => assert_eq!(path, "/some/dir"),
        other => panic!("unexpected {other:?}"),
    }
    assert!(h.app.dir_picker.is_none());
}

#[test]
fn tab_walks_into_a_directory_and_enter_adds_where_you_land() {
    let mut h = Harness::new();
    h.key(KeyCode::Char('n'));
    h.browse("/home", Some("/"), &[("u", false)]);
    h.sent();

    h.keys("u");
    h.key(KeyCode::Tab);
    match &h.sent()[0] {
        ClientMsg::ListDirectories { path, .. } => assert_eq!(path, "/home/u"),
        other => panic!("unexpected {other:?}"),
    }

    h.browse("/home/u", Some("/home"), &[("code", true)]);
    h.key(KeyCode::Enter);
    match &h.sent()[0] {
        ClientMsg::AddProject { path } => {
            assert_eq!(path, "/home/u", "the first row is the directory you are in");
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn a_listing_for_a_directory_already_left_is_dropped() {
    let mut h = Harness::new();
    h.key(KeyCode::Char('n'));
    h.browse("/home", Some("/"), &[("u", false)]);
    h.sent();
    h.keys("u");
    h.key(KeyCode::Tab);
    h.sent();

    h.app.on_server_msg(ServerMsg::Directories(DirListing {
        request_id: 999,
        path: "/stale".to_string(),
        parent: None,
        entries: Vec::new(),
        error: None,
    }));
    assert_eq!(h.app.dir_picker.as_ref().unwrap().path, "/home");
}

#[test]
fn a_new_project_becomes_the_selected_one() {
    let mut h = Harness::new();
    h.key(KeyCode::Char('n'));
    h.browse("/d", Some("/"), &[]);
    h.key(KeyCode::Enter);
    h.sent();

    let mut t = tree();
    t.push(ProjectInfo {
        id: ProjectId(3),
        name: "new".to_string(),
        repositories: vec![repository(
            7,
            "new-repo",
            vec![checkout(30, "new", true, vec![])],
        )],
        notes: Default::default(),
        has_note: false,
    });
    h.app.on_server_msg(ServerMsg::Tree(t));
    assert_eq!(h.app.current_project().unwrap().name, "new");
}

#[test]
fn n_in_the_repositories_column_adds_a_repository_to_that_project() {
    let mut h = Harness::new();
    h.keys("l");
    h.sent();
    assert_eq!(h.app.focus, Focus::Repositories);

    h.key(KeyCode::Char('n'));
    assert!(h.app.dir_picker.is_some());

    h.browse("/some", Some("/"), &[("repo", true)]);
    h.sent();
    h.keys("repo");
    h.key(KeyCode::Enter);
    match &h.sent()[0] {
        ClientMsg::AddRepository { project, path } => {
            assert_eq!(*project, ProjectId(1), "the project in view");
            assert_eq!(path, "/some/repo");
        }
        other => panic!("unexpected {other:?}"),
    }
    assert!(h.app.dir_picker.is_none());
}

#[test]
fn a_new_repository_becomes_the_selected_one() {
    let mut h = Harness::new();
    h.keys("l");
    h.key(KeyCode::Char('n'));
    h.browse("/r", Some("/"), &[]);
    h.key(KeyCode::Enter);
    h.sent();

    let mut t = tree();
    t[0].repositories.push(repository(
        7,
        "added",
        vec![checkout(30, "main", true, vec![])],
    ));
    h.app.on_server_msg(ServerMsg::Tree(t));
    assert_eq!(h.app.current_repository().unwrap().name, "added");
}

#[test]
fn esc_cancels_adding_a_repository() {
    let mut h = Harness::new();
    h.keys("l");
    h.sent();
    h.key(KeyCode::Char('n'));
    h.browse("/some", Some("/"), &[("repo", true)]);
    h.sent();
    h.keys("re");
    h.key(KeyCode::Esc);
    assert!(h.app.dir_picker.is_none());
    assert!(h.sent().is_empty());
}

#[test]
fn i_in_the_repositories_column_makes_a_repository_that_is_not_there_yet() {
    let mut h = Harness::new();
    h.keys("l");
    h.sent();

    h.key(KeyCode::Char('i'));
    h.browse("/src", Some("/"), &[("existing", true)]);
    h.sent();
    // The directory is where it goes, not what it is — so the browse
    // is followed by a name.
    h.key(KeyCode::Enter);
    assert!(h.app.dir_picker.is_none());
    assert!(matches!(h.app.prompt, Some(Prompt::NewRepository { .. })));

    h.keys("thing");
    h.key(KeyCode::Enter);
    match &h.sent()[0] {
        ClientMsg::InitRepository { project, path } => {
            assert_eq!(*project, ProjectId(1), "the project in view");
            assert_eq!(path, "/src/thing");
        }
        other => panic!("unexpected {other:?}"),
    }
    assert!(h.app.prompt.is_none());
}

#[test]
fn a_new_repository_with_no_name_is_the_directory_that_was_chosen() {
    let mut h = Harness::new();
    h.keys("l");
    h.key(KeyCode::Char('i'));
    h.browse("/src/already-made", Some("/src"), &[]);
    h.sent();
    h.key(KeyCode::Enter);
    h.key(KeyCode::Enter);
    match &h.sent()[0] {
        ClientMsg::InitRepository { path, .. } => assert_eq!(path, "/src/already-made"),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn esc_cancels_making_a_repository_at_either_step() {
    let mut h = Harness::new();
    h.keys("l");
    h.key(KeyCode::Char('i'));
    h.browse("/src", Some("/"), &[]);
    h.sent();
    h.key(KeyCode::Esc);
    assert!(h.app.dir_picker.is_none());
    assert!(h.sent().is_empty());

    h.key(KeyCode::Char('i'));
    h.browse("/src", Some("/"), &[]);
    h.sent();
    h.key(KeyCode::Enter);
    h.keys("half-typed");
    h.key(KeyCode::Esc);
    assert!(h.app.prompt.is_none());
    assert!(h.sent().is_empty(), "nothing was created");
}

#[test]
fn i_does_nothing_outside_the_repositories_column() {
    let mut h = Harness::new();
    assert_eq!(h.app.focus, Focus::Projects);
    h.key(KeyCode::Char('i'));
    assert!(h.app.dir_picker.is_none());

    h.keys("ll");
    h.sent();
    assert_eq!(h.app.focus, Focus::Checkouts);
    h.key(KeyCode::Char('i'));
    assert!(h.app.dir_picker.is_none());
}

#[test]
fn a_repository_just_made_becomes_the_selected_one() {
    let mut h = Harness::new();
    h.keys("l");
    h.key(KeyCode::Char('i'));
    h.browse("/src", Some("/"), &[]);
    h.key(KeyCode::Enter);
    h.keys("thing");
    h.key(KeyCode::Enter);
    h.sent();

    let mut t = tree();
    t[0].repositories.push(repository(
        7,
        "thing",
        vec![checkout(30, "main", true, vec![])],
    ));
    h.app.on_server_msg(ServerMsg::Tree(t));
    assert_eq!(h.app.current_repository().unwrap().name, "thing");
}

#[test]
fn n_in_the_checkouts_column_prompts_for_a_branch() {
    let mut h = Harness::new();
    h.keys("ll");
    h.sent();
    h.key(KeyCode::Char('n'));
    assert!(matches!(h.app.prompt, Some(Prompt::NewWorktree { .. })));

    h.keys("feat/x");
    h.key(KeyCode::Enter);
    match &h.sent()[0] {
        ClientMsg::CreateWorktree { checkout, branch } => {
            assert_eq!(
                *checkout,
                CheckoutId(10),
                "branched off the selected checkout"
            );
            assert_eq!(branch, "feat/x");
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn a_worktree_prompt_can_be_edited_and_cancelled() {
    let mut h = Harness::new();
    h.keys("lln");
    h.key(KeyCode::Up);
    h.keys("draft");
    h.key(KeyCode::Backspace);

    match &h.app.prompt {
        Some(Prompt::NewWorktree { input, .. }) => assert_eq!(input, "draf"),
        _ => panic!("expected a worktree prompt"),
    }

    h.key(KeyCode::Esc);
    assert!(h.app.prompt.is_none());
    assert!(h.sent().is_empty(), "cancelling must not create a worktree");
}

#[test]
fn a_new_worktree_becomes_the_selected_checkout() {
    let mut h = Harness::new();
    h.keys("lln");
    h.keys("x");
    h.key(KeyCode::Enter);
    h.sent();

    let mut t = tree();
    t[0].repositories[0]
        .checkouts
        .push(checkout(12, "x", false, vec![]));
    h.app.on_server_msg(ServerMsg::Tree(t));
    assert_eq!(h.app.current_checkout().unwrap().name, "x");
}

#[test]
fn a_pending_new_worktree_restores_the_columns_before_moving_selection() {
    let mut h = Harness::new();
    h.keys("llll");
    h.leader();
    h.key(KeyCode::Char('f'));
    h.app.pending_focus_new_checkout = Some(RepositoryId(5));
    let mut t = tree();
    t[0].repositories[0]
        .checkouts
        .push(checkout(12, "x", false, vec![]));

    h.app.on_server_msg(ServerMsg::Tree(t));

    assert_eq!(h.app.current_checkout().unwrap().name, "x");
    assert_eq!(h.app.focus, Focus::Panes);
    assert!(!h.app.pane_fullscreen);
}

#[test]
fn a_new_worktree_selects_its_repository_even_if_navigation_moved() {
    let mut h = Harness::new();
    h.keys("lln");
    h.keys("x");
    h.key(KeyCode::Enter);
    h.sent();

    let mut t = tree();
    t[0].repositories.push(repository(
        7,
        "satellite",
        vec![checkout(30, "main", true, vec![])],
    ));
    t[0].repositories[0]
        .checkouts
        .push(checkout(12, "x", false, vec![]));
    h.app.sel_repository = 1;
    h.app.on_server_msg(ServerMsg::Tree(t));

    assert_eq!(h.app.current_repository().unwrap().id, RepositoryId(5));
    assert_eq!(h.app.current_checkout().unwrap().name, "x");
}

#[test]
fn checkout_commands_use_the_repository_selections_current_checkout() {
    let mut h = Harness::new();
    h.key(KeyCode::Char('l'));
    assert_eq!(h.app.focus, Focus::Repositories);
    h.sent();

    h.key(KeyCode::Char('s'));

    assert!(matches!(
        h.sent().as_slice(),
        [ClientMsg::SpawnShell {
            checkout: CheckoutId(10)
        }]
    ));
}

#[test]
fn n_does_nothing_in_the_pane_columns() {
    let mut h = Harness::new();
    h.keys("lll");
    h.sent();
    h.key(KeyCode::Char('n'));
    assert!(h.app.prompt.is_none(), "no checkout context to branch from");
    assert!(h.app.dir_picker.is_none());
}

#[test]
fn an_empty_prompt_sends_nothing() {
    let mut h = Harness::new();
    h.keys("ll");
    h.sent();
    h.key(KeyCode::Char('n'));
    h.keys("   ");
    h.key(KeyCode::Enter);
    assert!(h.app.prompt.is_none());
    assert!(h.sent().is_empty(), "whitespace is not a branch name");
}

#[test]
fn esc_cancels_a_prompt_and_backspace_edits_it() {
    let mut h = Harness::new();
    h.keys("ll");
    h.sent();
    h.key(KeyCode::Char('n'));
    h.keys("abc");
    h.key(KeyCode::Backspace);
    match &h.app.prompt {
        Some(Prompt::NewWorktree { input, .. }) => assert_eq!(input, "ab"),
        _ => panic!("expected the new-worktree prompt to still be open"),
    }
    h.key(KeyCode::Esc);
    assert!(h.app.prompt.is_none());
    assert!(h.sent().is_empty());
}

#[test]
fn the_directory_browser_swallows_navigation_keys() {
    let mut h = Harness::new();
    h.key(KeyCode::Char('n'));
    h.browse("/home", Some("/"), &[("u", false)]);
    h.keys("jl");
    assert_eq!(h.app.sel_project, 0, "j typed into the query, not a move");
    assert_eq!(h.app.focus, Focus::Projects);
    assert_eq!(h.app.dir_picker.as_ref().unwrap().query, "jl");
}

#[test]
fn a_pasted_path_goes_into_the_browser_and_not_a_pane() {
    let mut h = Harness::new();
    h.key(KeyCode::Char('n'));
    h.browse("/home", Some("/"), &[]);
    h.sent();
    h.app.on_paste("/var/src".to_string());
    h.key(KeyCode::Tab);
    match &h.sent()[0] {
        ClientMsg::ListDirectories { path, .. } => assert_eq!(path, "/var/src"),
        other => panic!("unexpected {other:?}"),
    }
}

// --- removing a checkout ------------------------------------------------

#[test]
fn the_primary_checkout_cannot_be_removed() {
    let mut h = Harness::new();
    h.keys("ll");
    h.sent();
    h.key(KeyCode::Char('D'));
    assert!(h.app.prompt.is_none(), "no confirmation is even offered");
    assert!(h.sent().is_empty(), "and nothing is sent to the daemon");
    assert!(h.app.status.contains("primary"));
}

#[test]
fn removing_a_linked_worktree_asks_first_then_sends() {
    let mut h = Harness::new();
    h.keys("llj");
    h.sent();
    h.key(KeyCode::Char('D'));
    match &h.app.prompt {
        Some(Prompt::ConfirmRemove { target, label }) => {
            assert_eq!(*target, RemoveTarget::Checkout(CheckoutId(11)));
            assert_eq!(label, "feat", "the confirmation names what it will delete");
        }
        _ => panic!("expected a confirmation prompt"),
    }
    assert!(h.sent().is_empty(), "nothing sent before confirming");

    h.key(KeyCode::Char('y'));
    assert!(matches!(
        h.sent()[0],
        ClientMsg::RemoveCheckout {
            checkout: CheckoutId(11)
        }
    ));
    assert!(h.app.prompt.is_none());
}

#[test]
fn removing_a_project_asks_first_then_sends() {
    let mut h = Harness::new();
    h.key(KeyCode::Char('D'));
    match &h.app.prompt {
        Some(Prompt::ConfirmRemove { target, label }) => {
            assert_eq!(*target, RemoveTarget::Project(ProjectId(1)));
            assert_eq!(label, "argus");
        }
        other => panic!("expected a project confirmation, got {:?}", other.is_some()),
    }
    h.key(KeyCode::Char('y'));
    assert!(matches!(
        h.sent()[0],
        ClientMsg::RemoveProject {
            project: ProjectId(1)
        }
    ));
}

#[test]
fn removing_a_repository_asks_first_then_sends() {
    let mut h = Harness::new();
    h.keys("l");
    h.sent();
    h.key(KeyCode::Char('D'));
    match &h.app.prompt {
        Some(Prompt::ConfirmRemove { target, label }) => {
            assert_eq!(*target, RemoveTarget::Repository(RepositoryId(5)));
            assert_eq!(label, "orion");
        }
        other => panic!(
            "expected a repository confirmation, got {:?}",
            other.is_some()
        ),
    }
    h.key(KeyCode::Char('y'));
    assert!(matches!(
        h.sent()[0],
        ClientMsg::RemoveRepository {
            repository: RepositoryId(5)
        }
    ));
}

#[test]
fn a_removal_confirmation_says_whether_files_are_going_away() {
    // The whole point of the project/repository removals is that they
    // are not deletions, and the popup is the only place that says so.
    for target in [
        RemoveTarget::Project(ProjectId(1)),
        RemoveTarget::Repository(RepositoryId(5)),
    ] {
        assert!(target.wording().1.contains("files stay"));
    }
    assert!(RemoveTarget::Checkout(CheckoutId(11))
        .wording()
        .1
        .contains("worktree"));
}

#[test]
fn declining_the_removal_sends_nothing() {
    for decline in ['n', 'q'] {
        let mut h = Harness::new();
        h.keys("llj");
        h.sent();
        h.key(KeyCode::Char('D'));
        h.key(KeyCode::Char(decline));
        if decline == 'n' {
            assert!(h.app.prompt.is_none(), "n declines");
        }
        assert!(h.sent().is_empty(), "{decline} must not delete anything");
    }
}

#[test]
fn d_does_nothing_in_the_pane_columns() {
    // `D` follows focus through the three tree columns; a pane is
    // closed with `x` instead, and has nothing to remove.
    let mut h = Harness::new();
    h.keys("lll");
    h.sent();
    h.key(KeyCode::Char('D'));
    assert!(
        h.app.prompt.is_none(),
        "no removal offered from the panes column"
    );
    assert!(h.sent().is_empty());
}
