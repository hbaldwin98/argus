//! The managed hook blocks a checkout carries while an agent is
//! running in it, and their removal when the last one leaves.

use super::*;
use argus_protocol::{ContextScope, DecisionWrite, TodoState, TodoWrite};
#[tokio::test]
async fn sharing_a_checkout_is_allowed_unless_the_project_says_otherwise() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_fake_claude(dir.path());
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

    let d = daemon_with_fake_claude(dir.path());
    d.sweep_stale_hooks();
    assert!(
        !settings_of(dir.path()).exists(),
        "a previous boot's hooks must not survive startup"
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

#[tokio::test]
async fn context_reads_the_project_and_checkout_notes_of_the_asking_agent() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_claude_aliases(dir.path(), &["claude"]);
    let checkout = only_checkout(&d);
    let project = d.snapshot()[0].id;
    d.set_note(
        NoteTarget::Project(project),
        "- [!] house style\n- [ ] outstanding\n".to_string(),
    )
    .unwrap();
    d.set_note(
        NoteTarget::Checkout(checkout),
        "# This branch\n- [!] leave the schema alone\n".to_string(),
    )
    .unwrap();
    let agent = d.spawn_agent(checkout, "claude").unwrap();

    let context = d.context_for_agent(agent).unwrap();

    assert_eq!(
        context
            .notes
            .iter()
            .map(|note| note.scope)
            .collect::<Vec<_>>(),
        [ContextScope::Project, ContextScope::Checkout],
        "outermost scope first"
    );
    assert!(context.notes[1].body.contains("# This branch"));
    assert_eq!(
        context
            .pinned()
            .map(|(_, todo)| todo.text.as_str())
            .collect::<Vec<_>>(),
        ["house style", "leave the schema alone"]
    );
    close_all(&d);
}

#[tokio::test]
async fn context_omits_a_note_that_was_never_written() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_claude_aliases(dir.path(), &["claude"]);
    let checkout = only_checkout(&d);
    d.set_note(NoteTarget::Checkout(checkout), "- [ ] just here\n".to_string())
        .unwrap();
    let agent = d.spawn_agent(checkout, "claude").unwrap();

    let context = d.context_for_agent(agent).unwrap();

    assert_eq!(context.notes.len(), 1, "the empty project note is left out");
    assert_eq!(context.notes[0].scope, ContextScope::Checkout);
    close_all(&d);
}

#[tokio::test]
async fn context_is_refused_to_anything_that_is_not_a_live_agent() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_claude_aliases(dir.path(), &["claude"]);
    let checkout = only_checkout(&d);
    let shell = d.spawn_shell(checkout).unwrap();

    assert!(d.context_for_agent(shell).is_err(), "a shell is not an agent");
    assert!(d.context_for_agent(PaneId(9999)).is_err(), "nor is nobody");
    close_all(&d);
}

#[tokio::test]
async fn note_text_can_be_forwarded_only_to_a_live_agent_in_scope() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_claude_aliases(dir.path(), &["claude"]);
    let checkout = only_checkout(&d);
    let agent = d.spawn_agent(checkout, "claude").unwrap();
    let shell = d.spawn_shell(checkout).unwrap();

    d.forward_note(
        NoteTarget::Checkout(checkout),
        agent,
        "# Exact markdown\n- [!] stay editable".to_string(),
    )
    .unwrap();
    assert!(d
        .forward_note(NoteTarget::Checkout(checkout), shell, "no".to_string())
        .unwrap_err()
        .to_string()
        .contains("live agent"));
    assert!(d
        .forward_note(NoteTarget::Checkout(checkout), agent, " \n ".to_string())
        .unwrap_err()
        .to_string()
        .contains("empty"));
    assert!(d
        .forward_note(
            NoteTarget::Checkout(checkout),
            agent,
            "x".repeat(MAX_NOTE_BYTES + 1),
        )
        .unwrap_err()
        .to_string()
        .contains("exceeds"));
    close_all(&d);
}

#[tokio::test]
async fn project_forwarding_crosses_checkouts_but_checkout_forwarding_does_not() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let d = daemon_with_two_agent_checkouts(first.path(), second.path());
    let snapshot = d.snapshot();
    let project = snapshot[0].id;
    let first_checkout = snapshot[0].repositories[0].checkouts[0].id;
    let second_checkout = snapshot[0].repositories[1].checkouts[0].id;
    let agent = d.spawn_agent(second_checkout, "claude").unwrap();

    assert!(d
        .forward_note(
            NoteTarget::Checkout(first_checkout),
            agent,
            "wrong checkout".to_string(),
        )
        .unwrap_err()
        .to_string()
        .contains("scope"));
    d.forward_note(
        NoteTarget::Project(project),
        agent,
        "project-wide context".to_string(),
    )
    .unwrap();

    let _ = d.close_pane(agent);
}

#[tokio::test]
async fn an_authorized_context_hook_returns_the_checkout_notes() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_claude_aliases(dir.path(), &["claude"]);
    d.start_hook_server().unwrap();
    let checkout = only_checkout(&d);
    d.set_note(
        NoteTarget::Checkout(checkout),
        "- [!] read me first\n".to_string(),
    )
    .unwrap();
    let source = d.spawn_agent(checkout, "claude").unwrap();

    let response = post_agent_hook(&d, source, Endpoint::Context, "").await;

    assert!(response.starts_with(b"HTTP/1.1 200 OK"));
    let body = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| &response[index + 4..])
        .unwrap();
    let context: argus_protocol::AgentContext = serde_json::from_slice(body).unwrap();
    assert_eq!(
        context
            .pinned()
            .map(|(_, todo)| todo.text.clone())
            .collect::<Vec<_>>(),
        ["read me first"]
    );
    close_all(&d);
}

#[tokio::test]
async fn an_agent_may_add_and_tick_off_items_where_the_project_allows_it() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_allowing_agent_todos(dir.path());
    let checkout = only_checkout(&d);
    let agent = d.spawn_agent(checkout, "claude").unwrap();

    let counts = d
        .write_agent_todo(
            agent,
            Some("sess-1"),
            &TodoWrite::Add {
                text: "ported the parser".into(),
            },
        )
        .unwrap();
    assert_eq!((counts.open, counts.done), (1, 0));

    let counts = d
        .write_agent_todo(
            agent,
            Some("sess-1"),
            &TodoWrite::Set {
                line: 0,
                state: TodoState::Done,
            },
        )
        .unwrap();
    assert_eq!((counts.open, counts.done), (0, 1));

    let note = d.note(NoteTarget::Checkout(checkout)).unwrap();
    assert_eq!(note.body, "- [x] ported the parser\n");
    // Both changes are accounted for, newest first, against the agent that
    // made them.
    assert_eq!(
        note.audit
            .iter()
            .map(|entry| (entry.action.as_str(), entry.detail.as_str()))
            .collect::<Vec<_>>(),
        [("done", "ported the parser"), ("add", "ported the parser")]
    );
    assert_eq!(note.audit[0].session.as_deref(), Some("sess-1"));
    close_all(&d);
}

#[tokio::test]
async fn an_agent_note_write_is_refused_unless_the_project_asked_for_it() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_claude_aliases(dir.path(), &["claude"]);
    let checkout = only_checkout(&d);
    let agent = d.spawn_agent(checkout, "claude").unwrap();

    let refusal = d
        .write_agent_todo(
            agent,
            None,
            &TodoWrite::Add {
                text: "unasked for".into(),
            },
        )
        .unwrap_err()
        .to_string();

    assert!(refusal.contains("does not allow"), "{refusal}");
    let note = d.note(NoteTarget::Checkout(checkout)).unwrap();
    assert_eq!(note.body, "", "a refused write leaves nothing behind");
    assert!(note.audit.is_empty(), "and records nothing");
    close_all(&d);
}

#[tokio::test]
async fn an_agent_may_not_touch_a_standing_instruction() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_allowing_agent_todos(dir.path());
    let checkout = only_checkout(&d);
    d.set_note(
        NoteTarget::Checkout(checkout),
        "- [!] leave the schema alone\n".to_string(),
    )
    .unwrap();
    let agent = d.spawn_agent(checkout, "claude").unwrap();

    for state in [TodoState::Done, TodoState::Open] {
        assert!(
            d.write_agent_todo(agent, None, &TodoWrite::Set { line: 0, state })
                .is_err(),
            "ticking off a pinned line would delete the instruction"
        );
    }
    assert!(
        d.write_agent_todo(
            agent,
            None,
            &TodoWrite::Set {
                line: 0,
                state: TodoState::Pinned,
            }
        )
        .is_err(),
        "nor may an agent pin anything"
    );
    let note = d.note(NoteTarget::Checkout(checkout)).unwrap();
    assert_eq!(note.body, "- [!] leave the schema alone\n");
    close_all(&d);
}

#[tokio::test]
async fn an_agent_note_write_reaches_only_its_own_live_checkout() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_allowing_agent_todos(dir.path());
    let checkout = only_checkout(&d);
    let project = d.snapshot()[0].id;
    d.set_note(NoteTarget::Project(project), "- [!] house style\n".to_string())
        .unwrap();
    let shell = d.spawn_shell(checkout).unwrap();

    let add = TodoWrite::Add {
        text: "from nowhere".into(),
    };
    assert!(
        d.write_agent_todo(shell, None, &add).is_err(),
        "a shell is not an agent"
    );
    assert!(
        d.write_agent_todo(PaneId(9999), None, &add).is_err(),
        "nor is nobody"
    );
    // The project note is unreachable by construction — there is no way to
    // name it — so what is asserted is that it stayed as the human left it.
    assert_eq!(
        d.note(NoteTarget::Project(project)).unwrap().body,
        "- [!] house style\n"
    );
    close_all(&d);
}

#[tokio::test]
async fn a_refused_todo_hook_says_why_and_a_bad_one_is_not_a_note_change() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_allowing_agent_todos(dir.path());
    d.start_hook_server().unwrap();
    let checkout = only_checkout(&d);
    let source = d.spawn_agent(checkout, "claude").unwrap();

    let written = String::from_utf8_lossy(
        &post_agent_hook(
            &d,
            source,
            Endpoint::Todo,
            r#"{"add":{"text":"through the wire"}}"#,
        )
        .await,
    )
    .to_string();
    assert!(written.starts_with("HTTP/1.1 200 OK"), "{written}");
    assert!(written.ends_with("1 open, 0 done"), "{written}");

    let garbage =
        String::from_utf8_lossy(&post_agent_hook(&d, source, Endpoint::Todo, "hello").await)
            .to_string();
    assert!(garbage.starts_with("HTTP/1.1 400"), "{garbage}");

    let missing = String::from_utf8_lossy(
        &post_agent_hook(&d, source, Endpoint::Todo, r#"{"set":{"line":7,"state":"done"}}"#).await,
    )
    .to_string();
    assert!(missing.starts_with("HTTP/1.1 409"), "{missing}");
    assert!(missing.ends_with("line 8 is not a checkbox"), "{missing}");

    assert_eq!(
        d.note(NoteTarget::Checkout(checkout)).unwrap().body,
        "- [ ] through the wire\n"
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
        )
        .unwrap();

    let board = d.decision_board(project).unwrap();
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
    assert!(child.checkout.is_some(), "and which checkout it was made in");
    close_all(&d);
}

#[tokio::test]
async fn a_recorded_decision_is_pushed_at_every_attached_client() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_fake_claude(dir.path());
    let checkout = only_checkout(&d);
    let agent = d.spawn_agent(checkout, "claude").unwrap();
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
    )
    .unwrap();

    let board = rx.try_recv().expect("the write is pushed, not waited for");
    assert_eq!(board.name, d.snapshot()[0].name);
    assert_eq!(board.project, Some(d.snapshot()[0].id));
    assert_eq!(
        board.decisions.iter().map(|d| d.chose.as_str()).collect::<Vec<_>>(),
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
        d.record_agent_decision(shell, None, write.clone()).is_err(),
        "a shell is not an agent"
    );
    assert!(
        d.record_agent_decision(PaneId(9999), None, write).is_err(),
        "nor is nobody"
    );
    assert!(d.decision_board(project).unwrap().decisions.is_empty());
    close_all(&d);
}

#[tokio::test]
async fn superseding_leaves_the_decision_it_replaced_on_the_board() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_fake_claude(dir.path());
    let checkout = only_checkout(&d);
    let project = d.snapshot()[0].id;
    let agent = d.spawn_agent(checkout, "claude").unwrap();

    let old = d
        .record_agent_decision(
            agent,
            None,
            DecisionWrite {
                chose: "key notes by id".into(),
                ..Default::default()
            },
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
        )
        .unwrap();

    let board = d.decision_board(project).unwrap();
    assert_eq!(board.decisions.len(), 2, "the old one is marked, not removed");
    let old = board.decisions.iter().find(|d| d.id == old.id).unwrap();
    assert_eq!(old.superseded_by, Some(new.id));
    close_all(&d);
}

#[tokio::test]
async fn the_board_reaches_an_agent_whole_and_a_bad_decision_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_fake_claude(dir.path());
    d.start_hook_server().unwrap();
    let checkout = only_checkout(&d);
    let source = d.spawn_agent(checkout, "claude").unwrap();

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
    assert!(empty.ends_with("a decision has to say what was chosen"), "{empty}");

    let orphan = String::from_utf8_lossy(
        &post_agent_hook(&d, source, Endpoint::Decide, r#"{"chose":"x","under":99}"#).await,
    )
    .to_string();
    assert!(orphan.starts_with("HTTP/1.1 409"), "{orphan}");

    assert_eq!(
        d.decision_board(d.snapshot()[0].id).unwrap().decisions.len(),
        1,
        "only the one that was accepted"
    );
    close_all(&d);
}
