//! The managed hook blocks a checkout carries while an agent is
//! running in it, and their removal when the last one leaves.

use super::*;
use argus_protocol::{ArtifactScope, DecisionWrite, TaskAction, TaskWrite};
#[tokio::test]
async fn sharing_a_checkout_is_allowed_unless_the_project_says_otherwise() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_claude_aliases(dir.path(), &["claude"]);
    let checkout = only_checkout(&d);

    let first = d.spawn_agent(checkout, "claude").unwrap();
    let second = d.spawn_agent(checkout, "claude").unwrap();

    assert_eq!(
        d.snapshot()[0].repositories[0].checkouts[0].panes.len(),
        2,
        "two agents in one checkout is shown, not refused"
    );
    let _ = d.close_pane(first);
    let _ = d.close_pane(second);
}

#[tokio::test]
async fn an_exclusive_project_refuses_a_second_agent_in_one_checkout() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_an_exclusive_project(dir.path());
    let checkout = only_checkout(&d);
    let first = d.spawn_agent(checkout, "claude").unwrap();

    let err = d.spawn_agent(checkout, "claude").unwrap_err().to_string();

    assert!(err.contains("worktree"), "say what to do instead: {err:?}");
    assert_eq!(d.snapshot()[0].repositories[0].checkouts[0].panes.len(), 1);
    let _ = d.close_pane(first);
}

#[tokio::test]
async fn exclusivity_is_about_agents_not_shells() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_an_exclusive_project(dir.path());
    let checkout = only_checkout(&d);
    let agent = d.spawn_agent(checkout, "claude").unwrap();

    let shell = d.spawn_shell(checkout).expect("a shell is not an agent");

    let _ = d.close_pane(shell);
    let _ = d.close_pane(agent);
}

#[tokio::test]
async fn an_exclusive_checkout_takes_an_agent_again_once_the_first_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_an_exclusive_project(dir.path());
    let checkout = only_checkout(&d);
    let first = d.spawn_agent(checkout, "claude").unwrap();
    d.close_pane(first).unwrap();

    let second = d.spawn_agent(checkout, "claude").unwrap();

    let _ = d.close_pane(second);
}

#[tokio::test]
async fn a_review_comment_is_saved_before_it_is_sent_to_an_agent() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_claude_aliases(dir.path(), &["claude"]);
    let checkout = only_checkout(&d);
    let agent = d.spawn_agent(checkout, "claude").unwrap();

    let (id, delivered) = d
        .submit_review_comment(checkout, agent, review_anchor(8), "fix this".to_string())
        .unwrap();

    assert!(delivered);
    let comments = d.review_comments_for_agent(agent).unwrap();
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].id, id);
    assert_eq!(comments[0].body, "fix this");
    close_all(&d);
}

#[tokio::test]
async fn review_comments_require_a_live_agent_in_the_reviewed_checkout() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_claude_aliases(dir.path(), &["claude"]);
    let checkout = only_checkout(&d);
    let shell = d.spawn_shell(checkout).unwrap();

    assert!(d
        .submit_review_comment(checkout, shell, review_anchor(1), "fix".to_string())
        .is_err());
    assert!(d.review_comments_for_agent(shell).is_err());
    close_all(&d);
}

#[tokio::test]
async fn an_authorized_comments_hook_returns_checkout_feedback() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_claude_aliases(dir.path(), &["claude"]);
    d.start_hook_server().unwrap();
    let checkout = only_checkout(&d);
    let source = d.spawn_agent(checkout, "claude").unwrap();
    d.submit_review_comment(
        checkout,
        source,
        review_anchor(6),
        "consider this".to_string(),
    )
    .unwrap();

    let response = post_agent_hook(&d, source, Endpoint::Comments, "").await;

    assert!(response.starts_with(b"HTTP/1.1 200 OK"));
    let body = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| &response[index + 4..])
        .unwrap();
    let comments: Vec<argus_protocol::ReviewComment> = serde_json::from_slice(body).unwrap();
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].body, "consider this");
    close_all(&d);
}

#[tokio::test]
async fn hook_rejects_a_body_over_the_shared_limit() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_claude_aliases(dir.path(), &["claude"]);
    d.start_hook_server().unwrap();
    let source = d.spawn_agent(only_checkout(&d), "claude").unwrap();

    let response = post_agent_hook(&d, source, Endpoint::Title, &"x".repeat(4097)).await;

    assert!(response.starts_with(b"HTTP/1.1 413 Content Too Large"));
    close_all(&d);
}

#[test]
fn startup_sweeps_hooks_left_by_a_previous_daemon() {
    // Regression: a daemon's ephemeral port dies with it, so hooks left
    // in a checkout fire against nobody — and break every later agent
    // run in that directory, Argus-managed or not.
    let dir = tempfile::tempdir().unwrap();
    crate::harness::Harness::claude()
        .install(dir.path(), PaneId(4), 65140, "old")
        .unwrap();
    assert!(settings_of(dir.path()).exists());
    assert!(dir.path().join(".claude/skills/argus/SKILL.md").is_file());

    let d = daemon_with_fake_claude(dir.path());
    d.sweep_stale_hooks();
    assert!(!dir.path().join(".claude/skills/argus").exists());
    assert!(
        !settings_of(dir.path()).exists(),
        "a previous boot's hooks must not survive startup"
    );
}

#[test]
fn every_git_checkout_gets_local_excludes_for_argus_files() {
    let dir = tempfile::tempdir().unwrap();
    git2::Repository::init(dir.path()).unwrap();

    let d = daemon_with_fake_claude(dir.path());
    d.sweep_stale_hooks();

    let excludes = std::fs::read_to_string(dir.path().join(".git/info/exclude")).unwrap();
    assert!(excludes.contains("/.claude/settings.local.json"));
    assert!(excludes.contains("/.agents/hooks.json"));
    assert!(excludes.contains("/.pi/extensions/argus-status.ts"));
    assert!(excludes.contains("/.claude/skills/argus/SKILL.md"));
}

#[test]
fn default_worktree_root_is_excluded_from_the_primary_checkout() {
    let dir = tempfile::tempdir().unwrap();
    git2::Repository::init(dir.path()).unwrap();
    std::fs::create_dir_all(dir.path().join(".argus/worktrees/feature")).unwrap();
    std::fs::write(dir.path().join(".argus/worktrees/feature/file"), "x").unwrap();

    let d = daemon_with_fake_claude(dir.path());
    d.sweep_stale_hooks();

    let repo = git2::Repository::open(dir.path()).unwrap();
    let mut options = git2::StatusOptions::new();
    options.include_untracked(true);
    let statuses = repo.statuses(Some(&mut options)).unwrap();
    assert!(
        statuses.iter().all(|s| !s.path().unwrap().starts_with(".argus")),
        "linked worktrees must not show as untracked files"
    );
}

#[test]
fn sweeping_a_checkout_that_never_hosted_an_agent_is_harmless() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_fake_claude(dir.path());
    d.sweep_stale_hooks();
    assert!(
        !dir.path().join(".claude").exists(),
        "must not create anything"
    );
}

#[tokio::test]
async fn closing_the_last_agent_pane_takes_its_hooks_out() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_fake_claude(dir.path());
    d.start_hook_server().unwrap();

    let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();
    assert!(settings_of(dir.path()).exists(), "spawning installs hooks");

    d.close_pane(pane).unwrap();
    assert!(!dir.path().join(".claude/skills/argus").exists());
    assert!(
        !settings_of(dir.path()).exists(),
        "the last agent leaving takes the hooks with it"
    );
}

#[tokio::test]
async fn closing_one_of_two_agent_panes_leaves_the_hooks_alone() {
    // Hooks belong to the checkout, not the pane — pulling them while a
    // second agent is still running there would blind it.
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_fake_claude(dir.path());
    d.start_hook_server().unwrap();
    let checkout = only_checkout(&d);

    let first = d.spawn_agent(checkout, "claude").unwrap();
    let _second = d.spawn_agent(checkout, "claude").unwrap();

    d.close_pane(first).unwrap();
    assert!(dir.path().join(".claude/skills/argus/SKILL.md").is_file());
    assert!(
        settings_of(dir.path()).exists(),
        "the surviving agent still needs its status hooks"
    );
}

#[tokio::test]
async fn closing_a_shell_pane_does_not_disturb_an_agents_hooks() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_fake_claude(dir.path());
    d.start_hook_server().unwrap();
    let checkout = only_checkout(&d);

    let _agent = d.spawn_agent(checkout, "claude").unwrap();
    let shell = d.spawn_shell(checkout).unwrap();

    d.close_pane(shell).unwrap();
    assert!(settings_of(dir.path()).exists());
}

// --- features and the decision board ------------------------------------

/// Opens a feature and points the pane's checkout at it, which every
/// decision now needs: a decision with nowhere to be filed is the pile
/// this scoping exists to end.
fn open_feature(d: &Daemon, agent: PaneId, title: &str) -> String {
    let board = d
        .open_feature_for_agent(
            agent,
            None,
            argus_protocol::FeatureWrite {
                title: title.into(),
                body: None,
            },
            ArtifactScope::default(),
        )
        .unwrap();
    board.current.expect("opening a feature works on it")
}

#[tokio::test]
async fn a_withdrawn_decision_leaves_what_agents_read_and_stays_in_the_history() {
    use argus_protocol::DecisionChange;

    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_claude_aliases(dir.path(), &["claude"]);
    let checkout = only_checkout(&d);
    let project = d.snapshot()[0].id;
    let agent = d.spawn_agent(checkout, "claude").unwrap();
    open_feature(&d, agent, "elsewhere");
    let elsewhere = d
        .record_agent_decision(
            agent,
            None,
            DecisionWrite {
                chose: "filed under another feature".into(),
                ..Default::default()
            },
            ArtifactScope::default(),
        )
        .unwrap()
        .id;
    open_feature(&d, agent, "here");
    let mistaken = d
        .record_agent_decision(
            agent,
            None,
            DecisionWrite {
                chose: "a mistake".into(),
                ..Default::default()
            },
            ArtifactScope::default(),
        )
        .unwrap()
        .id;
    let read = |d: &Daemon| {
        d.decisions_for_agent(agent, ArtifactScope::default())
            .unwrap()
            .decisions
            .len()
    };
    assert_eq!(read(&d), 1);

    d.change_decision_for_agent(agent, DecisionChange::Withdraw { id: mistaken }, ArtifactScope::default())
        .unwrap();
    assert_eq!(read(&d), 0, "agents read what still stands");
    let history = d.decision_board(project, checkout).unwrap();
    let kept = history.decisions.iter().find(|x| x.id == mistaken).unwrap();
    assert!(kept.withdrawn(), "and the view keeps it, marked");

    d.change_decision_for_agent(agent, DecisionChange::Restore { id: mistaken }, ArtifactScope::default())
        .unwrap();
    assert_eq!(read(&d), 1, "restoring undoes it");

    let refused = d
        .change_decision_for_agent(
            agent,
            DecisionChange::Withdraw { id: elsewhere },
            ArtifactScope::default(),
        )
        .unwrap_err();
    assert!(refused.to_string().contains("not under this feature"), "{refused}");

    d.close_pane(agent).unwrap();
}

#[tokio::test]
async fn an_agent_corrects_a_feature_it_opened_and_drops_it_once_empty() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_claude_aliases(dir.path(), &["claude"]);
    let agent = d.spawn_agent(only_checkout(&d), "claude").unwrap();
    let slug = open_feature(&d, agent, "mistaken work");
    let feature_of = |d: &Daemon| {
        d.feature_board_for_agent(agent, ArtifactScope::default())
            .unwrap()
            .features
            .into_iter()
            .find(|f| f.slug == slug)
    };

    d.retitle_feature_for_agent(agent, &slug, "corrected work", ArtifactScope::default())
        .unwrap();
    d.rewrite_feature_for_agent(agent, &slug, "what it is for", ArtifactScope::default())
        .unwrap();
    let feature = feature_of(&d).expect("still there, same slug");
    assert_eq!(feature.title, "corrected work");
    assert_eq!(feature.body, "what it is for");

    d.part_action_for_agent(
        agent,
        None,
        TaskAction::Add(TaskWrite {
            title: "someone's task".into(),
            external: None,
            parent: None,
        }),
        ArtifactScope::default(),
    )
    .unwrap();
    let refused = d
        .drop_feature_for_agent(agent, &slug, ArtifactScope::default())
        .unwrap_err();
    assert!(refused.to_string().contains("1 task"), "{refused}");
    assert!(feature_of(&d).is_some(), "work under it is not an agent's to delete whole");

    let task = d
        .part_action_for_agent(agent, None, TaskAction::List, ArtifactScope::default())
        .unwrap()
        .tasks[0]
        .id;
    d.part_action_for_agent(agent, None, TaskAction::Remove { id: task }, ArtifactScope::default())
        .unwrap();
    d.drop_feature_for_agent(agent, &slug, ArtifactScope::default())
        .unwrap();
    assert!(feature_of(&d).is_none(), "empty, it goes");

    d.close_pane(agent).unwrap();
}

#[tokio::test]
async fn an_agent_told_to_accept_a_feature_is_recorded_as_the_one_who_did() {
    // Acceptance is a person's call; an agent carrying it out must not
    // make the history say the person moved it themselves.
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_claude_aliases(dir.path(), &["claude"]);
    let agent = d.spawn_agent(only_checkout(&d), "claude").unwrap();
    let slug = open_feature(&d, agent, "finished work");
    let state_of = |d: &Daemon| {
        d.feature_board_for_agent(agent, ArtifactScope::default())
            .unwrap()
            .features
            .into_iter()
            .find(|f| f.slug == slug)
            .unwrap()
            .state
    };

    d.move_feature_for_agent(
        agent,
        Some("session-1"),
        &slug,
        argus_protocol::FeatureState::Done,
        ArtifactScope::default(),
    )
    .unwrap();
    assert_eq!(state_of(&d), argus_protocol::FeatureState::Done);
    let key = d.agent_scope(agent).unwrap();
    let key = key.artifact_key(ArtifactScope::default());
    let accepted = d.store.feature_events(key, &slug).unwrap();
    let last = accepted.last().unwrap();
    assert_eq!((last.actor.as_str(), last.session.as_deref()), ("agent", Some("session-1")));

    d.move_feature_for_agent(
        agent,
        None,
        &slug,
        argus_protocol::FeatureState::Open,
        ArtifactScope::default(),
    )
    .unwrap();
    assert_eq!(state_of(&d), argus_protocol::FeatureState::Open, "reopen undoes it");

    d.close_pane(agent).unwrap();
}

#[tokio::test]
async fn an_agent_reads_and_writes_a_feature_by_name_without_moving_its_checkout() {
    // Several features sharing one checkout: reaching one by moving the
    // checkout's pointer would move every other agent's with it.
    use crate::state::features::Filing;

    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_claude_aliases(dir.path(), &["claude"]);
    let agent = d.spawn_agent(only_checkout(&d), "claude").unwrap();
    let other = open_feature(&d, agent, "other work");
    let mine = open_feature(&d, agent, "my work");
    let named = Filing {
        scope: ArtifactScope::default(),
        feature: Some(other.clone()),
    };

    let board = d.feature_board_for_agent(agent, named.clone()).unwrap();
    assert_eq!(board.current.as_deref(), Some(other.as_str()), "a read of it by name");

    d.part_action_for_agent(
        agent,
        None,
        TaskAction::Add(TaskWrite {
            title: "for the other feature".into(),
            external: None,
            parent: None,
        }),
        named.clone(),
    )
    .unwrap();
    let theirs = d
        .part_action_for_agent(agent, None, TaskAction::List, named.clone())
        .unwrap();
    assert_eq!(theirs.tasks.len(), 1, "a write to it by name");
    let own = d
        .part_action_for_agent(agent, None, TaskAction::List, ArtifactScope::default())
        .unwrap();
    assert!(own.tasks.is_empty(), "and not to the checkout's");

    d.record_agent_decision(
        agent,
        None,
        DecisionWrite {
            chose: "keep them apart".into(),
            ..Default::default()
        },
        named.clone(),
    )
    .unwrap();
    assert_eq!(d.decisions_for_agent(agent, named).unwrap().decisions.len(), 1);

    assert_eq!(
        d.feature_board_for_agent(agent, ArtifactScope::default())
            .unwrap()
            .current
            .as_deref(),
        Some(mine.as_str()),
        "the checkout stays on its own feature"
    );

    let unknown = Filing {
        scope: ArtifactScope::default(),
        feature: Some("no-such-feature".into()),
    };
    let refused = d.feature_board_for_agent(agent, unknown).unwrap_err();
    assert!(refused.to_string().contains("no-such-feature"), "{refused}");

    d.close_pane(agent).unwrap();
}

#[tokio::test]
async fn artifacts_are_repository_scoped_unless_workspace_scope_is_requested() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let d = daemon_with_two_agent_checkouts(first.path(), second.path());
    let snapshot = d.snapshot();
    let first_checkout = snapshot[0].repositories[0].checkouts[0].id;
    let second_checkout = snapshot[0].repositories[1].checkouts[0].id;
    let first_agent = d.spawn_agent(first_checkout, "claude").unwrap();
    let second_agent = d.spawn_agent(second_checkout, "claude").unwrap();

    open_feature(&d, first_agent, "branch-local work");
    assert!(
        d.feature_board_for_agent(second_agent, ArtifactScope::default())
            .unwrap()
            .features
            .is_empty(),
        "the default board does not cross repositories"
    );

    let shared = d
        .open_feature_for_agent(
            first_agent,
            None,
            argus_protocol::FeatureWrite {
                title: "workspace migration".into(),
                body: None,
            },
            ArtifactScope::Workspace,
        )
        .unwrap()
        .current
        .unwrap();
    d.select_feature_for_agent(second_agent, &shared, ArtifactScope::Workspace)
        .unwrap();
    d.record_agent_decision(
        first_agent,
        None,
        DecisionWrite {
            chose: "move both repositories together".into(),
            ..Default::default()
        },
        ArtifactScope::Workspace,
    )
    .unwrap();
    d.part_action_for_agent(
        first_agent,
        None,
        TaskAction::Add(TaskWrite {
            title: "update both crates".into(),
            external: None,
            parent: None,
        }),
        ArtifactScope::Workspace,
    )
    .unwrap();

    let decisions = d
        .decisions_for_agent(second_agent, ArtifactScope::Workspace)
        .unwrap();
    assert_eq!(
        decisions.decisions[0].chose,
        "move both repositories together"
    );
    let tasks = d
        .part_action_for_agent(
            second_agent,
            None,
            TaskAction::List,
            ArtifactScope::Workspace,
        )
        .unwrap();
    assert_eq!(tasks.tasks[0].title, "update both crates");
    close_all(&d);
}

#[tokio::test]
async fn repository_features_cross_branches_and_can_transfer_assignment() {
    let dir = tempfile::tempdir().unwrap();
    let _repo = real_repo(dir.path());
    let d = daemon_with_fake_claude(dir.path());
    let project = d.snapshot()[0].id;
    let source_checkout = only_checkout(&d);
    d.create_worktree(source_checkout, "topic".to_string())
        .await
        .unwrap();
    let destination_checkout = d.snapshot()[0].repositories[0]
        .checkouts
        .iter()
        .find(|checkout| checkout.id != source_checkout)
        .unwrap()
        .id;
    let source_agent = d.spawn_agent(source_checkout, "claude").unwrap();
    let destination_agent = d.spawn_agent(destination_checkout, "claude").unwrap();

    let slug = open_feature(&d, source_agent, "shared work");
    let destination_board = d
        .feature_board_for_agent(destination_agent, ArtifactScope::default())
        .unwrap();
    assert_eq!(destination_board.features.len(), 1);
    assert_eq!(destination_board.current, None);

    d.transfer_feature_for_client(project, source_checkout, destination_checkout, &slug)
        .unwrap();
    let source_board = d
        .feature_board_for_agent(source_agent, ArtifactScope::default())
        .unwrap();
    let destination_board = d
        .feature_board_for_agent(destination_agent, ArtifactScope::default())
        .unwrap();
    assert_eq!(source_board.current, None);
    assert_eq!(destination_board.current.as_deref(), Some(slug.as_str()));
    assert_eq!(destination_board.features[0].checkouts.len(), 1);
    assert!(destination_board.features[0].checkouts[0].contains("topic"));
    assert!(d
        .transfer_feature_for_client(project, source_checkout, destination_checkout, &slug)
        .is_err());
    close_all(&d);
}

#[tokio::test]
async fn a_decision_has_nowhere_to_go_until_the_checkout_is_on_a_feature() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_fake_claude(dir.path());
    let checkout = only_checkout(&d);
    let project = d.snapshot()[0].id;
    let agent = d.spawn_agent(checkout, "claude").unwrap();

    let refused = d
        .record_agent_decision(
            agent,
            None,
            DecisionWrite {
                chose: "sqlite".into(),
                ..Default::default()
            },
            ArtifactScope::default(),
        )
        .unwrap_err()
        .to_string();
    assert!(refused.contains("not on a feature"), "{refused}");
    assert!(d
        .decision_board(project, checkout)
        .unwrap()
        .decisions
        .is_empty());

    open_feature(&d, agent, "notes storage");
    assert!(d
        .record_agent_decision(
            agent,
            None,
            DecisionWrite {
                chose: "sqlite".into(),
                ..Default::default()
            },
            ArtifactScope::default(),
        )
        .is_ok());
    close_all(&d);
}

#[tokio::test]
async fn an_agent_reads_its_own_features_decisions_and_not_the_projects() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_fake_claude(dir.path());
    let checkout = only_checkout(&d);
    let agent = d.spawn_agent(checkout, "claude").unwrap();

    open_feature(&d, agent, "notes storage");
    d.record_agent_decision(
        agent,
        None,
        DecisionWrite {
            chose: "sqlite".into(),
            ..Default::default()
        },
        ArtifactScope::default(),
    )
    .unwrap();
    // The same checkout moves on to something else, the way an agent that
    // finished one feature and started another does.
    let second = open_feature(&d, agent, "the pty deadlock");
    d.record_agent_decision(
        agent,
        None,
        DecisionWrite {
            chose: "one reader thread".into(),
            ..Default::default()
        },
        ArtifactScope::default(),
    )
    .unwrap();

    let board = d
        .decisions_for_agent(agent, ArtifactScope::default())
        .unwrap();
    assert_eq!(
        board
            .decisions
            .iter()
            .map(|d| d.chose.as_str())
            .collect::<Vec<_>>(),
        ["one reader thread"],
        "the board an agent reads is the feature it is on, not the project"
    );
    let features = d
        .feature_board_for_agent(agent, ArtifactScope::default())
        .unwrap();
    assert_eq!(features.current.as_deref(), Some(second.as_str()));
    assert_eq!(features.features.len(), 2, "the others are still offered");
    // The project-wide board is what the client draws, and keeps both.
    assert_eq!(
        d.decision_board(d.snapshot()[0].id, checkout)
            .unwrap()
            .decisions
            .len(),
        2
    );
    close_all(&d);
}

#[tokio::test]
async fn a_feature_document_grows_and_a_checkout_can_go_back_to_it() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_fake_claude(dir.path());
    let checkout = only_checkout(&d);
    let agent = d.spawn_agent(checkout, "claude").unwrap();

    let first = open_feature(&d, agent, "notes storage");
    d.append_to_feature_for_agent(
        agent,
        "the key has to outlive the ids",
        ArtifactScope::default(),
    )
    .unwrap();
    open_feature(&d, agent, "the pty deadlock");

    let board = d
        .select_feature_for_agent(agent, &first, ArtifactScope::default())
        .unwrap();
    assert_eq!(board.current.as_deref(), Some(first.as_str()));
    let document = &board
        .features
        .iter()
        .find(|f| f.slug == first)
        .unwrap()
        .body;
    assert_eq!(document, "the key has to outlive the ids");

    assert!(
        d.select_feature_for_agent(agent, "no-such-feature", ArtifactScope::default())
            .is_err(),
        "a checkout cannot be pointed at a feature that does not exist"
    );
    close_all(&d);
}

// --- the decision board -------------------------------------------------

#[tokio::test]
async fn a_decision_lands_on_its_projects_board_wherever_the_agent_was() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_fake_claude(dir.path());
    let checkout = only_checkout(&d);
    let project = d.snapshot()[0].id;
    let agent = d.spawn_agent(checkout, "claude").unwrap();
    open_feature(&d, agent, "notes storage");

    let root = d
        .record_agent_decision(
            agent,
            Some("sess-1"),
            DecisionWrite {
                chose: "sqlite".into(),
                over: Some("a file per feature".into()),
                because: Some("both need migrations".into()),
                ..Default::default()
            },
            ArtifactScope::default(),
        )
        .unwrap();
    let child = d
        .record_agent_decision(
            agent,
            Some("sess-1"),
            DecisionWrite {
                chose: "wal mode".into(),
                under: Some(root.id),
                ..Default::default()
            },
            ArtifactScope::default(),
        )
        .unwrap();

    let board = d.decision_board(project, checkout).unwrap();
    assert_eq!(board.name, d.snapshot()[0].name);
    assert_eq!(
        board
            .tree()
            .iter()
            .map(|(depth, d)| (*depth, d.chose.as_str()))
            .collect::<Vec<_>>(),
        [(0, "sqlite"), (1, "wal mode")]
    );
    // Attribution is the point of allowing the write at all.
    assert_eq!(child.session.as_deref(), Some("sess-1"));
    assert!(
        child.checkout.is_some(),
        "and which checkout it was made in"
    );
    close_all(&d);
}

#[tokio::test]
async fn a_recorded_decision_is_pushed_at_every_attached_client() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_fake_claude(dir.path());
    let checkout = only_checkout(&d);
    let agent = d.spawn_agent(checkout, "claude").unwrap();
    open_feature(&d, agent, "notes storage");
    // Subscribed before the write, the way a connection is: a client that
    // has to ask again has already shown the operator a stale board.
    let mut rx = d.subscribe_decisions();

    d.record_agent_decision(
        agent,
        Some("sess-1"),
        DecisionWrite {
            chose: "sqlite".into(),
            ..Default::default()
        },
        ArtifactScope::default(),
    )
    .unwrap();

    let board = rx.try_recv().expect("the write is pushed, not waited for");
    assert_eq!(board.name, d.snapshot()[0].name);
    assert_eq!(board.project, Some(d.snapshot()[0].id));
    assert_eq!(
        board
            .decisions
            .iter()
            .map(|d| d.chose.as_str())
            .collect::<Vec<_>>(),
        ["sqlite"]
    );
    close_all(&d);
}

#[tokio::test]
async fn only_a_live_agent_may_record_a_decision() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_fake_claude(dir.path());
    let checkout = only_checkout(&d);
    let project = d.snapshot()[0].id;
    let shell = d.spawn_shell(checkout).unwrap();

    let write = DecisionWrite {
        chose: "sqlite".into(),
        ..Default::default()
    };
    assert!(
        d.record_agent_decision(shell, None, write.clone(), ArtifactScope::default())
            .is_err(),
        "a shell is not an agent"
    );
    assert!(
        d.record_agent_decision(PaneId(9999), None, write, ArtifactScope::default())
            .is_err(),
        "nor is nobody"
    );
    assert!(d
        .decision_board(project, checkout)
        .unwrap()
        .decisions
        .is_empty());
    close_all(&d);
}

#[tokio::test]
async fn superseding_leaves_the_decision_it_replaced_on_the_board() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_fake_claude(dir.path());
    let checkout = only_checkout(&d);
    let project = d.snapshot()[0].id;
    let agent = d.spawn_agent(checkout, "claude").unwrap();
    open_feature(&d, agent, "notes storage");

    let old = d
        .record_agent_decision(
            agent,
            None,
            DecisionWrite {
                chose: "key notes by id".into(),
                ..Default::default()
            },
            ArtifactScope::default(),
        )
        .unwrap();
    let new = d
        .record_agent_decision(
            agent,
            None,
            DecisionWrite {
                chose: "key notes by path".into(),
                because: Some("ids are handed out fresh every start".into()),
                supersedes: Some(old.id),
                ..Default::default()
            },
            ArtifactScope::default(),
        )
        .unwrap();

    let board = d.decision_board(project, checkout).unwrap();
    assert_eq!(
        board.decisions.len(),
        2,
        "the old one is marked, not removed"
    );
    let old = board.decisions.iter().find(|d| d.id == old.id).unwrap();
    assert_eq!(old.superseded_by, Some(new.id));
    close_all(&d);
}

#[tokio::test]
async fn the_board_reaches_an_agent_whole_and_a_bad_decision_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_claude_aliases(dir.path(), &["claude"]);
    d.start_hook_server().unwrap();
    let checkout = only_checkout(&d);
    let source = d.spawn_agent(checkout, "claude").unwrap();
    open_feature(&d, source, "notes storage");

    let recorded = String::from_utf8_lossy(
        &post_agent_hook(
            &d,
            source,
            Endpoint::Decide,
            r#"{"chose":"sqlite","over":"a file per feature"}"#,
        )
        .await,
    )
    .to_string();
    assert!(recorded.starts_with("HTTP/1.1 200 OK"), "{recorded}");
    assert!(recorded.contains(r#""chose":"sqlite""#), "{recorded}");

    let read = String::from_utf8_lossy(&post_agent_hook(&d, source, Endpoint::Decisions, "").await)
        .to_string();
    assert!(read.contains(r#""over":"a file per feature""#), "{read}");

    let garbage =
        String::from_utf8_lossy(&post_agent_hook(&d, source, Endpoint::Decide, "hello").await)
            .to_string();
    assert!(garbage.starts_with("HTTP/1.1 400"), "{garbage}");

    let empty = String::from_utf8_lossy(
        &post_agent_hook(&d, source, Endpoint::Decide, r#"{"chose":"  "}"#).await,
    )
    .to_string();
    assert!(empty.starts_with("HTTP/1.1 409"), "{empty}");
    assert!(
        empty.ends_with("a decision has to say what was chosen"),
        "{empty}"
    );

    let orphan = String::from_utf8_lossy(
        &post_agent_hook(&d, source, Endpoint::Decide, r#"{"chose":"x","under":99}"#).await,
    )
    .to_string();
    assert!(orphan.starts_with("HTTP/1.1 409"), "{orphan}");

    assert_eq!(
        d.decision_board(d.snapshot()[0].id, checkout)
            .unwrap()
            .decisions
            .len(),
        1,
        "only the one that was accepted"
    );
    close_all(&d);
}

#[tokio::test]
async fn a_person_accepts_a_feature_and_can_take_it_back() {
    use argus_protocol::FeatureState;

    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_fake_claude(dir.path());
    let checkout = only_checkout(&d);
    let project = d.snapshot()[0].id;
    let agent = d.spawn_agent(checkout, "claude").unwrap();
    let slug = open_feature(&d, agent, "streaming the pty");

    let state_of = |slug: &str| {
        d.decision_board(project, checkout)
            .unwrap()
            .features
            .iter()
            .find(|f| f.slug == slug)
            .map(|f| f.state)
            .unwrap()
    };
    assert_eq!(state_of(&slug), FeatureState::Open);

    d.move_feature_for_client(project, checkout, &slug, FeatureState::Done, None)
        .unwrap();
    assert_eq!(state_of(&slug), FeatureState::Done);

    // And it is reversible, which is the other half of a state a person
    // sets: an acceptance made in error must not need a new feature.
    d.move_feature_for_client(project, checkout, &slug, FeatureState::Open, None)
        .unwrap();
    assert_eq!(state_of(&slug), FeatureState::Open);
    close_all(&d);
}

#[tokio::test]
async fn tasks_belong_to_the_feature_the_checkout_is_on() {
    use argus_protocol::{TaskState, TaskWrite};

    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_fake_claude(dir.path());
    let checkout = only_checkout(&d);
    let agent = d.spawn_agent(checkout, "claude").unwrap();

    let refused = d
        .part_action_for_agent(
            agent,
            None,
            TaskAction::Add(TaskWrite {
                title: "port the parser".into(),
                external: None,
                parent: None,
            }),
            ArtifactScope::default(),
        )
        .unwrap_err()
        .to_string();
    assert!(refused.contains("not on a feature"), "{refused}");

    let pty = open_feature(&d, agent, "streaming the pty");
    let list = d
        .part_action_for_agent(
            agent,
            Some("sess-1"),
            TaskAction::Add(TaskWrite {
                title: "backpressure on the reader".into(),
                external: Some("ORION-412".into()),
                parent: None,
            }),
            ArtifactScope::default(),
        )
        .unwrap();
    assert_eq!(list.feature.as_deref(), Some(pty.as_str()));
    let id = list.tasks[0].id;
    assert_eq!(list.tasks[0].external.as_deref(), Some("ORION-412"));

    let list = d
        .part_action_for_agent(
            agent,
            Some("sess-1"),
            TaskAction::Add(TaskWrite {
                title: "bound the queue".into(),
                external: None,
                parent: Some(id),
            }),
            ArtifactScope::default(),
        )
        .unwrap();
    assert_eq!(
        list.tasks.iter().map(|task| task.id).collect::<Vec<_>>(),
        [id, list.tasks[1].id]
    );
    assert_eq!(list.tasks[1].parent, Some(id));

    let list = d
        .part_action_for_agent(
            agent,
            Some("sess-1"),
            TaskAction::Move {
                id,
                state: TaskState::Doing,
            },
            ArtifactScope::default(),
        )
        .unwrap();
    assert_eq!(list.tasks[0].claimed_by.as_deref(), Some("sess-1"));
    let list = d
        .part_action_for_agent(
            agent,
            None,
            TaskAction::SetBody {
                id,
                body: "Accept sustained output.\nVerify bounded buffering.".into(),
            },
            ArtifactScope::RepositoryBranch,
        )
        .unwrap();
    assert_eq!(
        list.tasks[0].body.as_deref(),
        Some("Accept sustained output.\nVerify bounded buffering.")
    );

    // Moving the checkout to another feature moves what it can see and
    // what it can touch, together.
    let notes = open_feature(&d, agent, "notes storage");
    let list = d
        .part_action_for_agent(agent, None, TaskAction::List, ArtifactScope::default())
        .unwrap();
    assert_eq!(list.feature.as_deref(), Some(notes.as_str()));
    assert!(
        list.tasks.is_empty(),
        "another feature's tasks are not this one's"
    );

    let refused = d
        .part_action_for_agent(
            agent,
            Some("sess-1"),
            TaskAction::Move {
                id,
                state: TaskState::Done,
            },
            ArtifactScope::default(),
        )
        .unwrap_err()
        .to_string();
    assert!(
        refused.contains("task") && refused.contains("not under this feature"),
        "a stale id cannot tick off another feature's work: {refused}"
    );
    let refused = d
        .part_action_for_agent(
            agent,
            None,
            TaskAction::SetBody {
                id,
                body: "rewrite the wrong task".into(),
            },
            ArtifactScope::RepositoryBranch,
        )
        .unwrap_err()
        .to_string();
    assert!(
        refused.contains("task") && refused.contains("not under this feature"),
        "a stale id cannot rewrite another feature's brief: {refused}"
    );

    // The order is the human's statement of what to do first.
    let refused = d
        .part_action_for_agent(
            agent,
            None,
            TaskAction::Reorder { id, to: 0 },
            ArtifactScope::default(),
        )
        .unwrap_err()
        .to_string();
    assert!(refused.contains("human's to set"), "{refused}");
    close_all(&d);
}

#[tokio::test]
async fn a_client_cannot_touch_a_row_under_a_feature_it_did_not_name() {
    use argus_protocol::{DiagramAction, DiagramWrite, TaskState, TaskWrite};

    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_fake_claude(dir.path());
    let checkout = only_checkout(&d);
    let project = d.snapshot()[0].id;
    let agent = d.spawn_agent(checkout, "claude").unwrap();
    let pty = open_feature(&d, agent, "streaming the pty");
    let task = d
        .part_action_for_agent(
            agent,
            None,
            TaskAction::Add(TaskWrite {
                title: "backpressure on the reader".into(),
                external: None,
                parent: None,
            }),
            ArtifactScope::default(),
        )
        .unwrap()
        .tasks[0]
        .id;
    let diagram = d
        .part_action_for_agent(
            agent,
            None,
            DiagramAction::Add(DiagramWrite {
                title: "reader".into(),
                body: "sequenceDiagram\n    A->>B: bytes".into(),
            }),
            ArtifactScope::default(),
        )
        .unwrap()
        .diagrams[0]
        .id;
    let notes = open_feature(&d, agent, "notes storage");

    // A client holding the other feature's view names ids from a list it
    // is no longer showing; the same guard an agent meets refuses it.
    let refused = d
        .part_action_for_client(
            project,
            checkout,
            &notes,
            TaskAction::Move {
                id: task,
                state: TaskState::Done,
            },
        )
        .unwrap_err()
        .to_string();
    assert!(refused.contains("task") && refused.contains("not under this feature"), "{refused}");
    let refused = d
        .part_action_for_client(project, checkout, &notes, DiagramAction::Remove { id: diagram })
        .unwrap_err()
        .to_string();
    assert!(refused.contains("diagram") && refused.contains("not under this feature"), "{refused}");

    // Named under its own feature, the same change goes through.
    let list = d
        .part_action_for_client(
            project,
            checkout,
            &pty,
            TaskAction::Move {
                id: task,
                state: TaskState::Done,
            },
        )
        .unwrap();
    assert_eq!(list.tasks[0].state, TaskState::Done);
    close_all(&d);
}

#[tokio::test]
async fn diagrams_belong_to_the_feature_the_checkout_is_on() {
    use argus_protocol::{DiagramAction, DiagramWrite};

    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_fake_claude(dir.path());
    let checkout = only_checkout(&d);
    let agent = d.spawn_agent(checkout, "claude").unwrap();

    let refused = d
        .part_action_for_agent(
            agent,
            None,
            DiagramAction::Add(DiagramWrite {
                title: "hook path".into(),
                body: "sequenceDiagram\n    A->>B: ping".into(),
            }),
            ArtifactScope::default(),
        )
        .unwrap_err()
        .to_string();
    assert!(refused.contains("not on a feature"), "{refused}");

    let slug = open_feature(&d, agent, "diagram hooks");
    let list = d
        .part_action_for_agent(
            agent,
            Some("sess-1"),
            DiagramAction::Add(DiagramWrite {
                title: "list and add".into(),
                body: "sequenceDiagram\n    Hook->>Daemon: POST /diagrams".into(),
            }),
            ArtifactScope::default(),
        )
        .unwrap();
    assert_eq!(list.feature.as_deref(), Some(slug.as_str()));
    let id = list.diagrams[0].id;
    assert_eq!(list.diagrams[0].title, "list and add");

    let list = d
        .part_action_for_agent(
            agent,
            None,
            DiagramAction::Remove { id },
            ArtifactScope::default(),
        )
        .unwrap();
    assert!(list.diagrams.is_empty(), "drop removes the row");
    close_all(&d);
}
