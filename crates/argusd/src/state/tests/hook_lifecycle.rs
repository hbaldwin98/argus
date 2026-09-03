//! The managed hook blocks a checkout carries while an agent is
//! running in it, and their removal when the last one leaves.

use super::*;
use argus_protocol::ContextScope;
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
