//! What survives a daemon restart, and what a resumed conversation is
//! allowed to claim.

use super::*;
#[test]
fn nothing_recorded_means_nothing_restored() {
    let dir = tempfile::tempdir().unwrap();
    with_temp_config(|_| {
        let d = daemon_for_restore(dir.path());
        d.restore_session();
        assert!(d.snapshot()[0].repositories[0].checkouts[0]
            .panes
            .is_empty());
    });
}

#[tokio::test]
async fn what_is_running_is_written_down() {
    let dir = tempfile::tempdir().unwrap();
    with_temp_config(|_| {
        let d = daemon_for_restore(dir.path());
        let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;
        d.spawn_shell(checkout).unwrap();

        let saved = saved_panes();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].kind, PaneKind::Shell);

        close_all(&d);
    });
}

#[tokio::test]
async fn what_was_running_comes_back_after_a_restart() {
    // The point of the feature: a reboot should not cost you the panes
    // you had open.
    let dir = tempfile::tempdir().unwrap();
    with_temp_config(|_| {
        record(
            &[(PaneKind::Shell, "shell"), (PaneKind::Agent, "test-agent")],
            dir.path(),
        );

        let d = daemon_for_restore(dir.path());
        d.restore_session();

        let kinds: Vec<PaneKind> = d.snapshot()[0].repositories[0].checkouts[0]
            .panes
            .iter()
            .map(|p| p.kind)
            .collect();
        assert_eq!(kinds.len(), 2, "both panes came back: {kinds:?}");
        assert!(kinds.contains(&PaneKind::Shell));
        assert!(kinds.contains(&PaneKind::Agent));

        close_all(&d);
    });
}

#[tokio::test]
async fn agent_status_and_note_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    with_temp_config(|_| {
        record_panes(vec![crate::store::SessionPane {
            checkout_path: dir.path().to_path_buf(),
            kind: PaneKind::Agent,
            title: "review parser".to_string(),
            template: Some("test-agent".to_string()),
            status: PaneStatus::NeedsReview,
            note: Some("ready to inspect".to_string()),
            harness_session_id: None,
            harness: None,
        }]);

        let d = daemon_for_restore(dir.path());
        d.restore_session();

        let pane = &d.snapshot()[0].repositories[0].checkouts[0].panes[0];
        assert_eq!(pane.status, PaneStatus::NeedsReview);
        assert_eq!(pane.note.as_deref(), Some("ready to inspect"));
        close_all(&d);
    });
}

#[tokio::test]
async fn an_agent_comes_back_as_the_template_it_was() {
    // The title is how a restored agent knows what to launch.
    let dir = tempfile::tempdir().unwrap();
    with_temp_config(|_| {
        record(&[(PaneKind::Agent, "test-agent")], dir.path());

        let d = daemon_for_restore(dir.path());
        d.restore_session();

        assert_eq!(
            d.snapshot()[0].repositories[0].checkouts[0].panes[0].title,
            "test-agent"
        );
        close_all(&d);
    });
}

#[tokio::test]
async fn an_agent_that_renamed_itself_restores_its_display_title() {
    // Regression: an agent is spawned by template name, and a renamed
    // pane's title is no longer that. Restoring by title would look up
    // a template called "fixing the pty deadlock" and find nothing.
    let dir = tempfile::tempdir().unwrap();
    with_temp_config(|_| {
        record_panes(vec![crate::store::SessionPane {
            checkout_path: dir.path().to_path_buf(),
            kind: PaneKind::Agent,
            title: "fixing the pty deadlock".to_string(),
            template: Some("test-agent".to_string()),
            status: PaneStatus::Idle,
            note: None,
            harness_session_id: None,
            harness: None,
        }]);

        let d = daemon_for_restore(dir.path());
        d.restore_session();

        let panes = &d.snapshot()[0].repositories[0].checkouts[0].panes;
        assert_eq!(panes.len(), 1, "the renamed agent should be back");
        assert_eq!(
            panes[0].title, "fixing the pty deadlock",
            "its separately persisted display title should be restored"
        );

        close_all(&d);
    });
}

#[test]
fn an_agent_whose_template_is_gone_costs_only_that_pane() {
    // Templates come from config, which changes between runs.
    let dir = tempfile::tempdir().unwrap();
    with_temp_config(|_| {
        record(&[(PaneKind::Agent, "no-such-template")], dir.path());

        let d = daemon_for_restore(dir.path());
        d.restore_session();

        assert!(
            d.snapshot()[0].repositories[0].checkouts[0]
                .panes
                .is_empty(),
            "skipped, not fatal"
        );
    });
}

#[test]
fn an_editor_is_never_restored() {
    // It belonged to a floating window that no longer exists.
    let dir = tempfile::tempdir().unwrap();
    with_temp_config(|_| {
        record(&[(PaneKind::Editor, "a.rs")], dir.path());

        let d = daemon_for_restore(dir.path());
        d.restore_session();

        assert!(d.snapshot()[0].repositories[0].checkouts[0]
            .panes
            .is_empty());
    });
}

#[test]
fn the_escape_hatch_starts_clean() {
    // For the case where the restore is itself the problem.
    let dir = tempfile::tempdir().unwrap();
    with_temp_config(|_| {
        record(&[(PaneKind::Shell, "shell")], dir.path());

        std::env::set_var(crate::store::NO_RESTORE, "1");
        let d = daemon_for_restore(dir.path());
        d.restore_session();
        std::env::remove_var(crate::store::NO_RESTORE);

        assert!(d.snapshot()[0].repositories[0].checkouts[0]
            .panes
            .is_empty());
    });
}

#[tokio::test]
async fn a_session_file_from_an_older_argus_is_restored_from() {
    // The upgrade path: what the previous version left behind is
    // imported when the store first opens, and restores like anything
    // else recorded in it.
    let dir = tempfile::tempdir().unwrap();
    with_temp_config(|cfg| {
        std::fs::write(
            cfg.join("session.json"),
            format!(
                r#"{{"panes":[{{"checkout_path":{:?},"kind":"Shell","title":"shell"}}]}}"#,
                dir.path().to_string_lossy().replace('\\', "/")
            ),
        )
        .unwrap();

        let d = Daemon::with_store(
            restore_config(dir.path()),
            crate::store::Store::open().unwrap(),
        );
        d.restore_session();

        assert_eq!(
            d.snapshot()[0].repositories[0].checkouts[0].panes.len(),
            1,
            "the imported pane should have come back"
        );
        close_all(&d);
    });
}

#[tokio::test]
async fn a_daemon_without_a_store_on_disk_writes_nothing() {
    // Every test builds a daemon; none of them may write over the real
    // user's state. `Daemon::new` is what guarantees that, by handing
    // one a store that lives and dies with the process.
    let dir = tempfile::tempdir().unwrap();
    with_temp_config(|cfg| {
        let d = Daemon::new(restore_config(dir.path()));
        let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;
        d.spawn_shell(checkout).unwrap();
        close_all(&d);

        assert!(
            !cfg.join("runtime.db").exists(),
            "a daemon persists only through the store it was given"
        );
    });
}

#[test]
fn a_running_test_daemon_does_not_open_the_config_store() {
    // `daemon_with_claude_aliases` used to call `Store::open`, so every
    // pane-start test held the process-global `runtime.db` for seconds
    // and the tests running beside it failed with SQLITE_BUSY.
    with_temp_config(|cfg| {
        let dir = tempfile::tempdir().unwrap();
        let _d = daemon_with_claude_aliases(dir.path(), &["claude"]);
        assert!(
            !cfg.join("runtime.db").exists(),
            "holding the shared store is how parallel tests lose to SQLITE_BUSY"
        );
    });
}

#[tokio::test]
async fn a_pane_you_closed_does_not_come_back() {
    // The file follows the tree, so closing one forgets it.
    let dir = tempfile::tempdir().unwrap();
    with_temp_config(|_| {
        let d = daemon_for_restore(dir.path());
        let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;
        let pane = d.spawn_shell(checkout).unwrap();
        let _ = d.close_pane(pane);

        assert!(saved_panes().is_empty());
    });
}

#[tokio::test]
async fn an_exited_pane_is_not_recorded_as_running() {
    let dir = tempfile::tempdir().unwrap();
    with_temp_config(|_| {
        let d = daemon_for_restore(dir.path());
        let checkout = d.snapshot()[0].repositories[0].checkouts[0].id;
        let pane = d.spawn_shell(checkout).unwrap();

        d.mark_pane_exited(pane, Some(0));

        assert!(saved_panes().is_empty());
        close_all(&d);
    });
}

#[tokio::test]
async fn a_pane_in_a_worktree_comes_back_too() {
    // Regression: a worktree is discovered from git by a poll that has
    // not run yet when restore does, so a pane in one looked like a
    // pane whose checkout was gone — and was silently dropped.
    let dir = tempfile::tempdir().unwrap();
    let repo = real_repo(dir.path());
    let worktree = dir.path().join("wt-feature");
    repo.worktree("feature", &worktree, None).unwrap();

    with_temp_config(|_| {
        record(&[(PaneKind::Shell, "shell")], &worktree);

        let d = daemon_for_restore(dir.path());
        d.restore_session();

        let checkouts = d.snapshot().remove(0).repositories.remove(0).checkouts;
        let restored = checkouts
            .iter()
            .find(|c| same_path(std::path::Path::new(&c.path), &worktree))
            .expect("the worktree should have joined the tree");
        assert_eq!(restored.panes.len(), 1, "its pane should have come back");

        for c in &checkouts {
            for p in &c.panes {
                let _ = d.close_pane(p.id);
            }
        }
    });
}

#[tokio::test]
async fn only_one_pane_per_checkout_reopens_the_conversation() {
    // `--continue` means "the last conversation in this directory", so
    // two of them would land on the same session and write over each
    // other. Both agents come back; only one carries the old thread.
    let dir = tempfile::tempdir().unwrap();
    with_temp_config(|_| {
        record(
            &[(PaneKind::Agent, "claude"), (PaneKind::Agent, "claude")],
            dir.path(),
        );

        let d = persistent(fake_claude_config(dir.path()));
        d.restore_session();

        let panes = d
            .snapshot()
            .remove(0)
            .repositories
            .remove(0)
            .checkouts
            .remove(0)
            .panes;
        assert_eq!(panes.len(), 2, "both agents came back");
        assert_eq!(resuming_panes(&d), 1, "one conversation, one claimant");

        close_all(&d);
    });
}

#[test]
fn a_new_agent_is_a_new_conversation() {
    // Resume arguments belong to restore alone: asking for an agent
    // means asking for one, not for the last one back.
    let (args, resuming) = agent_args(
        &["--model".to_string(), "opus".to_string()],
        &["--continue".to_string()],
        &["--resume".to_string(), "{session_id}".to_string()],
        Start::Fresh,
        None,
    );
    assert_eq!(args, vec!["--model", "opus"]);
    assert!(!resuming);
}

#[test]
fn a_restored_agent_is_asked_to_continue_where_it_left_off() {
    let (args, resuming) = agent_args(
        &["--model".to_string(), "opus".to_string()],
        &["--continue".to_string()],
        &["--resume".to_string(), "{session_id}".to_string()],
        Start::Resuming,
        None,
    );
    assert_eq!(
        args,
        vec!["--model", "opus", "--continue"],
        "after the template's own flags, which still apply"
    );
    assert!(resuming);
}

#[tokio::test]
async fn distinct_exact_ids_in_one_checkout_restore_independently() {
    let dir = tempfile::tempdir().unwrap();
    with_temp_config(|_| {
        record_agents(
            dir.path(),
            &[("first", Some("session-a")), ("second", Some("session-b"))],
        );
        let d = daemon_with_claude_aliases_for_restore(dir.path(), &["first", "second"]);
        d.restore_session();

        assert_eq!(resuming_panes(&d), 2, "exact IDs need no broad claim guard");
        let mut ids: Vec<_> = d
            .session_panes()
            .into_iter()
            .filter_map(|pane| pane.harness_session_id)
            .collect();
        ids.sort();
        assert_eq!(ids, ["session-a", "session-b"]);
        close_all(&d);
    });
}

#[tokio::test]
async fn aliases_of_one_harness_share_the_legacy_broad_claim() {
    let dir = tempfile::tempdir().unwrap();
    with_temp_config(|_| {
        record_agents(dir.path(), &[("first", None), ("second", None)]);
        let d = daemon_with_claude_aliases_for_restore(dir.path(), &["first", "second"]);
        d.restore_session();

        assert_eq!(d.snapshot()[0].repositories[0].checkouts[0].panes.len(), 2);
        assert_eq!(
            resuming_panes(&d),
            1,
            "claim is checkout plus harness, not template"
        );
        close_all(&d);
    });
}

#[test]
fn exact_resume_expands_the_id_as_argv_not_shell_text() {
    let (args, resuming) = agent_args(
        &["--model".to_string(), "opus".to_string()],
        &["--continue".to_string()],
        &["--resume".to_string(), "{session_id}".to_string()],
        Start::Resuming,
        Some("session with spaces;still-one-arg"),
    );
    assert_eq!(
        args,
        [
            "--model",
            "opus",
            "--resume",
            "session with spaces;still-one-arg"
        ]
    );
    assert!(resuming);
}

#[test]
fn a_harness_that_cannot_resume_restores_the_old_way() {
    // Nothing to append, and nothing for a failed start to fall back
    // from — the pane must not be treated as a resume that went wrong.
    let (args, resuming) = agent_args(&["-q".to_string()], &[], &[], Start::Resuming, None);
    assert_eq!(args, vec!["-q"]);
    assert!(!resuming);
}

#[test]
fn an_immediate_refusal_reads_as_nothing_to_resume() {
    assert!(nothing_to_resume(Some(1), Duration::from_millis(300)));
    assert!(
        nothing_to_resume(None, Duration::from_millis(300)),
        "no exit code at all is still a start that did not take"
    );
}

#[test]
fn a_restored_agent_that_ran_is_not_a_failed_resume() {
    assert!(
        !nothing_to_resume(Some(0), Duration::from_millis(300)),
        "these CLIs leave cleanly when you quit them"
    );
    assert!(
        !nothing_to_resume(Some(1), RESUME_GRACE + Duration::from_secs(1)),
        "it was up long enough to have been the conversation"
    );
}

#[tokio::test]
async fn a_resume_with_nothing_behind_it_comes_back_as_a_fresh_agent() {
    // The cost of guessing wrong about what a CLI can continue: the
    // user gets the agent they had, not a dead row where one should be.
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_fake_claude(dir.path());
    let pane = d
        .start_agent(only_checkout(&d), "claude", Start::Resuming, None)
        .unwrap();

    d.mark_pane_exited(pane, Some(1));

    let panes = d
        .snapshot()
        .remove(0)
        .repositories
        .remove(0)
        .checkouts
        .remove(0)
        .panes;
    assert_eq!(panes.len(), 1, "the dead row goes, it does not pile up");
    assert_ne!(panes[0].id, pane, "a new agent took its place");
    assert_eq!(panes[0].status, PaneStatus::Idle);

    close_all(&d);
}

#[tokio::test]
async fn an_agent_that_starts_and_fails_again_is_left_alone() {
    // One retry, never a loop: the replacement is a plain agent, so its
    // own failure is just a failure.
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_fake_claude(dir.path());
    let pane = d
        .start_agent(only_checkout(&d), "claude", Start::Resuming, None)
        .unwrap();

    d.mark_pane_exited(pane, Some(1));
    let replacement = d
        .snapshot()
        .remove(0)
        .repositories
        .remove(0)
        .checkouts
        .remove(0)
        .panes[0]
        .id;
    d.mark_pane_exited(replacement, Some(1));

    let panes = d
        .snapshot()
        .remove(0)
        .repositories
        .remove(0)
        .checkouts
        .remove(0)
        .panes;
    assert_eq!(panes.len(), 1);
    assert_eq!(panes[0].id, replacement, "no third attempt");
    assert_eq!(panes[0].status, PaneStatus::Exited { code: Some(1) });

    close_all(&d);
}

#[tokio::test]
async fn quitting_a_restored_agent_leaves_it_quit() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_fake_claude(dir.path());
    let pane = d
        .start_agent(only_checkout(&d), "claude", Start::Resuming, None)
        .unwrap();

    d.mark_pane_exited(pane, Some(0));

    let panes = d
        .snapshot()
        .remove(0)
        .repositories
        .remove(0)
        .checkouts
        .remove(0)
        .panes;
    assert_eq!(panes.len(), 1);
    assert_eq!(panes[0].id, pane, "still the pane the user closed");
    assert_eq!(panes[0].status, PaneStatus::Exited { code: Some(0) });

    close_all(&d);
}
