//! Round trips through a store built in memory, so no test can reach
//! the real `runtime.db`.

use super::*;

fn store() -> Store {
    Store::in_memory().unwrap()
}

fn pane(path: &str, kind: PaneKind, title: &str) -> SessionPane {
    SessionPane {
        checkout_path: PathBuf::from(path),
        kind,
        title: title.to_string(),
        template: None,
        status: PaneStatus::Idle,
        note: None,
        harness_session_id: None,
        harness: None,
    }
}

fn anchor(line: u32) -> ReviewAnchor {
    ReviewAnchor {
        commit: None,
        base: argus_protocol::ReviewBase::Unstaged,
        path: "src/main.rs".to_string(),
        old_path: None,
        old_start: None,
        old_end: None,
        new_start: Some(line),
        new_end: Some(line),
        text: vec!["+changed".to_string()],
    }
}

#[test]
fn a_pane_survives_a_round_trip() {
    let s = store();
    let mut agent = pane("/repo", PaneKind::Agent, "fixing the pty deadlock");
    agent.template = Some("claude".into());
    agent.status = PaneStatus::NeedsReview;
    agent.note = Some("ready to inspect".into());
    agent.harness = Some("claude".into());
    agent.harness_session_id = Some("session-123".into());
    let want = vec![agent, pane("/repo", PaneKind::Shell, "shell")];

    s.save_panes(&want).unwrap();
    assert_eq!(s.panes().unwrap(), want);
}

#[test]
fn review_comments_survive_a_round_trip_in_order() {
    let s = store();
    let first = s
        .add_review_comment(Path::new("/repo"), anchor(4), "first".to_string())
        .unwrap();
    let second = s
        .add_review_comment(Path::new("/repo"), anchor(9), "second".to_string())
        .unwrap();

    assert_eq!(
        s.review_comments(Path::new("/repo")).unwrap(),
        [first, second]
    );
    assert!(s.review_comments(Path::new("/other")).unwrap().is_empty());
}

#[test]
fn only_the_newest_review_comments_are_returned() {
    let s = store();
    for line in 1..=(MAX_REVIEW_COMMENTS as u32 + 5) {
        s.add_review_comment(Path::new("/repo"), anchor(line), format!("comment {line}"))
            .unwrap();
    }

    let comments = s.review_comments(Path::new("/repo")).unwrap();
    assert_eq!(comments.len(), MAX_REVIEW_COMMENTS);
    assert_eq!(comments.first().unwrap().anchor.new_start, Some(6));
    assert_eq!(comments.last().unwrap().anchor.new_start, Some(105));
}

#[test]
fn saving_replaces_rather_than_appends() {
    // The tree is the truth and the table follows it, so a pane that
    // closed has no row to update — it simply stops being written.
    let s = store();
    s.save_panes(&[pane("/old", PaneKind::Shell, "shell")])
        .unwrap();
    s.save_panes(&[pane("/new", PaneKind::Agent, "claude")])
        .unwrap();

    let back = s.panes().unwrap();
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].checkout_path, PathBuf::from("/new"));
}

#[test]
fn panes_come_back_in_the_order_they_were_saved() {
    let s = store();
    let want: Vec<SessionPane> = ["a", "b", "c"]
        .iter()
        .map(|t| pane("/repo", PaneKind::Shell, t))
        .collect();
    s.save_panes(&want).unwrap();
    let titles: Vec<String> = s.panes().unwrap().into_iter().map(|p| p.title).collect();
    assert_eq!(titles, ["a", "b", "c"]);
}

#[test]
fn an_exit_code_survives_the_round_trip() {
    // The one status carrying data, and the reason statuses are stored
    // as their serde form rather than a name.
    let s = store();
    let mut p = pane("/repo", PaneKind::Agent, "claude");
    p.status = PaneStatus::Exited { code: Some(3) };
    s.save_panes(&[p]).unwrap();
    assert_eq!(
        s.panes().unwrap()[0].status,
        PaneStatus::Exited { code: Some(3) }
    );
}

#[test]
fn the_escape_hatch_reads_nothing_back() {
    let s = store();
    s.save_panes(&[pane("/repo", PaneKind::Shell, "shell")])
        .unwrap();
    std::env::set_var(NO_RESTORE, "1");
    let out = s.panes();
    std::env::remove_var(NO_RESTORE);
    assert!(out.unwrap().is_empty());
}

#[test]
fn editors_are_not_restored() {
    // An editor belongs to the window it opened in; reopening a file
    // nobody asked for is noise.
    let known = vec![PathBuf::from("/repo")];
    let panes = vec![
        pane("/repo", PaneKind::Editor, "a.rs"),
        pane("/repo", PaneKind::Agent, "claude"),
    ];
    let out: Vec<&SessionPane> = restorable(&panes, &known).collect();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].kind, PaneKind::Agent);
}

#[test]
fn a_pane_whose_checkout_is_gone_is_dropped() {
    let known = vec![PathBuf::from("/still-here")];
    let panes = vec![
        pane("/gone", PaneKind::Shell, "shell"),
        pane("/still-here", PaneKind::Shell, "shell"),
    ];
    let out: Vec<&SessionPane> = restorable(&panes, &known).collect();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].checkout_path, PathBuf::from("/still-here"));
}

#[test]
fn paths_match_across_separator_and_case_differences() {
    let dir = tempfile::tempdir().unwrap();
    let forward = PathBuf::from(dir.path().to_string_lossy().replace('\\', "/"));
    let known = vec![dir.path().to_path_buf()];
    let panes = vec![SessionPane {
        checkout_path: forward,
        ..pane("/unused", PaneKind::Shell, "shell")
    }];
    assert_eq!(restorable(&panes, &known).count(), 1);
}

#[test]
fn a_renamed_agent_comes_back_as_the_template_it_was() {
    // Regression: an agent that renamed its own pane to what it was
    // working on would otherwise be looked up as a template by that
    // name, and never come back at all.
    let mut p = pane("/repo", PaneKind::Agent, "fixing the pty deadlock");
    p.template = Some("claude".into());
    assert_eq!(p.template(), "claude");
}

#[test]
fn two_directories_sharing_a_name_are_two_projects() {
    let s = store();
    s.add_project(&ProjectOverlay {
        name: "src".into(),
        root: PathBuf::from("/home/me/src"),
        workspace: "default".into(),
    })
    .unwrap();
    s.add_project(&ProjectOverlay {
        name: "src".into(),
        root: PathBuf::from("/elsewhere"),
        workspace: "work".into(),
    })
    .unwrap();

    let out = s.project_overlays().unwrap();
    assert_eq!(out.len(), 2, "two directories are two projects");
    assert_eq!(out[1].root, PathBuf::from("/elsewhere"));
    assert_eq!(out[1].workspace, "work");
}

#[test]
fn re_adding_the_same_directory_updates_it_in_place() {
    let s = store();
    for workspace in ["default", "work"] {
        s.add_project(&ProjectOverlay {
            name: "src".into(),
            root: PathBuf::from("/home/me/src"),
            workspace: workspace.into(),
        })
        .unwrap();
    }
    let out = s.project_overlays().unwrap();
    assert_eq!(out.len(), 1, "one directory is one project");
    assert_eq!(out[0].workspace, "work");
}

#[test]
fn removing_a_config_project_hides_it_instead_of_dropping_it() {
    // `projects.toml` is the user's file, so the only way to take a
    // project it declares out of the panel is to record the removal.
    let s = store();
    s.remove_project("declared", Some(Path::new("/declared")))
        .unwrap();
    assert_eq!(
        s.hidden_projects().unwrap(),
        ["declared"],
        "no overlay row for that root means the config declared it"
    );
}

#[test]
fn removing_an_added_project_leaves_nothing_behind() {
    let s = store();
    s.add_project(&ProjectOverlay {
        name: "added".into(),
        root: PathBuf::from("/added"),
        workspace: "default".into(),
    })
    .unwrap();
    s.add_repo("added", Path::new("/added/repo")).unwrap();

    s.remove_project("added", Some(Path::new("/added")))
        .unwrap();

    assert!(s.project_overlays().unwrap().is_empty());
    assert!(s.repos_for("added").unwrap().is_empty());
    assert!(
        s.hidden_projects().unwrap().is_empty(),
        "there is no config block to hide"
    );
}

#[test]
fn adding_a_project_back_unhides_it() {
    let s = store();
    s.remove_project("src", None).unwrap();
    s.add_project(&ProjectOverlay {
        name: "src".into(),
        root: PathBuf::from("/src"),
        workspace: "default".into(),
    })
    .unwrap();
    assert!(s.hidden_projects().unwrap().is_empty());
}

#[test]
fn a_repo_is_only_added_to_a_project_once() {
    let s = store();
    s.add_repo("src", Path::new("/src/a")).unwrap();
    s.add_repo("src", Path::new("/src/a")).unwrap();
    s.add_repo("src", Path::new("/src/b")).unwrap();
    assert_eq!(s.repos_for("src").unwrap().len(), 2);
    assert!(s.repos_for("other").unwrap().is_empty());
}

#[test]
fn exclusions_can_be_rewritten_wholesale() {
    // How exclusions under a removed project are forgotten.
    let s = store();
    s.exclude_repo(Path::new("/a")).unwrap();
    s.exclude_repo(Path::new("/b")).unwrap();
    s.set_excluded_repos(&[PathBuf::from("/b")]).unwrap();
    assert_eq!(s.excluded_repos().unwrap(), [PathBuf::from("/b")]);
}

#[test]
fn the_open_workspace_is_remembered() {
    let s = store();
    assert_eq!(s.open_workspace().unwrap(), None);
    s.set_open_workspace("work").unwrap();
    s.set_open_workspace("home").unwrap();
    assert_eq!(s.open_workspace().unwrap().as_deref(), Some("home"));
}

#[test]
fn an_empty_workspace_exists_because_it_was_declared() {
    let s = store();
    s.add_workspace("empty").unwrap();
    s.add_workspace("empty").unwrap();
    assert_eq!(s.workspace_overlays().unwrap(), ["empty"]);
}

#[test]
fn overlays_carry_each_added_projects_own_repositories() {
    let s = store();
    s.add_project(&ProjectOverlay {
        name: "added".into(),
        root: PathBuf::from("/added"),
        workspace: "default".into(),
    })
    .unwrap();
    s.add_repo("added", Path::new("/added/extra")).unwrap();
    s.add_repo("declared", Path::new("/declared/extra"))
        .unwrap();

    let o = s.overlays().unwrap();
    assert_eq!(o.projects.len(), 1);
    assert_eq!(o.projects[0].1, [PathBuf::from("/added/extra")]);
    assert_eq!(
        o.repos,
        [("declared".to_string(), PathBuf::from("/declared/extra"))],
        "an added project's repositories must not also be installed loose"
    );
}

#[test]
fn a_store_reopens_with_its_contents_and_in_wal_mode() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("runtime.db");
    {
        let s = Store::open_at(&path).unwrap();
        s.save_panes(&[pane("/repo", PaneKind::Agent, "claude")])
            .unwrap();
    }
    let s = Store::open_at(&path).unwrap();
    assert_eq!(s.panes().unwrap().len(), 1);

    let mode: String = s
        .conn()
        .pragma_query_value(None, "journal_mode", |r| r.get(0))
        .unwrap();
    assert_eq!(mode.to_lowercase(), "wal");
}

#[test]
fn a_store_from_a_newer_argus_is_refused_rather_than_rewritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("runtime.db");
    {
        let s = Store::open_at(&path).unwrap();
        s.conn()
            .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
    }
    let err = Store::open_at(&path).unwrap_err().to_string();
    assert!(err.contains("newer than this Argus understands"), "{err}");
}

#[test]
fn migrating_an_already_current_store_is_a_no_op() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("runtime.db");
    let s = Store::open_at(&path).unwrap();
    s.save_panes(&[pane("/repo", PaneKind::Shell, "shell")])
        .unwrap();
    s.migrate().unwrap();
    assert_eq!(s.panes().unwrap().len(), 1);
}

#[test]
fn migrating_a_v1_store_preserves_existing_state_and_adds_comments() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("runtime.db");
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(SCHEMA_V1).unwrap();
        conn.execute(
            "INSERT INTO pane
               (seq, checkout_path, kind, title, template, status, note, harness, harness_session_id)
             VALUES (0, '/repo', ?1, 'claude', NULL, ?2, NULL, NULL, NULL)",
            rusqlite::params![encode(&PaneKind::Agent).unwrap(), encode(&PaneStatus::Idle).unwrap()],
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
    }

    let s = Store::open_at(&path).unwrap();
    assert_eq!(s.panes().unwrap().len(), 1);
    let saved = s
        .add_review_comment(Path::new("/repo"), anchor(3), "persist this".to_string())
        .unwrap();
    assert_eq!(s.review_comments(Path::new("/repo")).unwrap(), [saved]);
    s.set_note(&NoteKey::checkout(Path::new("/repo")), "- [ ] and this")
        .unwrap();
    assert_eq!(
        s.note(&NoteKey::checkout(Path::new("/repo"))).unwrap(),
        Some("- [ ] and this".to_string())
    );
    // The v4 table too: a store that predates it can still take an
    // agent's write.
    s.set_note_as_agent(
        &NoteKey::checkout(Path::new("/repo")),
        "- [ ] and this
- [ ] and one more
",
        &audit("add", "and one more"),
    )
    .unwrap();
    assert_eq!(
        s.note_audit(&NoteKey::checkout(Path::new("/repo")))
            .unwrap()
            .len(),
        1
    );
    // And v5's board.
    s.add_decision("argus", &chose("sqlite"), None, 1, None, None).unwrap();
    assert_eq!(s.decisions("argus").unwrap().len(), 1);
}

fn chose(chose: &str) -> DecisionWrite {
    DecisionWrite {
        chose: chose.to_string(),
        ..Default::default()
    }
}

#[test]
fn a_decision_hangs_where_it_was_told_to() {
    let s = store();
    let root = s.add_decision("argus", &chose("sqlite"), None, 1, None, None).unwrap();
    let child = s
        .add_decision(
            "argus",
            &DecisionWrite {
                chose: "wal mode".into(),
                over: Some("rollback".into()),
                because: Some("readers must not block the daemon".into()),
                under: Some(root),
                ..Default::default()
            },
            None,
            2,
            Some("sess-1"),
            Some("/repo"),
        )
        .unwrap();

    let board = s.decisions("argus").unwrap();
    assert_eq!(board.len(), 2);
    assert_eq!(board[1].id, child);
    assert_eq!(board[1].parent, Some(root));
    assert_eq!(board[1].over.as_deref(), Some("rollback"));
    assert_eq!(board[1].session.as_deref(), Some("sess-1"));
    assert_eq!(board[1].checkout.as_deref(), Some("/repo"));
    assert!(!board[1].superseded());
}

#[test]
fn superseding_marks_the_old_decision_rather_than_removing_it() {
    let s = store();
    let root = s.add_decision("argus", &chose("sqlite"), None, 1, None, None).unwrap();
    let old = s
        .add_decision(
            "argus",
            &DecisionWrite {
                chose: "one table per note".into(),
                under: Some(root),
                ..Default::default()
            },
            None,
            2,
            None,
            None,
        )
        .unwrap();
    let new = s
        .add_decision(
            "argus",
            &DecisionWrite {
                chose: "one row per note".into(),
                supersedes: Some(old),
                ..Default::default()
            },
            None,
            3,
            None,
            None,
        )
        .unwrap();

    let board = s.decisions("argus").unwrap();
    let find = |id: i64| board.iter().find(|d| d.id == id).unwrap();
    assert_eq!(find(old).superseded_by, Some(new));
    assert_eq!(
        find(new).parent,
        Some(root),
        "the replacement answers the same question, so it takes the same place"
    );
}

#[test]
fn a_decision_cannot_hang_off_one_that_is_not_on_this_board() {
    let s = store();
    let elsewhere = s.add_decision("other", &chose("sqlite"), None, 1, None, None).unwrap();
    assert!(s
        .add_decision(
            "argus",
            &DecisionWrite {
                chose: "wal mode".into(),
                under: Some(elsewhere),
                ..Default::default()
            },
            None,
            2,
            None,
            None,
        )
        .is_err());
    assert!(s
        .add_decision(
            "argus",
            &DecisionWrite {
                chose: "wal mode".into(),
                supersedes: Some(elsewhere),
                ..Default::default()
            },
            None,
            2,
            None,
            None,
        )
        .is_err());
    assert!(s.decisions("argus").unwrap().is_empty());
}

#[test]
fn one_projects_board_is_not_anothers() {
    let s = store();
    s.add_decision("argus", &chose("sqlite"), None, 1, None, None).unwrap();
    s.add_decision("other", &chose("postgres"), None, 1, None, None).unwrap();
    assert_eq!(s.decisions("argus").unwrap().len(), 1);
    assert_eq!(s.decisions("argus").unwrap()[0].chose, "sqlite");
}

fn feature(title: &str) -> argus_protocol::FeatureWrite {
    argus_protocol::FeatureWrite {
        title: title.to_string(),
        body: None,
    }
}

#[test]
fn two_features_that_read_alike_do_not_share_a_board() {
    let s = store();
    let first = s
        .add_feature("argus", &feature("Retry the poll"), None, None, 1, None)
        .unwrap();
    let second = s
        .add_feature("argus", &feature("retry the poll"), None, None, 2, None)
        .unwrap();
    assert_eq!(first.slug, "retry-the-poll");
    assert_eq!(second.slug, "retry-the-poll-2");
}

#[test]
fn a_decision_is_read_back_under_the_feature_it_was_filed_under() {
    let s = store();
    s.add_feature("argus", &feature("notes storage"), None, None, 1, None)
        .unwrap();
    s.add_decision("argus", &chose("sqlite"), Some("notes-storage"), 1, None, None)
        .unwrap();
    s.add_decision("argus", &chose("one reader thread"), Some("pty"), 2, None, None)
        .unwrap();

    let board = s.decisions("argus").unwrap();
    assert_eq!(board[0].feature.as_deref(), Some("notes-storage"));
    assert_eq!(board[1].feature.as_deref(), Some("pty"));
}

#[test]
fn a_checkout_remembers_the_feature_it_was_pointed_at() {
    let s = store();
    s.add_feature("argus", &feature("notes storage"), None, None, 1, None)
        .unwrap();
    assert_eq!(s.feature_scope(Path::new("/repo"), "argus").unwrap(), None);

    s.set_feature_scope(Path::new("/repo"), "argus", "notes-storage")
        .unwrap();
    assert_eq!(
        s.feature_scope(Path::new("/repo"), "argus").unwrap().as_deref(),
        Some("notes-storage")
    );
    assert_eq!(
        s.feature_scope(Path::new("/repo"), "other").unwrap(),
        None,
        "one project's scope is not another's"
    );
    assert!(
        s.set_feature_scope(Path::new("/repo"), "argus", "nothing")
            .is_err(),
        "a checkout cannot be pointed at a feature that does not exist"
    );
}

#[test]
fn a_feature_document_grows_by_paragraph() {
    let s = store();
    s.add_feature("argus", &feature("notes storage"), None, None, 1, None)
        .unwrap();
    let body = s
        .append_to_feature("argus", "notes-storage", "the key has to outlive the ids")
        .unwrap();
    assert_eq!(body, "the key has to outlive the ids");
    let body = s
        .append_to_feature("argus", "notes-storage", "so notes are keyed by path")
        .unwrap();
    assert_eq!(
        body,
        "the key has to outlive the ids\n\nso notes are keyed by path"
    );
    assert!(s.append_to_feature("argus", "nothing", "x").is_err());
}

fn audit(action: &str, detail: &str) -> TodoAudit {
    TodoAudit {
        at: 1_700_000_000,
        session: Some("sess-1".to_string()),
        action: action.to_string(),
        detail: detail.to_string(),
    }
}

#[test]
fn an_agents_note_write_and_its_record_arrive_together() {
    let s = Store::in_memory().unwrap();
    let repo = NoteKey::checkout(Path::new("/repo"));
    let other = NoteKey::checkout(Path::new("/other"));

    s.set_note_as_agent(&repo, "- [ ] first
", &audit("add", "first"))
        .unwrap();
    s.set_note_as_agent(&repo, "- [x] first
", &audit("done", "first"))
        .unwrap();
    s.set_note_as_agent(&other, "- [ ] elsewhere
", &audit("add", "elsewhere"))
        .unwrap();

    assert_eq!(s.note(&repo).unwrap(), Some("- [x] first
".to_string()));
    assert_eq!(
        s.note_audit(&repo)
            .unwrap()
            .iter()
            .map(|e| e.action.clone())
            .collect::<Vec<_>>(),
        ["done", "add"],
        "newest first"
    );
    assert_eq!(
        s.note_audit(&other).unwrap().len(),
        1,
        "one note's record is not another's"
    );
}

#[test]
fn a_notes_record_outlives_the_note_itself() {
    let s = Store::in_memory().unwrap();
    let repo = NoteKey::checkout(Path::new("/repo"));

    s.set_note_as_agent(&repo, "- [ ] first
", &audit("add", "first"))
        .unwrap();
    // Emptying a note deletes it; what an agent did to it is still the
    // answer to "who wrote that".
    s.set_note(&repo, "").unwrap();

    assert_eq!(s.note(&repo).unwrap(), None);
    assert_eq!(s.note_audit(&repo).unwrap().len(), 1);
}

#[test]
fn a_note_round_trips_per_scope_and_key() {
    let s = Store::in_memory().unwrap();
    let repo = NoteKey::checkout(Path::new("/repo"));
    let other = NoteKey::checkout(Path::new("/other"));
    let project = NoteKey::Project("repo".to_string());

    s.set_note(&repo, "checkout note").unwrap();
    s.set_note(&project, "project note").unwrap();

    assert_eq!(s.note(&repo).unwrap(), Some("checkout note".to_string()));
    assert_eq!(s.note(&project).unwrap(), Some("project note".to_string()));
    assert_eq!(
        s.note(&other).unwrap(),
        None,
        "a checkout with no note has none"
    );
}

#[test]
fn a_project_and_a_checkout_of_the_same_name_hold_separate_notes() {
    let s = Store::in_memory().unwrap();
    s.set_note(&NoteKey::Project("argus".to_string()), "the project")
        .unwrap();
    s.set_note(&NoteKey::Checkout(PathBuf::from("argus")), "the checkout")
        .unwrap();
    assert_eq!(
        s.note(&NoteKey::Project("argus".to_string())).unwrap(),
        Some("the project".to_string())
    );
    assert_eq!(
        s.note(&NoteKey::Checkout(PathBuf::from("argus"))).unwrap(),
        Some("the checkout".to_string())
    );
}

#[test]
fn rewriting_a_note_replaces_it_rather_than_adding_a_second() {
    let s = Store::in_memory().unwrap();
    let key = NoteKey::checkout(Path::new("/repo"));
    s.set_note(&key, "first").unwrap();
    s.set_note(&key, "second").unwrap();
    assert_eq!(s.note(&key).unwrap(), Some("second".to_string()));
    assert_eq!(s.notes().unwrap().len(), 1);
}

#[test]
fn emptying_a_note_deletes_it() {
    let s = Store::in_memory().unwrap();
    let key = NoteKey::checkout(Path::new("/repo"));
    s.set_note(&key, "- [ ] something").unwrap();
    s.set_note(&key, "   
  
").unwrap();
    assert_eq!(s.note(&key).unwrap(), None);
    assert!(s.notes().unwrap().is_empty());
}

#[test]
fn every_note_reads_back_in_one_pass_for_the_tree() {
    let s = Store::in_memory().unwrap();
    s.set_note(&NoteKey::checkout(Path::new("/a")), "a").unwrap();
    s.set_note(&NoteKey::checkout(Path::new("/b")), "b").unwrap();
    s.set_note(&NoteKey::Project("p".to_string()), "p").unwrap();

    let mut notes = s.notes().unwrap();
    notes.sort_by(|a, b| a.1.cmp(&b.1));
    assert_eq!(
        notes,
        [
            (NoteKey::checkout(Path::new("/a")), "a".to_string()),
            (NoteKey::checkout(Path::new("/b")), "b".to_string()),
            (NoteKey::Project("p".to_string()), "p".to_string()),
        ]
    );
}

#[test]
fn a_note_scope_this_argus_does_not_know_is_dropped_rather_than_guessed() {
    let s = Store::in_memory().unwrap();
    s.conn()
        .execute(
            "INSERT INTO note (scope, key, body) VALUES ('board', 'x', 'from the future')",
            [],
        )
        .unwrap();
    assert!(s.notes().unwrap().is_empty());
}

#[test]
fn a_session_file_is_imported_once_and_moved_aside() {
    let dir = tempfile::tempdir().unwrap();
    let session = dir.path().join("session.json");
    std::fs::write(
        &session,
        r#"{"panes":[{"checkout_path":"/repo","kind":"Agent","title":"claude"}]}"#,
    )
    .unwrap();

    let s = Store::open_at(&dir.path().join("runtime.db")).unwrap();
    s.import_legacy_files(dir.path()).unwrap();

    let panes = s.panes().unwrap();
    assert_eq!(panes.len(), 1);
    assert_eq!(
        panes[0].status,
        PaneStatus::Idle,
        "a file older than status persistence restores as idle"
    );
    assert!(!session.exists(), "an imported file is moved aside");
    assert!(dir.path().join("session.imported").exists());

    // A second start must not import it again over newer rows.
    s.save_panes(&[]).unwrap();
    s.import_legacy_files(dir.path()).unwrap();
    assert!(s.panes().unwrap().is_empty());
}

#[test]
fn a_broken_session_file_is_left_untouched_for_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let session = dir.path().join("session.json");
    std::fs::write(&session, b"{ incomplete").unwrap();

    let s = Store::open_at(&dir.path().join("runtime.db")).unwrap();
    s.import_legacy_files(dir.path()).unwrap();

    assert!(s.panes().unwrap().is_empty());
    assert_eq!(
        std::fs::read(&session).unwrap(),
        b"{ incomplete",
        "a file we could not read is one a later version might"
    );
}

#[test]
fn the_older_side_files_are_imported_too() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("excluded-repos"), "/a\n\n/b\n").unwrap();
    std::fs::write(dir.path().join("open-workspace"), "work\n").unwrap();

    let s = Store::open_at(&dir.path().join("runtime.db")).unwrap();
    s.import_legacy_files(dir.path()).unwrap();

    let mut excluded = s.excluded_repos().unwrap();
    excluded.sort();
    assert_eq!(excluded, [PathBuf::from("/a"), PathBuf::from("/b")]);
    assert_eq!(s.open_workspace().unwrap().as_deref(), Some("work"));
}
