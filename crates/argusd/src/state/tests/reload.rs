//! Re-reading `projects.toml` under a running daemon.

use super::*;
// --- reloading the config -----------------------------------------------

#[test]
fn a_project_added_to_the_file_arrives_on_reload() {
    with_temp_config(|dir| {
        std::fs::write(
            dir.join("projects.toml"),
            "[[project]]\nname = \"one\"\nrepos = [\"/one\"]\n",
        )
        .unwrap();
        let d = Daemon::new(config::load().unwrap());
        assert_eq!(d.snapshot().len(), 1);

        std::fs::write(
            dir.join("projects.toml"),
            "[[project]]\nname = \"one\"\nrepos = [\"/one\"]\n\n[[project]]\nname = \"two\"\nrepos = [\"/two\"]\n",
        )
        .unwrap();
        d.reload_config().unwrap();

        let names: Vec<String> = d.snapshot().into_iter().map(|p| p.name).collect();
        assert_eq!(names, vec!["one".to_string(), "two".to_string()]);
    });
}

#[tokio::test]
async fn reloading_keeps_the_panes_and_ids_of_everything_still_configured() {
    let repo = tempfile::tempdir().unwrap();
    let repo_path = repo.path().to_string_lossy().replace('\\', "/");
    with_temp_config(|dir| {
        std::fs::write(
            dir.join("projects.toml"),
            format!("[[project]]\nname = \"one\"\nrepos = [\"{repo_path}\"]\n"),
        )
        .unwrap();
        let d = Daemon::new(config::load().unwrap());
        let checkout = only_checkout(&d);
        let pane = d.spawn_shell(checkout).unwrap();

        std::fs::write(
            dir.join("projects.toml"),
            format!(
                "[[project]]\nname = \"one\"\nrepos = [\"{repo_path}\"]\nexclusive = true\n"
            ),
        )
        .unwrap();
        d.reload_config().unwrap();

        let snapshot = d.snapshot();
        assert_eq!(
            snapshot[0].repositories[0].checkouts[0].id, checkout,
            "the same checkout, not a rebuilt one"
        );
        assert_eq!(
            snapshot[0].repositories[0].checkouts[0].panes.len(),
            1,
            "the shell kept running"
        );
        let _ = d.close_pane(pane);
    });
}

#[test]
fn a_repository_the_file_stopped_naming_leaves_only_when_it_is_empty() {
    let repo = tempfile::tempdir().unwrap();
    let repo_path = repo.path().to_string_lossy().replace('\\', "/");
    with_temp_config(|dir| {
        std::fs::write(
            dir.join("projects.toml"),
            format!("[[project]]\nname = \"one\"\nrepos = [\"{repo_path}\", \"/second\"]\n"),
        )
        .unwrap();
        let d = Daemon::new(config::load().unwrap());
        assert_eq!(d.snapshot()[0].repositories.len(), 2);

        std::fs::write(
            dir.join("projects.toml"),
            format!("[[project]]\nname = \"one\"\nrepos = [\"{repo_path}\"]\n"),
        )
        .unwrap();
        d.reload_config().unwrap();

        assert_eq!(
            d.snapshot()[0].repositories.len(),
            1,
            "the repository with nothing running in it goes"
        );
    });
}

#[tokio::test]
async fn a_project_removed_from_the_file_stays_while_an_agent_is_working_in_it() {
    // The config file does not get to end somebody's work in progress.
    let repo = tempfile::tempdir().unwrap();
    let repo_path = repo.path().to_string_lossy().replace('\\', "/");
    with_temp_config(|dir| {
        std::fs::write(
            dir.join("projects.toml"),
            format!("[[project]]\nname = \"one\"\nrepos = [\"{repo_path}\"]\n"),
        )
        .unwrap();
        let d = Daemon::new(config::load().unwrap());
        let pane = d.spawn_shell(only_checkout(&d)).unwrap();

        std::fs::write(dir.join("projects.toml"), "").unwrap();
        d.reload_config().unwrap();

        assert_eq!(d.snapshot().len(), 1, "still there, still running");

        d.close_pane(pane).unwrap();
        d.reload_config().unwrap();
        assert!(d.snapshot().is_empty(), "and gone once it is empty");
    });
}

#[test]
fn reloading_replaces_the_agent_templates() {
    with_temp_config(|dir| {
        std::fs::write(
            dir.join("projects.toml"),
            "[[agent]]\nname = \"old\"\ncmd = [\"x\"]\n",
        )
        .unwrap();
        let d = Daemon::new(config::load().unwrap());
        assert_eq!(d.template_names(), vec!["old".to_string()]);

        std::fs::write(
            dir.join("projects.toml"),
            "[[agent]]\nname = \"new\"\ncmd = [\"y\"]\n",
        )
        .unwrap();
        d.reload_config().unwrap();

        assert_eq!(d.template_names(), vec!["new".to_string()]);
    });
}

#[tokio::test]
async fn a_project_that_becomes_exclusive_starts_refusing_a_second_agent() {
    let repo = tempfile::tempdir().unwrap();
    let repo_path = repo.path().to_string_lossy().replace('\\', "/");
    with_temp_config(|dir| {
        let agent = "[[agent]]\nname = \"claude\"\ncmd = [\"echo\", \"hi\"]\n";
        std::fs::write(
            dir.join("projects.toml"),
            format!("[[project]]\nname = \"one\"\nrepos = [\"{repo_path}\"]\n{agent}"),
        )
        .unwrap();
        let d = Daemon::new(config::load().unwrap());
        let checkout = only_checkout(&d);
        let first = d.spawn_agent(checkout, "claude").unwrap();

        std::fs::write(
            dir.join("projects.toml"),
            format!(
                "[[project]]\nname = \"one\"\nrepos = [\"{repo_path}\"]\nexclusive = true\n{agent}"
            ),
        )
        .unwrap();
        d.reload_config().unwrap();

        assert!(
            d.spawn_agent(checkout, "claude").is_err(),
            "the setting applies to the checkout that was already there"
        );
        let _ = d.close_pane(first);
    });
}

#[test]
fn a_config_that_never_heard_of_workspaces_still_works() {
    // Every existing projects.toml has no workspace keys at all; those
    // projects must land somewhere visible, not vanish.
    with_temp_config(|_| {
        let d = daemon_with_primary("/repo");
        assert_eq!(d.workspaces().len(), 1, "just the built-in default");
        assert!(d.workspaces()[0].open);
        assert_eq!(d.workspaces()[0].name, crate::config::DEFAULT_WORKSPACE);
        assert_eq!(names_of(&d).len(), 1, "and its project is visible");
    });
}

#[test]
fn workspaces_come_from_declarations_and_from_project_references() {
    with_temp_config(|_| {
        let d = persistent(config_with_workspaces());
        let names: Vec<String> = d.workspaces().into_iter().map(|w| w.name).collect();
        assert_eq!(
            names,
            vec!["default", "work", "weekend"],
            "declared and implied alike, in config order"
        );
    });
}

#[test]
fn the_tree_is_scoped_to_the_open_workspace() {
    with_temp_config(|_| {
        let d = persistent(config_with_workspaces());
        assert_eq!(
            names_of(&d),
            vec!["home-thing"],
            "only the default workspace"
        );

        d.open_workspace(workspace_named(&d, "work")).unwrap();
        assert_eq!(names_of(&d), vec!["day-job"]);

        d.open_workspace(workspace_named(&d, "weekend")).unwrap();
        assert_eq!(names_of(&d), vec!["side"]);
    });
}

#[test]
fn switching_workspace_pushes_a_new_tree_and_workspace_list() {
    with_temp_config(|_| {
        let d = persistent(config_with_workspaces());
        let mut tree_rx = d.subscribe_tree();
        let mut ws_rx = d.subscribe_workspaces();

        d.open_workspace(workspace_named(&d, "work")).unwrap();

        let tree = tree_rx.try_recv().expect("clients need the re-scoped tree");
        assert_eq!(tree[0].name, "day-job");
        let ws = ws_rx.try_recv().expect("and the new open flag");
        assert!(ws.iter().find(|w| w.name == "work").unwrap().open);
        assert!(!ws.iter().find(|w| w.name == "default").unwrap().open);
    });
}

#[test]
fn exactly_one_workspace_is_open_at_a_time() {
    with_temp_config(|_| {
        let d = persistent(config_with_workspaces());
        d.open_workspace(workspace_named(&d, "work")).unwrap();
        assert_eq!(d.workspaces().iter().filter(|w| w.open).count(), 1);
    });
}

#[test]
fn reopening_the_already_open_workspace_changes_nothing() {
    with_temp_config(|_| {
        let d = persistent(config_with_workspaces());
        let mut tree_rx = d.subscribe_tree();
        let open = d.workspaces().into_iter().find(|w| w.open).unwrap().id;

        d.open_workspace(open).unwrap();
        assert!(
            tree_rx.try_recv().is_err(),
            "a no-op switch must not churn every client's tree"
        );
    });
}

#[test]
fn switching_to_a_workspace_that_does_not_exist_is_an_error() {
    with_temp_config(|_| {
        let d = persistent(config_with_workspaces());
        assert!(d.open_workspace(WorkspaceId(9999)).is_err());
    });
}

#[test]
fn the_open_workspace_is_remembered_for_the_next_daemon() {
    with_temp_config(|_| {
        let d = persistent(config_with_workspaces());
        d.open_workspace(workspace_named(&d, "work")).unwrap();
        drop(d);

        let next = persistent(config_with_workspaces());
        assert_eq!(
            next.workspaces().into_iter().find(|w| w.open).unwrap().name,
            "work",
            "restarting should land you back where you were"
        );
    });
}

#[test]
fn a_remembered_workspace_that_no_longer_exists_falls_back_to_default() {
    with_temp_config(|_| {
        let d = persistent(config_with_workspaces());
        d.open_workspace(workspace_named(&d, "weekend")).unwrap();
        drop(d);

        // The user deletes that workspace's project from their config.
        let mut cfg = config_with_workspaces();
        cfg.projects
            .retain(|p| p.workspace.as_deref() != Some("weekend"));
        let next = Daemon::new(cfg);
        assert_eq!(
            next.workspaces().into_iter().find(|w| w.open).unwrap().name,
            crate::config::DEFAULT_WORKSPACE,
            "a dangling name must not leave every client staring at nothing"
        );
    });
}

#[test]
fn workspace_rollups_count_projects_and_panes_across_the_whole_workspace() {
    with_temp_config(|_| {
        let d = persistent(config_with_workspaces());
        let ws = d.workspaces();
        let default = ws.iter().find(|w| w.name == "default").unwrap();
        assert_eq!(default.projects, 1);
        assert_eq!(default.panes, 0);
    });
}

#[tokio::test]
async fn panes_in_a_closed_workspace_keep_running_and_stay_counted() {
    // The whole point of scoping rather than unloading: an agent in a
    // workspace you are not looking at is still working, and you should
    // still be able to see that it is.
    let dir = tempfile::tempdir().unwrap();
    with_temp_config(|_| {
        let d = Daemon::new(ConfigFile {
            workspaces: Vec::new(),
            projects: vec![
                ProjectConfig {
                    name: "here".to_string(),
                    root: None,
                    repos: vec![dir.path().to_string_lossy().to_string()],
                    workspace: None,
                    ..Default::default()
                },
                ProjectConfig {
                    name: "elsewhere".to_string(),
                    root: None,
                    repos: vec![dir.path().to_string_lossy().to_string()],
                    workspace: Some("other".to_string()),
                    ..Default::default()
                },
            ],
            agents: Vec::new(),
            harnesses: Vec::new(),
        });

        // Spawn a pane in the *other* workspace, then look away.
        let other = workspace_named(&d, "other");
        d.open_workspace(other).unwrap();
        let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;
        let pane = d.spawn_shell(checkout).unwrap();

        d.open_workspace(workspace_named(&d, "default")).unwrap();
        assert_eq!(names_of(&d), vec!["here"], "the tree re-scoped");

        let rollup = d.workspaces();
        let other_ws = rollup.iter().find(|w| w.name == "other").unwrap();
        assert_eq!(
            other_ws.panes, 1,
            "the pane is still running and still counted"
        );

        let _ = d.close_pane(pane);
    });
}

#[test]
fn adding_a_project_files_it_under_the_open_workspace() {
    let repo = tempfile::tempdir().unwrap();
    with_temp_config(|_| {
        let d = persistent(config_with_workspaces());
        d.open_workspace(workspace_named(&d, "work")).unwrap();

        d.add_project(&repo.path().to_string_lossy()).unwrap();

        assert!(
            names_of(&d)
                .iter()
                .any(|n| n == repo.path().file_name().unwrap().to_str().unwrap()),
            "a project added while looking at a workspace belongs to it"
        );
        let work = d
            .workspaces()
            .into_iter()
            .find(|w| w.name == "work")
            .unwrap();
        assert_eq!(work.projects, 2);
    });
}

#[test]
fn an_added_projects_workspace_is_persisted_so_it_survives_a_restart() {
    let repo = tempfile::tempdir().unwrap();
    with_temp_config(|_| {
        let d = persistent(config_with_workspaces());
        d.open_workspace(workspace_named(&d, "work")).unwrap();
        d.add_project(&repo.path().to_string_lossy()).unwrap();

        let restarted = persistent(config_with_workspaces());
        let work = workspace_named(&restarted, "work");
        assert!(
            restarted
                .snapshot()
                .iter()
                .any(|p| p.name == repo.path().file_name().unwrap().to_string_lossy()),
            "the added project should be in the open workspace"
        );
        assert_eq!(
            restarted
                .workspaces()
                .into_iter()
                .find(|w| w.open)
                .unwrap()
                .id,
            work,
            "which is still the one it was added to"
        );
    });
}

#[test]
fn a_created_workspace_is_declared_on_disk_and_opened() {
    with_temp_config(|dir| {
        let d = persistent(config_with_workspaces());
        d.create_workspace("side").unwrap();

        let ws = d.workspaces();
        let side = ws.iter().find(|w| w.name == "side").expect("it exists");
        assert!(side.open, "you land in what you just made");
        assert_eq!(side.projects, 0, "and it starts empty");
        assert_eq!(names_of(&d).len(), 0, "so the tree is empty too");

        // Declared, not implied: an empty workspace has nothing in it
        // to imply it, so without a record of its own it would not
        // survive a restart.
        assert!(
            crate::store::Store::open()
                .unwrap()
                .workspace_overlays()
                .unwrap()
                .contains(&"side".to_string()),
            "the declaration must be recorded, not just held in memory"
        );
        assert!(
            !declares_a_workspace(dir),
            "and recorded beside the user's config, not in it"
        );
    });
}

#[test]
fn a_workspace_created_then_given_a_project_is_how_grouping_starts() {
    // The whole point: reaching a second workspace without editing
    // projects.toml by hand.
    let repo = tempfile::tempdir().unwrap();
    with_temp_config(|_| {
        let d = daemon_with_primary("/repo");
        d.create_workspace("side").unwrap();
        d.add_project(&repo.path().to_string_lossy()).unwrap();

        let side = d
            .workspaces()
            .into_iter()
            .find(|w| w.name == "side")
            .unwrap();
        assert_eq!(side.projects, 1, "added into what was open");
        assert_eq!(names_of(&d).len(), 1);
    });
}

#[test]
fn a_created_workspace_survives_a_restart() {
    with_temp_config(|_| {
        let d = persistent(config_with_workspaces());
        d.create_workspace("side").unwrap();
        drop(d);

        let reloaded = persistent(crate::config::load().unwrap());
        let names: Vec<String> = reloaded.workspaces().into_iter().map(|w| w.name).collect();
        assert!(names.contains(&"side".to_string()), "{names:?}");
        assert!(
            reloaded
                .workspaces()
                .iter()
                .find(|w| w.name == "side")
                .unwrap()
                .open,
            "and it is still the one open"
        );
    });
}

#[test]
fn a_workspace_that_already_exists_is_refused_rather_than_reopened() {
    // The picker already lists the existing rows; one gesture meaning
    // both "go there" and "make it" is how duplicates get made.
    with_temp_config(|dir| {
        let d = persistent(config_with_workspaces());
        assert!(d.create_workspace("work").is_err());
        assert!(d.create_workspace("   ").is_err(), "nor an empty name");

        assert_eq!(d.workspaces().len(), 3, "nothing was added");
        let written = std::fs::read_to_string(dir.join("projects.toml")).unwrap_or_default();
        assert!(
            !written.contains("[[workspace]]"),
            "and nothing was written:\n{written}"
        );
    });
}

#[test]
fn creating_a_workspace_pushes_a_new_tree_and_workspace_list() {
    with_temp_config(|_| {
        let d = persistent(config_with_workspaces());
        let mut tree_rx = d.subscribe_tree();
        let mut ws_rx = d.subscribe_workspaces();

        d.create_workspace("side").unwrap();

        let tree = tree_rx.try_recv().expect("clients need the empty tree");
        assert!(tree.is_empty());
        let ws = ws_rx.try_recv().expect("and the new row");
        assert!(ws.iter().any(|w| w.name == "side" && w.open));
    });
}

#[test]
fn an_editor_pane_will_not_open_a_path_outside_the_checkout() {
    // `path` comes from a client and lands on a command line.
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_primary(&dir.path().to_string_lossy());
    let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;

    for bad in [
        "",
        "../elsewhere.rs",
        "sub/../../elsewhere.rs",
        "/etc/passwd",
        r"\\server\share\x",
        r"C:\Windows\x",
    ] {
        assert!(
            d.spawn_editor(checkout, bad, None, false, None).is_err(),
            "{bad:?} should be refused"
        );
    }
}
