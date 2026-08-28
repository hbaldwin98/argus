//! Keeping the tree level with the disk: worktree reconciliation, the
//! project-root scan, and adding or removing rows by hand.

use super::*;
// --- worktree reconciliation -------------------------------------------

#[test]
fn snapshot_keeps_configured_repositories_separate() {
    let d = daemon_with_repositories(&["/first", "/second"]);

    let tree = d.snapshot();
    assert_eq!(tree[0].repositories.len(), 2);
    assert_eq!(tree[0].repositories[0].name, "first");
    assert_eq!(tree[0].repositories[0].checkouts.len(), 1);
    assert_eq!(tree[0].repositories[0].checkouts[0].path, "/first");
    assert_eq!(tree[0].repositories[1].name, "second");
    assert_eq!(tree[0].repositories[1].checkouts.len(), 1);
    assert_eq!(tree[0].repositories[1].checkouts[0].path, "/second");
}

#[test]
fn reconciliation_is_isolated_per_repository() {
    let d = daemon_with_repositories(&["/first", "/second"]);
    d.reconcile_worktrees_with(|primary| match primary.to_string_lossy().as_ref() {
        "/first" => listing(&["/first", "/first/wt-a"]),
        "/second" => listing(&["/second", "/second/wt-b"]),
        _ => Vec::new(),
    });

    let tree = d.snapshot();
    let first = &tree[0].repositories[0].checkouts;
    let second = &tree[0].repositories[1].checkouts;
    assert_eq!(first.len(), 2);
    assert!(first.iter().any(|c| c.path == "/first/wt-a"));
    assert!(!first.iter().any(|c| c.path == "/second/wt-b"));
    assert_eq!(second.len(), 2);
    assert!(second.iter().any(|c| c.path == "/second/wt-b"));

    d.reconcile_worktrees_with(|primary| match primary.to_string_lossy().as_ref() {
        "/first" => listing(&["/first"]),
        "/second" => listing(&["/second", "/second/wt-b"]),
        _ => Vec::new(),
    });
    let tree = d.snapshot();
    assert_eq!(tree[0].repositories[0].checkouts.len(), 1);
    assert_eq!(tree[0].repositories[1].checkouts.len(), 2);
}

#[test]
fn reconcile_adds_a_worktree_created_outside_argus() {
    let d = daemon_with_primary("/repo");
    d.reconcile_worktrees_with(|_| listing(&["/repo", "/repo/.argus/worktrees/feat"]));

    let paths = checkout_paths(&d);
    assert_eq!(
        paths.len(),
        2,
        "discovered worktree should join the tree: {paths:?}"
    );
    assert!(paths.iter().any(|p| p.ends_with("feat")));
}

#[test]
fn discovered_worktree_is_not_primary_and_primary_stays_primary() {
    let d = daemon_with_primary("/repo");
    d.reconcile_worktrees_with(|_| listing(&["/repo", "/repo/wt"]));

    let checkouts: Vec<_> = d
        .snapshot()
        .into_iter()
        .flat_map(|p| p.repositories)
        .flat_map(|r| r.checkouts)
        .collect();
    let primary = checkouts.iter().find(|c| c.path == "/repo").unwrap();
    let linked = checkouts.iter().find(|c| c.path == "/repo/wt").unwrap();
    assert!(primary.primary, "the configured checkout stays primary");
    assert!(
        !linked.primary,
        "a discovered worktree is removable, not primary"
    );
}

#[test]
fn reconcile_is_idempotent() {
    let d = daemon_with_primary("/repo");
    for _ in 0..3 {
        d.reconcile_worktrees_with(|_| listing(&["/repo", "/repo/wt"]));
    }
    assert_eq!(
        checkout_paths(&d).len(),
        2,
        "repeated ticks must not duplicate rows"
    );
}

#[test]
fn reconcile_drops_a_worktree_removed_outside_argus() {
    let d = daemon_with_primary("/repo");
    d.reconcile_worktrees_with(|_| listing(&["/repo", "/repo/wt"]));
    assert_eq!(checkout_paths(&d).len(), 2);

    d.reconcile_worktrees_with(|_| listing(&["/repo"]));
    assert_eq!(checkout_paths(&d), vec!["/repo".to_string()]);
}

#[test]
fn an_empty_listing_never_wipes_the_tree() {
    // `git::list_worktrees` returns empty when the path isn't a repo or
    // the `git` binary is missing — that must mean "nothing to
    // reconcile", never "every worktree was removed".
    let d = daemon_with_primary("/repo");
    d.reconcile_worktrees_with(|_| listing(&["/repo", "/repo/wt"]));

    d.reconcile_worktrees_with(|_| Vec::new());
    assert_eq!(checkout_paths(&d).len(), 2, "empty listing must be a no-op");
}

#[test]
fn reconcile_never_removes_the_primary_checkout() {
    // Even if git somehow stops listing it — a moved/renamed repo dir —
    // the configured checkout is the user's, not ours to drop.
    let d = daemon_with_primary("/repo");
    d.reconcile_worktrees_with(|_| listing(&["/somewhere/else"]));
    assert!(
        checkout_paths(&d).contains(&"/repo".to_string()),
        "primary must survive a listing that omits it"
    );
}

#[test]
fn discovered_checkouts_get_distinct_ids() {
    let d = daemon_with_primary("/repo");
    d.reconcile_worktrees_with(|_| listing(&["/repo", "/repo/a", "/repo/b"]));

    let ids: Vec<_> = d
        .snapshot()
        .into_iter()
        .flat_map(|p| p.repositories)
        .flat_map(|r| r.checkouts)
        .map(|c| c.id)
        .collect();
    let mut uniq = ids.clone();
    uniq.sort_by_key(|i| i.0);
    uniq.dedup();
    assert_eq!(uniq.len(), ids.len(), "ids must be unique: {ids:?}");
}

// --- display naming ----------------------------------------------------

#[test]
fn worktree_display_name_falls_back_to_the_directory_name() {
    // Non-repo path: no branch to read, so the leaf directory names it.
    assert_eq!(
        worktree_display_name(std::path::Path::new("/repo/wt/feat-x"), false),
        "feat-x"
    );
    assert_eq!(
        worktree_display_name(std::path::Path::new("/repo"), true),
        "repo"
    );
}

/// The bug this guards: `git switch` in another terminal rewrites HEAD,
/// and a status read landing in that window used to come back as a
/// checkout on no branch. That threw the row's name back to the
/// directory the worktree was created as and left the branch it was
/// really on looking free, so the column rearranged itself under the
/// user for a tick.
#[test]
fn a_status_read_that_failed_does_not_erase_the_branch_we_knew() {
    let d = daemon_with_primary("/repo");
    d.reconcile_worktrees_with(|_| listing(&["/repo", "/repo/wt"]));
    d.refresh_git_status_with(|path| {
        Some(status_on(if path.ends_with("wt") { "dev" } else { "main" }))
    });

    let worktree = d.snapshot()[0].repositories[0]
        .checkouts
        .iter()
        .find(|c| !c.primary)
        .map(|c| c.id)
        .unwrap();
    assert_eq!(
        d.snapshot()[0].repositories[0]
            .checkouts
            .iter()
            .find(|c| c.id == worktree)
            .unwrap()
            .name,
        "dev"
    );

    // git is mid-switch and cannot answer.
    d.refresh_git_status_with(|_| None);

    let c = d.snapshot()[0].repositories[0]
        .checkouts
        .iter()
        .find(|c| c.id == worktree)
        .unwrap()
        .clone();
    assert_eq!(
        c.name, "dev",
        "the row must not rename itself on a failed read"
    );
    assert_eq!(
        c.git.and_then(|g| g.branch),
        Some("dev".to_string()),
        "and must still count as the occupant of the branch"
    );
}

/// A switch made outside Argus is news, not a failure: the row follows
/// the branch that now occupies it.
#[test]
fn a_branch_switched_outside_argus_renames_the_row_it_happened_in() {
    let d = daemon_with_primary("/repo");
    d.reconcile_worktrees_with(|_| listing(&["/repo", "/repo/wt"]));
    d.refresh_git_status_with(|path| {
        Some(status_on(if path.ends_with("wt") { "dev" } else { "main" }))
    });
    d.refresh_git_status_with(|path| {
        Some(status_on(if path.ends_with("wt") {
            "spike"
        } else {
            "main"
        }))
    });

    let names: Vec<String> = d.snapshot()[0].repositories[0]
        .checkouts
        .iter()
        .map(|c| c.name.clone())
        .collect();
    assert!(
        names.contains(&"spike".to_string()) && !names.contains(&"dev".to_string()),
        "the row should follow the branch: {names:?}"
    );
}

// --- tree broadcast ----------------------------------------------------

#[test]
fn reconcile_result_is_visible_to_tree_subscribers() {
    let d = daemon_with_primary("/repo");
    let mut rx = d.subscribe_tree();
    d.reconcile_worktrees_with(|_| listing(&["/repo", "/repo/wt"]));
    d.broadcast_tree();

    let tree = rx
        .try_recv()
        .expect("a tree snapshot should have been broadcast");
    assert_eq!(tree[0].repositories[0].checkouts.len(), 2);
}

#[test]
fn default_agent_templates_are_offered_when_config_has_none() {
    let d = daemon_with_primary("/repo");
    assert_eq!(
        d.template_names(),
        vec!["claude", "codex", "opencode", "agy", "agent"]
    );
}

#[test]
fn every_built_in_template_gets_a_harness_that_can_report() {
    // Regression: `opencode` shipped as a template with no harness of
    // the same name, so it fell through to `generic` — which installs
    // nothing — and its rows never left Idle however hard it worked.
    let d = daemon_with_primary("/repo");
    for name in d.template_names() {
        let template = AgentConfig {
            name: name.clone(),
            cmd: vec![name.clone()],
            env: Default::default(),
            harness: None,
            restart: Default::default(),
        };
        let h = d.harness_for(&template);
        assert_ne!(
            h.name, "generic",
            "{name} has no harness, so its pane can never report"
        );
    }
}

#[test]
fn gen_token_is_not_a_fixed_string() {
    assert_eq!(gen_token().len(), 32);
    assert_ne!(gen_token(), gen_token());
}

#[test]
fn reconciling_a_real_repo_does_not_duplicate_the_primary_checkout() {
    // The listing and the configured path are produced by different
    // things — libgit2's workdir (canonicalized, trailing separator,
    // native separators) versus whatever the user wrote in the config.
    // If those two ever stop comparing equal, every poll tick decides
    // the primary is a newly-discovered worktree and adds another row.
    let dir = tempfile::tempdir().unwrap();
    let _repo = real_repo(dir.path());
    // Deliberately configured with forward slashes, the way the config
    // file and `add_project` write them on Windows.
    let configured = dir.path().to_string_lossy().replace('\\', "/");

    let d = Daemon::new(ConfigFile {
        workspaces: Vec::new(),
        projects: vec![ProjectConfig {
            name: "proj".to_string(),
            root: None,
            repos: vec![configured],
            workspace: None,
            ..Default::default()
        }],
        agents: Vec::new(),
        harnesses: Vec::new(),
    });

    for _ in 0..3 {
        d.reconcile_worktrees();
    }
    let checkouts = &d.snapshot()[0].repositories[0].checkouts;
    assert_eq!(
        checkouts.len(),
        1,
        "the primary must match its own listing, not clone itself: {:?}",
        checkouts.iter().map(|c| &c.path).collect::<Vec<_>>()
    );
    assert!(checkouts[0].primary);
}

#[test]
fn a_worktree_made_outside_argus_is_discovered_against_a_real_repo() {
    let dir = tempfile::tempdir().unwrap();
    let repo = real_repo(dir.path());

    let d = Daemon::new(ConfigFile {
        workspaces: Vec::new(),
        projects: vec![ProjectConfig {
            name: "proj".to_string(),
            root: None,
            repos: vec![dir.path().to_string_lossy().to_string()],
            workspace: None,
            ..Default::default()
        }],
        agents: Vec::new(),
        harnesses: Vec::new(),
    });
    d.reconcile_worktrees();
    assert_eq!(d.snapshot()[0].repositories[0].checkouts.len(), 1);

    // Someone runs `git worktree add` in a shell.
    repo.worktree("feature", &dir.path().join("wt-feature"), None)
        .unwrap();

    d.reconcile_worktrees();
    let checkouts = &d.snapshot()[0].repositories[0].checkouts;
    assert_eq!(
        checkouts.len(),
        2,
        "the new worktree should appear: {checkouts:?}"
    );
    assert!(
        checkouts.iter().any(|c| !c.primary),
        "and be removable, not marked primary"
    );
}

#[test]
fn a_root_brings_in_every_repository_under_it() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["orion", "notes"] {
        let child = dir.path().join(name);
        std::fs::create_dir(&child).unwrap();
        let _repo = real_repo(&child);
    }

    let d = daemon_rooted_at(dir.path(), &[]);
    assert_eq!(repository_names(&d), vec!["notes", "orion"]);
}

#[test]
fn repositories_written_down_outright_still_mean_exactly_what_they_did() {
    // The schema every existing config is written in. A path here is
    // taken at its word — this one is not a Git repository at all, and
    // still has to be a row with a checkout to open panes in.
    let dir = tempfile::tempdir().unwrap();
    let plain = dir.path().join("scratch");
    std::fs::create_dir(&plain).unwrap();

    let d = daemon_with_repositories(&[&plain.to_string_lossy()]);
    assert_eq!(repository_names(&d), vec!["scratch"]);
    assert_eq!(checkout_paths(&d).len(), 1);
}

#[test]
fn a_root_and_the_repositories_named_outright_combine_without_duplicating() {
    // The same repository reached both ways is one row, and the row is
    // the explicit one, so a scan can never take it away.
    let dir = tempfile::tempdir().unwrap();
    let shared = dir.path().join("orion");
    std::fs::create_dir(&shared).unwrap();
    let _repo = real_repo(&shared);
    let outside = tempfile::tempdir().unwrap();
    let _elsewhere = real_repo(outside.path());

    let d = daemon_rooted_at(
        dir.path(),
        &[&shared.to_string_lossy(), &outside.path().to_string_lossy()],
    );

    // Order is part of the contract: what the config names comes first,
    // in the order it names it, and what a scan turns up follows.
    assert_eq!(
        repository_names(&d),
        vec![
            "orion".to_string(),
            outside
                .path()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string(),
        ]
    );
}

#[test]
fn a_repository_cloned_into_a_root_arrives_on_the_next_scan() {
    // The reason the root is remembered at all rather than resolved once
    // and thrown away.
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_rooted_at(dir.path(), &[]);
    assert!(repository_names(&d).is_empty(), "nothing there yet");

    let cloned = dir.path().join("orion");
    assert!(
        d.reconcile_repositories_with(|_, _| listing(&[&cloned.to_string_lossy()])),
        "the tree changed, so clients need telling"
    );

    assert_eq!(repository_names(&d), vec!["orion"]);
}

#[test]
fn an_empty_root_is_a_project_in_its_own_right() {
    // Pressing `n` on a directory you are about to clone into should
    // leave you with the project, not with an error.
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_rooted_at(dir.path(), &[]);

    let projects = d.snapshot();
    assert_eq!(projects.len(), 1);
    assert!(projects[0].repositories.is_empty());
}

#[tokio::test]
async fn a_scan_leaves_the_repositories_it_already_knew_about_alone() {
    // Ids reach clients as selection state and reach panes as their
    // place in the tree. Rebuilding a repository that merely turned up
    // in a scan again would move the user's cursor and orphan its panes.
    let dir = tempfile::tempdir().unwrap();
    let child = dir.path().join("orion");
    std::fs::create_dir(&child).unwrap();
    let _repo = real_repo(&child);

    let d = daemon_rooted_at(dir.path(), &[]);
    let before = d.snapshot();
    let repository = before[0].repositories[0].clone();
    let pane = d.spawn_shell(repository.checkouts[0].id).unwrap();

    assert!(
        !d.reconcile_repositories_with(|_, _| listing(&[&child.to_string_lossy()])),
        "nothing changed, so nothing should be broadcast"
    );

    let after = &d.snapshot()[0].repositories[0];
    assert_eq!(after.id, repository.id);
    assert_eq!(after.checkouts[0].id, repository.checkouts[0].id);
    assert_eq!(after.checkouts[0].panes.len(), 1);

    let _ = d.close_pane(pane);
}

#[test]
fn a_discovered_repository_that_leaves_the_root_leaves_the_project() {
    let dir = tempfile::tempdir().unwrap();
    let child = dir.path().join("orion");
    std::fs::create_dir(&child).unwrap();
    let _repo = real_repo(&child);
    let d = daemon_rooted_at(dir.path(), &[]);

    assert!(d.reconcile_repositories_with(|_, _| Vec::new()));
    assert!(repository_names(&d).is_empty());
}

#[tokio::test]
async fn a_repository_holding_panes_survives_a_scan_that_cannot_find_it() {
    // A directory can go missing for reasons that have nothing to do
    // with the user's intent. Killing a running agent over it is not a
    // trade worth making, so the row waits until it is empty.
    let dir = tempfile::tempdir().unwrap();
    let child = dir.path().join("orion");
    std::fs::create_dir(&child).unwrap();
    let _repo = real_repo(&child);
    let d = daemon_rooted_at(dir.path(), &[]);
    let pane = d
        .spawn_shell(d.snapshot()[0].repositories[0].checkouts[0].id)
        .unwrap();

    assert!(!d.reconcile_repositories_with(|_, _| Vec::new()));
    assert_eq!(
        repository_names(&d),
        vec!["orion"],
        "still there, with its pane"
    );

    d.close_pane(pane).unwrap();
    assert!(d.reconcile_repositories_with(|_, _| Vec::new()));
    assert!(repository_names(&d).is_empty(), "and gone once it is empty");
}

#[test]
fn a_repository_named_outright_survives_a_scan_that_cannot_find_it() {
    // Explicit configuration is the user speaking. A scan of the root
    // has no standing to contradict it — and it may not be a Git
    // repository for a scan to find in the first place.
    let dir = tempfile::tempdir().unwrap();
    let plain = dir.path().join("scratch");
    std::fs::create_dir(&plain).unwrap();

    let d = daemon_rooted_at(dir.path(), &[&plain.to_string_lossy()]);
    assert!(!d.reconcile_repositories_with(|_, _| Vec::new()));
    assert_eq!(repository_names(&d), vec!["scratch"]);
}

#[test]
fn a_project_without_a_root_is_never_scanned() {
    let d = daemon_with_repositories(&["/configured"]);
    assert!(
        !d.reconcile_repositories_with(|_, _| panic!("a rootless project has nothing to scan")),
        "and nothing changed"
    );
}

#[test]
fn adding_a_directory_of_repositories_adds_every_repository_under_it() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["orion", "notes"] {
        let child = dir.path().join(name);
        std::fs::create_dir(&child).unwrap();
        let _repo = real_repo(&child);
    }

    with_temp_config(|_| {
        let d = Daemon::new(ConfigFile::default());
        d.add_project(&dir.path().to_string_lossy()).unwrap();
        assert_eq!(repository_names(&d), vec!["notes", "orion"]);
    });
}

#[test]
fn adding_a_repository_adds_that_one_repository() {
    // The oldest meaning of `n`, and the one that must not change.
    let dir = tempfile::tempdir().unwrap();
    let _repo = real_repo(dir.path());

    with_temp_config(|_| {
        let d = Daemon::new(ConfigFile::default());
        d.add_project(&dir.path().to_string_lossy()).unwrap();

        let name = dir
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert_eq!(repository_names(&d), vec![name]);
    });
}

#[test]
fn an_added_project_persists_the_root_it_was_given() {
    // Not the repositories found under it: writing those down would
    // freeze the project as it looked the day it was added.
    let dir = tempfile::tempdir().unwrap();
    let child = dir.path().join("orion");
    std::fs::create_dir(&child).unwrap();
    let _repo = real_repo(&child);

    with_temp_config(|_| {
        let d = persistent(ConfigFile::default());
        d.add_project(&dir.path().to_string_lossy()).unwrap();

        let recorded = crate::store::Store::open().unwrap().overlays().unwrap();
        assert_eq!(recorded.projects.len(), 1);
        let (project, repos) = &recorded.projects[0];
        assert_eq!(
            project.root,
            dir.path(),
            "the root is what gets scanned again next time"
        );
        assert!(
            repos.is_empty(),
            "and what it found is not frozen alongside it: {repos:?}"
        );
    });
}

#[test]
fn a_project_added_at_runtime_comes_back_the_same_after_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let child = dir.path().join("orion");
    std::fs::create_dir(&child).unwrap();
    let _repo = real_repo(&child);

    with_temp_config(|_| {
        let d = persistent(ConfigFile::default());
        d.add_project(&dir.path().to_string_lossy()).unwrap();
        let before = repository_names(&d);

        let restarted = persistent(crate::config::load().unwrap());
        assert_eq!(repository_names(&restarted), before);
    });
}

#[test]
fn a_repository_can_be_added_to_a_project_from_outside_its_root() {
    with_temp_config(|_| {
        let (_root, outside, d) = project_and_an_outside_repository();
        let project = d.snapshot()[0].id;

        d.add_repository(project, &outside.path().join("notes").to_string_lossy())
            .unwrap();

        assert_eq!(repository_names(&d), vec!["orion", "notes"]);
    });
}

#[test]
fn a_repository_added_by_path_is_still_there_after_a_restart() {
    with_temp_config(|_| {
        let (_root, outside, d) = project_and_an_outside_repository();
        let project = d.snapshot()[0].id;
        d.add_repository(project, &outside.path().join("notes").to_string_lossy())
            .unwrap();

        // Named repositories are built before the root is scanned, so
        // the row order changes across a restart even though the set
        // does not.
        let restarted = persistent(crate::config::load().unwrap());
        assert_eq!(repository_names(&restarted), vec!["notes", "orion"]);
    });
}

#[test]
fn a_repository_named_by_hand_is_not_a_scan_result_and_no_scan_removes_it() {
    with_temp_config(|_| {
        let (_root, outside, d) = project_and_an_outside_repository();
        let project = d.snapshot()[0].id;
        d.add_repository(project, &outside.path().join("notes").to_string_lossy())
            .unwrap();

        // A scan of the root finds only what is under it, which the
        // added repository never was.
        assert!(!d.reconcile_repositories_with(crate::git::discover_repositories_within));
        assert_eq!(repository_names(&d), vec!["orion", "notes"]);
    });
}

#[test]
fn adding_a_repository_the_project_already_has_is_refused() {
    with_temp_config(|_| {
        let (root, _outside, d) = project_and_an_outside_repository();
        let project = d.snapshot()[0].id;

        let err = d
            .add_repository(project, &root.path().join("orion").to_string_lossy())
            .unwrap_err()
            .to_string();
        assert!(err.contains("already has"), "{err}");
        assert_eq!(repository_names(&d), vec!["orion"]);
    });
}

#[test]
fn adding_something_that_is_not_a_directory_is_refused() {
    with_temp_config(|_| {
        let (root, _outside, d) = project_and_an_outside_repository();
        let project = d.snapshot()[0].id;

        let err = d
            .add_repository(project, &root.path().join("nope").to_string_lossy())
            .unwrap_err()
            .to_string();
        assert!(err.contains("not a directory"), "{err}");
    });
}

#[test]
fn adding_a_repository_back_undoes_having_removed_it() {
    with_temp_config(|_| {
        let (root, _outside, d) = project_and_an_outside_repository();
        let project = d.snapshot()[0].id;
        let repository = d.snapshot()[0].repositories[0].id;

        d.remove_repository(repository).unwrap();
        assert!(repository_names(&d).is_empty());

        d.add_repository(project, &root.path().join("orion").to_string_lossy())
            .unwrap();
        assert_eq!(repository_names(&d), vec!["orion"]);

        // The exclusion is gone too, or a restart would drop it again.
        let restarted = persistent(crate::config::load().unwrap());
        assert_eq!(repository_names(&restarted), vec!["orion"]);
    });
}

/// `init_repository` shells out to `git`, so its tests need a runtime.
/// Built here rather than with `#[tokio::test]` because the config
/// guard around them is synchronous.
fn blocking<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Runtime::new().unwrap().block_on(future)
}

#[test]
fn a_brand_new_repository_is_created_where_it_was_asked_for_and_added() {
    with_temp_config(|_| {
        let (root, _outside, d) = project_and_an_outside_repository();
        let project = d.snapshot()[0].id;
        let dest = root.path().join("fresh");

        blocking(d.init_repository(project, &dest.to_string_lossy())).unwrap();

        assert!(dest.join(".git").exists(), "git init ran in it");
        assert_eq!(repository_names(&d), vec!["orion", "fresh"]);
    });
}

#[test]
fn a_new_repository_survives_a_restart_the_way_an_added_one_does() {
    with_temp_config(|_| {
        let (root, _outside, d) = project_and_an_outside_repository();
        let project = d.snapshot()[0].id;
        blocking(d.init_repository(project, &root.path().join("fresh").to_string_lossy()))
            .unwrap();

        let restarted = persistent(crate::config::load().unwrap());
        assert_eq!(repository_names(&restarted), vec!["fresh", "orion"]);
    });
}

#[test]
fn a_directory_that_is_already_a_repository_is_added_without_being_reinited() {
    with_temp_config(|_| {
        let (_root, outside, d) = project_and_an_outside_repository();
        let project = d.snapshot()[0].id;
        let notes = outside.path().join("notes");
        let before = head_of(&notes);

        blocking(d.init_repository(project, &notes.to_string_lossy())).unwrap();

        assert_eq!(head_of(&notes), before, "its history is untouched");
        assert_eq!(repository_names(&d), vec!["orion", "notes"]);
    });
}

#[test]
fn making_a_repository_where_a_file_already_sits_is_refused() {
    with_temp_config(|_| {
        let (root, _outside, d) = project_and_an_outside_repository();
        let project = d.snapshot()[0].id;
        let file = root.path().join("taken");
        std::fs::write(&file, "").unwrap();

        let err = blocking(d.init_repository(project, &file.to_string_lossy()))
            .unwrap_err()
            .to_string();
        assert!(err.contains("is a file"), "{err}");
    });
}

#[test]
fn a_removed_project_leaves_the_tree_the_config_and_the_disk_alone() {
    with_temp_config(|cfg| {
        let (dir, d) = added_project_with(&["orion"]);
        let project = d.snapshot()[0].id;

        d.remove_project(project).unwrap();

        assert!(d.snapshot().is_empty(), "gone from the tree");
        assert!(
            persistent(crate::config::load().unwrap())
                .snapshot()
                .is_empty(),
            "and gone for good, not just for this run"
        );
        assert!(
            !declares_a_project(cfg),
            "without Argus having written to the user's config"
        );
        assert!(
            dir.path().join("orion").is_dir(),
            "removing is not deleting — the repository is still on disk"
        );
    });
}

#[test]
fn removing_a_declared_project_hides_it_without_touching_the_file() {
    // The config is hand-edited and full of comments, and taking a row
    // out of the panel is not permission to edit it. The removal is
    // recorded beside the file instead, and outlasts a restart all the
    // same.
    with_temp_config(|cfg| {
        let cfg_path = cfg.join("projects.toml");
        std::fs::write(
            &cfg_path,
            r#"# what these are
[[project]]
name = "keep-me"
repos = ["/keep"]

# the one going away
[[project]]
name = "doomed"
repos = ["/doomed"]

[[project]]
name = "also-keep"
repos = ["/also"]
"#,
        )
        .unwrap();

        let d = persistent(crate::config::load().unwrap());
        let doomed = d
            .snapshot()
            .into_iter()
            .find(|p| p.name == "doomed")
            .unwrap()
            .id;
        d.remove_project(doomed).unwrap();

        assert_eq!(
            names_of(&d),
            vec!["keep-me", "also-keep"],
            "gone from the panel"
        );
        assert_eq!(
            names_of(&persistent(crate::config::load().unwrap())),
            vec!["keep-me", "also-keep"],
            "and still gone after a restart"
        );

        let before = r#"# what these are
[[project]]
name = "keep-me"
repos = ["/keep"]

# the one going away
[[project]]
name = "doomed"
repos = ["/doomed"]

[[project]]
name = "also-keep"
repos = ["/also"]
"#;
        assert_eq!(
            std::fs::read_to_string(&cfg_path).unwrap(),
            before,
            "the user's file is untouched, comments and all"
        );
    });
}

#[test]
fn adding_a_repository_extends_that_projects_list_and_leaves_the_file_alone() {
    with_temp_config(|cfg| {
        let cfg_path = cfg.join("projects.toml");
        std::fs::write(
            &cfg_path,
            r#"# hand written
[[project]]
name = "first"
repos = [
  "/one",
]

[[project]]
name = "second"
root = "/somewhere"
"#,
        )
        .unwrap();
        let added = tempfile::tempdir().unwrap();

        let d = persistent(crate::config::load().unwrap());
        let first = d
            .snapshot()
            .into_iter()
            .find(|p| p.name == "first")
            .unwrap();
        d.add_repository(first.id, &added.path().to_string_lossy())
            .unwrap();

        let merged = crate::config::with_overlays(
            crate::config::load().unwrap(),
            &crate::store::Store::open().unwrap().overlays().unwrap(),
        );
        assert_eq!(
            merged.projects[0].repos,
            vec![
                "/one".to_string(),
                added.path().to_string_lossy().replace('\\', "/")
            ],
            "the new path joins the ones the config already lists"
        );
        assert_eq!(
            std::fs::read_to_string(&cfg_path).unwrap(),
            r#"# hand written
[[project]]
name = "first"
repos = [
  "/one",
]

[[project]]
name = "second"
root = "/somewhere"
"#,
            "and the file itself never moved"
        );
    });
}

#[test]
fn a_project_that_lists_no_repositories_yet_gains_the_key() {
    with_temp_config(|_| {
        let (_dir, d) = added_project_with(&["orion"]);
        let added = tempfile::tempdir().unwrap();

        // `add_project` writes a block with a root and no `repos`.
        d.add_repository(d.snapshot()[0].id, &added.path().to_string_lossy())
            .unwrap();

        let restarted = persistent(crate::config::load().unwrap());
        let names = repository_names(&restarted);
        assert!(
            names.contains(&"orion".to_string()) && names.len() == 2,
            "{names:?}"
        );
    });
}

#[tokio::test]
async fn a_project_still_holding_panes_is_not_removed() {
    with_temp_config(|_| {
        let (_dir, d) = added_project_with(&["orion"]);
        let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;
        let pane = d.spawn_shell(checkout).unwrap();

        let err = d.remove_project(d.snapshot()[0].id).unwrap_err();
        assert!(err.to_string().contains("panes"), "{err}");
        assert_eq!(d.snapshot().len(), 1, "and it stays put");

        d.close_pane(pane).unwrap();
        d.remove_project(d.snapshot()[0].id).unwrap();
    });
}

#[test]
fn a_removed_repository_does_not_come_back_on_the_next_scan() {
    // The project's root is scanned every ten seconds, so an exclusion
    // that only lived in memory would be undone by the next tick.
    with_temp_config(|_| {
        let (_dir, d) = added_project_with(&["orion", "notes"]);
        let doomed = d.snapshot()[0]
            .repositories
            .iter()
            .find(|r| r.name == "notes")
            .unwrap()
            .id;

        d.remove_repository(doomed).unwrap();
        assert_eq!(repository_names(&d), vec!["orion"]);

        assert!(
            !d.reconcile_repositories(),
            "a scan that finds it again changes nothing"
        );
        assert_eq!(repository_names(&d), vec!["orion"]);
    });
}

#[test]
fn a_removed_repository_is_still_gone_after_a_restart() {
    with_temp_config(|_| {
        let (_dir, d) = added_project_with(&["orion", "notes"]);
        let doomed = d.snapshot()[0].repositories[0].id;
        let kept: Vec<String> = repository_names(&d).into_iter().skip(1).collect();

        d.remove_repository(doomed).unwrap();

        let restarted = persistent(crate::config::load().unwrap());
        assert_eq!(repository_names(&restarted), kept);
    });
}

#[tokio::test]
async fn a_repository_still_holding_panes_is_not_removed() {
    with_temp_config(|_| {
        let (_dir, d) = added_project_with(&["orion"]);
        let repository = d.snapshot()[0].repositories[0].id;
        let pane = d
            .spawn_shell(d.snapshot()[0].repositories[0].checkouts[0].id)
            .unwrap();

        let err = d.remove_repository(repository).unwrap_err();
        assert!(err.to_string().contains("panes"), "{err}");
        assert_eq!(repository_names(&d), vec!["orion"]);

        d.close_pane(pane).unwrap();
        d.remove_repository(repository).unwrap();
    });
}

#[test]
fn re_adding_a_project_brings_back_the_repositories_it_had_lost() {
    // An exclusion describes a project's scan. Once the project is
    // gone, keeping it would mean adding the same directory back and
    // silently getting less than is in it.
    with_temp_config(|_| {
        let (dir, d) = added_project_with(&["orion", "notes"]);
        let doomed = d.snapshot()[0].repositories[0].id;
        d.remove_repository(doomed).unwrap();
        d.remove_project(d.snapshot()[0].id).unwrap();

        let restarted = persistent(crate::config::load().unwrap());
        restarted
            .add_project(&dir.path().to_string_lossy())
            .unwrap();
        assert_eq!(repository_names(&restarted), vec!["notes", "orion"]);
    });
}

#[test]
fn a_projects_own_scan_rules_decide_what_its_root_turns_up() {
    let root = tempfile::tempdir().unwrap();
    let kept = root.path().join("kept");
    let vendored = root.path().join("vendor").join("thing");
    std::fs::create_dir_all(&kept).unwrap();
    std::fs::create_dir_all(&vendored).unwrap();
    init_repo(&kept);
    init_repo(&vendored);

    let d = Daemon::new(ConfigFile {
        workspaces: Vec::new(),
        projects: vec![ProjectConfig {
            name: "proj".to_string(),
            root: Some(root.path().to_string_lossy().to_string()),
            exclude: vec!["vendor".to_string()],
            ..Default::default()
        }],
        agents: Vec::new(),
        harnesses: Vec::new(),
    });

    assert_eq!(repository_names(&d), vec!["kept".to_string()]);
}
