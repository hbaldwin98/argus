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
    // The clients show this text as it is: it is all they learn of who
    // holds the checkout.
    assert!(err.starts_with("claude is already working here"), "name the agent already there: {err:?}");
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
        .feature_action_for_agent(
            first_agent,
            None,
            argus_protocol::FeatureAction::Open(argus_protocol::FeatureWrite {
                title: "workspace migration".into(),
                body: None,
            }),
            ArtifactScope::Workspace,
        )
        .unwrap()
        .current
        .unwrap();
    let select = argus_protocol::FeatureAction::Select { slug: shared };
    d.feature_action_for_agent(second_agent, None, select, ArtifactScope::Workspace)
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

// --- the decision board -------------------------------------------------

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

/// The board rules are tested on a hand-made address; this is what makes
/// that address the one both sides would have resolved.
#[tokio::test]
async fn an_agent_and_a_client_at_one_checkout_address_one_board() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_fake_claude(dir.path());
    let checkout = only_checkout(&d);
    let project = d.snapshot()[0].id;
    let agent = d.spawn_agent(checkout, "claude").unwrap();

    let from_agent = d
        .address_for_agent(agent, ArtifactScope::RepositoryBranch)
        .unwrap();
    let from_client = d.address_for_client(project, checkout).unwrap();
    assert_eq!(from_agent.key, from_client.key);
    assert_eq!(from_agent.project, project);
    assert_eq!(from_client.project, project);
    assert_eq!(from_agent.project_name, from_client.project_name);
    assert_eq!(from_agent.repositories, from_client.repositories);
    assert_eq!(
        from_client.repositories,
        [d.snapshot()[0].repositories[0].id]
    );
    assert_eq!(from_agent.checkout_path, from_client.checkout_path);

    open_feature(&d, agent, "notes storage");
    let mut rx = d.subscribe_decisions();
    let write = DecisionWrite {
        chose: "sqlite".into(),
        ..Default::default()
    };
    d.record_agent_decision(agent, None, write, ArtifactScope::default())
        .unwrap();
    let pushed = rx.try_recv().expect("the write is pushed");
    assert_eq!(pushed.project, Some(project));
    let board = d.decision_board(project, checkout).unwrap();
    assert_eq!(board.decisions[0].chose, "sqlite", "and the client reads it");
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

// --- open decisions, and what a task says about its own progress ----------

#[tokio::test]
async fn an_open_question_reaches_an_agent_as_a_question_and_is_answered_once() {
    use argus_protocol::{DecisionChange, DecisionState};

    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_claude_aliases(dir.path(), &["claude"]);
    d.start_hook_server().unwrap();
    let checkout = only_checkout(&d);
    let agent = d.spawn_agent(checkout, "claude").unwrap();
    open_feature(&d, agent, "notes storage");
    let post = |endpoint, body: String| {
        let d = d.clone();
        async move { String::from_utf8_lossy(&post_agent_hook(&d, agent, endpoint, &body).await).to_string() }
    };

    let asked = post(
        Endpoint::Decide,
        r#"{"chose":"which store?","state":"open","options":["sqlite","a file per feature"]}"#.into(),
    )
    .await;
    assert!(asked.starts_with("HTTP/1.1 200 OK"), "{asked}");
    let read = d.decisions_for_agent(agent, ArtifactScope::default()).unwrap();
    let question = &read.decisions[0];
    assert!(question.open(), "the agent is shown it, and shown it as not decided");
    assert_eq!(question.options, ["sqlite", "a file per feature"]);
    let board = d.feature_board_for_agent(agent, ArtifactScope::default()).unwrap();
    assert_eq!(board.decisions[0].state, DecisionState::Open);

    // Options belong to a question; a made decision carrying them is told so.
    let muddled = post(Endpoint::Decide, r#"{"chose":"sqlite","options":["x"]}"#.into()).await;
    assert!(muddled.starts_with("HTTP/1.1 409") && muddled.contains("only an open decision"), "{muddled}");

    let resolve = |answer: &str| {
        serde_json::to_string(&DecisionChange::Resolve {
            id: question.id,
            answer: answer.into(),
            because: Some("both need migrations".into()),
        })
        .unwrap()
    };
    let answered = post(Endpoint::DecisionChange, resolve("sqlite")).await;
    assert!(answered.starts_with("HTTP/1.1 200 OK"), "{answered}");
    let made = d.decisions_for_agent(agent, ArtifactScope::default()).unwrap().decisions.remove(0);
    assert!(made.stands());
    assert_eq!(
        (made.detail.as_deref(), made.over.as_deref(), made.because.as_deref()),
        (Some("sqlite"), Some("a file per feature"), Some("both need migrations"))
    );

    let again = post(Endpoint::DecisionChange, resolve("a file per feature")).await;
    assert!(again.starts_with("HTTP/1.1 409") && again.contains("not an open question"), "{again}");
    close_all(&d);
}

#[tokio::test]
async fn a_decision_full_to_its_bounds_fits_through_the_pane_api() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_claude_aliases(dir.path(), &["claude"]);
    d.start_hook_server().unwrap();
    let agent = d.spawn_agent(only_checkout(&d), "claude").unwrap();
    open_feature(&d, agent, "notes storage");
    let full = "x".repeat(argus_protocol::MAX_DECISION_BYTES);
    let write = DecisionWrite {
        chose: full.clone(),
        context: Some(full.clone()),
        detail: Some(full.clone()),
        because: Some(full.clone()),
        over: Some(full.clone()),
        consequences: vec![
            argus_protocol::Consequence { gain: false, text: full.clone() };
            argus_protocol::MAX_DECISION_CONSEQUENCES
        ],
        ..Default::default()
    };

    let body = serde_json::to_string(&write).unwrap();
    let recorded =
        String::from_utf8_lossy(&post_agent_hook(&d, agent, Endpoint::Decide, &body).await).to_string();

    assert!(recorded.starts_with("HTTP/1.1 200 OK"), "{}", &recorded[..recorded.len().min(200)]);
    close_all(&d);
}

// A view in one project named an agent at work in another and could not
// link it, since nothing told the writer that agent's pane.
#[tokio::test]
async fn the_agents_hook_lists_the_other_agents_of_every_project() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_claude_aliases(dir.path(), &["claude"]);
    let other = tempfile::tempdir().unwrap();
    let elsewhere = other.path().join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    let _repo = real_repo(&elsewhere);
    with_temp_config(|_| d.add_project(&other.path().to_string_lossy(), None, &[]).unwrap());
    d.start_hook_server().unwrap();
    let tree = d.snapshot();
    assert_eq!(tree.len(), 2);
    let asker = d.spawn_agent(tree[0].repositories[0].checkouts[0].id, "claude").unwrap();
    let away = d.spawn_agent(tree[1].repositories[0].checkouts[0].id, "claude").unwrap();
    let shell = d.spawn_shell(tree[0].repositories[0].checkouts[0].id).unwrap();

    let response = post_agent_hook(&d, asker, Endpoint::Agents, "").await;

    assert!(response.starts_with(b"HTTP/1.1 200 OK"));
    let body = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| &response[index + 4..])
        .unwrap();
    let agents: Vec<argus_protocol::AgentListing> = serde_json::from_slice(body).unwrap();
    assert_eq!(agents.iter().map(|a| a.pane).collect::<Vec<_>>(), vec![away], "not itself, not a shell");
    assert_eq!(agents[0].repository, "elsewhere");
    assert!(!agents[0].here);
    assert!(d.agents_for_agent(shell).is_err(), "a shell is not an agent to answer");
    for pane in [asker, away, shell] {
        let _ = d.close_pane(pane);
    }
}
