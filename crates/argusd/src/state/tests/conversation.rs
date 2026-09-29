//! A pane's conversation: what one read model over its files, its pushed
//! entries and its draft shows, which reports may write to it, what a
//! watcher is sent, and paging back through what came before.

use std::io::Write;
use std::time::Duration;

use argus_protocol::{Body, Draft, Earlier, Entry, ToolState, Update};

use super::*;
use crate::harness::transcript::Dialect;
use crate::state::conversation::Conversation;

/// One Claude prompt record, padded so a few of them fill a test window.
fn prompt(text: &str) -> String {
    serde_json::json!({
        "type": "user",
        "message": {"role": "user", "content": text},
        "padding": "x".repeat(200),
    })
    .to_string()
}

fn reply(text: &str) -> String {
    serde_json::json!({
        "type": "assistant",
        "message": {"role": "assistant", "content": [{"type": "text", "text": text}]},
    })
    .to_string()
}

fn write_lines(path: &std::path::Path, lines: &[String]) {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    for line in lines {
        writeln!(file, "{line}").unwrap();
    }
}

/// The text of every prompt and reply in `updates`, in order.
fn said(updates: &[Update]) -> Vec<String> {
    updates
        .iter()
        .filter_map(|update| match update {
            Update::Upsert(entry) => match &entry.body {
                Body::Prompt { text } | Body::Reply { text } => Some(text.clone()),
                Body::Divider { .. } => Some("--".to_string()),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

fn transcript(msg: ServerMsg) -> (bool, Option<Earlier>, Vec<Update>) {
    match msg {
        ServerMsg::Transcript {
            fresh,
            earlier,
            updates,
            ..
        } => (fresh, earlier, updates),
        other => panic!("not a transcript: {other:?}"),
    }
}

async fn next_update(rx: &mut broadcast::Receiver<ServerMsg>) -> (bool, Option<Earlier>, Vec<Update>) {
    let msg = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("an update within the poll")
        .expect("the feed is open");
    transcript(msg)
}

fn agent(dir: &std::path::Path) -> (Arc<Daemon>, PaneId) {
    let d = daemon_with_running_claude(dir);
    let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();
    (d, pane)
}

fn push(fresh: bool, texts: &[&str]) -> argus_protocol::Push {
    argus_protocol::Push {
        fresh,
        updates: texts
            .iter()
            .map(|text| {
                Update::Upsert(argus_protocol::Entry {
                    id: format!("id-{text}"),
                    at: None,
                    body: Body::Reply {
                        text: text.to_string(),
                    },
                })
            })
            .collect(),
    }
}

/// The updates a fresh tail of `conversation` is sent.
fn tail(conversation: &Conversation) -> Vec<Update> {
    conversation.tail().read().1
}

fn upsert(id: &str, body: Body) -> Update {
    Update::Upsert(Entry {
        id: id.to_string(),
        at: None,
        body,
    })
}

// --- the conversation itself, with no daemon ---------------------------

#[test]
fn a_fresh_push_replaces_everything_pushed_before() {
    let mut conversation = Conversation::new(PaneId(1));
    assert!(!conversation.offered());

    let push1 = push(true, &["one", "two"]);
    assert!(conversation.push(push1.fresh, push1.updates), "the first push offers it");
    let push2 = push(false, &["three"]);
    assert!(!conversation.push(push2.fresh, push2.updates), "offered once is enough");
    assert_eq!(said(&tail(&conversation)), ["one", "two", "three"]);

    let replay = push(true, &["again"]);
    conversation.push(replay.fresh, replay.updates);
    assert_eq!(said(&tail(&conversation)), ["again"]);
}

#[test]
fn a_pushed_conversation_keeps_only_its_recent_end() {
    let mut conversation = Conversation::new(PaneId(1));
    let texts: Vec<String> = (0..600).map(|n| format!("m{n}")).collect();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    conversation.push(false, push(false, &refs).updates);

    let held = said(&tail(&conversation));
    assert_eq!(held.len(), 500);
    assert_eq!(held.first().map(String::as_str), Some("m100"));
    assert_eq!(held.last().map(String::as_str), Some("m599"));
}

#[test]
fn a_pushed_entry_grows_by_its_appended_text_and_takes_its_tools_state() {
    let mut conversation = Conversation::new(PaneId(1));
    let call = Body::ToolCall {
        tool: "Bash".into(),
        summary: "cargo test".into(),
        input: String::new(),
        state: ToolState::Running,
    };
    conversation.push(
        false,
        vec![
            upsert("r", Body::Reply { text: "Build ".into() }),
            upsert("t", call),
            Update::AppendText { id: "r".into(), delta: "fixed.".into() },
            Update::ToolState { id: "t".into(), state: ToolState::Done },
            // Nothing by these ids was pushed; they change nothing.
            Update::AppendText { id: "gone".into(), delta: "lost".into() },
            Update::ToolState { id: "r".into(), state: ToolState::Failed },
        ],
    );

    let updates = tail(&conversation);
    assert_eq!(updates.len(), 2);
    assert_eq!(updates[0], upsert("r", Body::Reply { text: "Build fixed.".into() }));
    assert!(matches!(
        &updates[1],
        Update::Upsert(Entry { body: Body::ToolCall { state: ToolState::Done, .. }, .. })
    ));
}

#[test]
fn the_draft_comes_after_the_file_and_what_was_pushed_until_it_is_done() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("s1.jsonl");
    write_lines(&file, &[prompt("from the file")]);
    let mut conversation = Conversation::new(PaneId(1));
    conversation.name_file(file, Dialect::Claude);
    conversation.push(false, push(false, &["pushed"]).updates);

    conversation.draft(vec![Draft::Start { thinking: true }, Draft::More { text: "Weigh".into() }]);
    conversation.draft(vec![Draft::More { text: "ing".into() }]);
    let updates = tail(&conversation);
    assert_eq!(said(&updates), ["from the file", "pushed"]);
    assert_eq!(
        updates[2..],
        [
            Update::Draft(Draft::Start { thinking: true }),
            Update::Draft(Draft::More { text: "Weighing".into() }),
        ]
    );

    conversation.draft(vec![Draft::Done]);
    assert!(!tail(&conversation).iter().any(|u| matches!(u, Update::Draft(_))));
}

#[test]
fn a_draft_alone_is_no_conversation_to_offer() {
    let mut conversation = Conversation::new(PaneId(1));
    conversation.draft(vec![Draft::Start { thinking: false }]);
    assert!(!conversation.offered());
    assert_eq!(tail(&conversation), [
        Update::Draft(Draft::Start { thinking: false }),
        Update::Draft(Draft::More { text: String::new() }),
    ]);
}

#[test]
fn an_entry_both_read_and_pushed_is_shown_once_and_only_pushed_ones_still_show() {
    // Codex's live channel pushes items the rollout file later records, and
    // approvals it never does. A phone opening the pane afterwards sees each
    // item once and the question too.
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("s1.jsonl");
    write_lines(&file, &[prompt("from the file")]);
    let mut conversation = Conversation::new(PaneId(1));
    conversation.name_file(file, Dialect::Claude);
    let Update::Upsert(read) = &tail(&conversation)[0] else { panic!("an entry") };

    let mut pushed = push(false, &["only pushed"]).updates;
    pushed.push(upsert(&read.id, Body::Reply { text: "a live copy of the file's entry".into() }));
    conversation.push(false, pushed);

    assert_eq!(said(&tail(&conversation)), ["from the file", "only pushed"]);
}

#[test]
fn the_file_named_last_is_read_below_a_divider_and_naming_it_again_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("s1.jsonl");
    let second = dir.path().join("s2.jsonl");
    write_lines(&first, &[prompt("old")]);
    write_lines(&second, &[prompt("after clear")]);
    let mut conversation = Conversation::new(PaneId(1));

    assert!(conversation.name_file(first, Dialect::Claude), "the first file offers it");
    assert!(!conversation.name_file(second.clone(), Dialect::Claude));
    assert!(!conversation.name_file(second, Dialect::Claude));

    assert_eq!(said(&tail(&conversation)), ["--", "after clear"]);
}

#[tokio::test]
async fn followers_are_sent_what_is_pushed_and_drafted() {
    let mut conversation = Conversation::new(PaneId(7));
    // Nobody follows yet: nothing to send to, and nothing lost by it.
    conversation.push(false, push(false, &["before"]).updates);
    let mut rx = conversation.follow(None, || None);

    conversation.push(false, push(false, &["after"]).updates);
    conversation.draft(vec![Draft::Start { thinking: false }]);

    let (fresh, _, updates) = next_update(&mut rx).await;
    assert!(!fresh);
    assert_eq!(said(&updates), ["after"]);
    let (_, _, updates) = next_update(&mut rx).await;
    assert_eq!(updates, [Update::Draft(Draft::Start { thinking: false })]);
}

#[tokio::test]
async fn the_followers_stop_when_the_last_watcher_goes() {
    let mut conversation = Conversation::new(PaneId(1));
    let _a = conversation.follow(None, || None);
    let _b = conversation.follow(None, || None);
    conversation.leave();
    assert!(conversation.followed());
    conversation.leave();
    assert!(!conversation.followed());
}

#[tokio::test]
async fn dropping_a_conversation_ends_every_watch_of_it() {
    let mut conversation = Conversation::new(PaneId(1));
    let mut rx = conversation.follow(None, || None);
    drop(conversation);
    let ended = tokio::time::timeout(Duration::from_secs(5), rx.recv()).await;
    assert!(matches!(ended, Ok(Err(broadcast::error::RecvError::Closed))), "the watch outlived it");
}

// --- the conversation on a daemon's panes -------------------------------


#[tokio::test]
async fn a_pane_offers_its_conversation_once_its_own_agent_names_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    let file = dir.path().join("s1.jsonl");
    d.set_pane_session_id(pane, "s1");
    assert!(!pane_info(&d, pane).has_transcript);

    // A CLI started inside the pane inherits its hook environment; its
    // conversation is not the pane's.
    d.report_transcript(pane, Some("child"), &dir.path().join("child.jsonl").to_string_lossy());
    assert!(!pane_info(&d, pane).has_transcript);
    d.report_transcript(pane, Some("s1"), "relative/s1.jsonl");
    assert!(!pane_info(&d, pane).has_transcript);

    d.report_transcript(pane, Some("s1"), &file.to_string_lossy());
    assert!(pane_info(&d, pane).has_transcript);
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn a_shell_has_no_conversation_to_offer() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_fake_claude(dir.path());
    let shell = d.spawn_shell(only_checkout(&d)).unwrap();
    d.report_transcript(shell, None, &dir.path().join("x.jsonl").to_string_lossy());
    assert!(!pane_info(&d, shell).has_transcript);
    let _ = d.close_pane(shell);
}

#[tokio::test]
async fn a_watcher_is_sent_the_tail_then_each_line_as_it_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    let file = dir.path().join("s1.jsonl");
    write_lines(&file, &[prompt("first"), reply("hello")]);
    d.report_transcript(pane, None, &file.to_string_lossy());

    let mut rx = d.watch_transcript(pane);
    let (fresh, earlier, updates) = transcript(d.transcript_tail(pane));
    assert!(fresh);
    assert_eq!(earlier, None, "the whole file fits");
    assert_eq!(said(&updates), ["first", "hello"]);

    write_lines(&file, &[reply("more")]);
    let (fresh, _, updates) = next_update(&mut rx).await;
    assert!(!fresh);
    assert_eq!(said(&updates), ["more"]);
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn a_line_still_being_written_is_sent_once_it_is_finished() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    let file = dir.path().join("s1.jsonl");
    write_lines(&file, &[prompt("first")]);
    let half = reply("finished later");
    let (head, rest) = half.split_at(half.len() / 2);
    std::fs::OpenOptions::new()
        .append(true)
        .open(&file)
        .unwrap()
        .write_all(head.as_bytes())
        .unwrap();
    d.report_transcript(pane, None, &file.to_string_lossy());

    let mut rx = d.watch_transcript(pane);
    assert_eq!(said(&transcript(d.transcript_tail(pane)).2), ["first"]);

    std::fs::OpenOptions::new()
        .append(true)
        .open(&file)
        .unwrap()
        .write_all(format!("{rest}\n").as_bytes())
        .unwrap();
    assert_eq!(said(&next_update(&mut rx).await.2), ["finished later"]);
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn a_pane_watched_before_it_names_a_file_is_sent_that_file_fresh() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    let mut rx = d.watch_transcript(pane);
    assert!(said(&transcript(d.transcript_tail(pane)).2).is_empty());

    let file = dir.path().join("s1.jsonl");
    write_lines(&file, &[prompt("typed before the first hook")]);
    d.report_transcript(pane, None, &file.to_string_lossy());

    let (fresh, _, updates) = next_update(&mut rx).await;
    assert!(fresh);
    assert_eq!(said(&updates), ["typed before the first hook"]);
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn a_new_conversation_is_announced_below_the_old_one() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    let first = dir.path().join("s1.jsonl");
    let second = dir.path().join("s2.jsonl");
    write_lines(&first, &[prompt("old")]);
    write_lines(&second, &[prompt("after clear")]);
    d.report_transcript(pane, None, &first.to_string_lossy());

    let mut rx = d.watch_transcript(pane);
    let _ = d.transcript_tail(pane);
    d.report_transcript(pane, None, &second.to_string_lossy());

    let (fresh, _, updates) = next_update(&mut rx).await;
    assert!(!fresh, "the old conversation stays above");
    assert_eq!(said(&updates)[..2], ["--", "after clear"]);
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn paging_back_reaches_the_start_across_every_file_the_pane_used() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    let first = dir.path().join("s1.jsonl");
    let second = dir.path().join("s2.jsonl");
    let old: Vec<String> = (0..12).map(|n| format!("old {n}")).collect();
    let new: Vec<String> = (0..12).map(|n| format!("new {n}")).collect();
    write_lines(&first, &old.iter().map(|t| prompt(t)).collect::<Vec<_>>());
    write_lines(&second, &new.iter().map(|t| prompt(t)).collect::<Vec<_>>());
    d.report_transcript(pane, None, &first.to_string_lossy());
    d.report_transcript(pane, None, &second.to_string_lossy());

    let (_, mut earlier, updates) = transcript(d.transcript_tail(pane));
    let mut held = said(&updates);
    let mut pages = 0;
    while let Some(before) = earlier {
        let ServerMsg::EarlierTranscript { earlier: next, updates, .. } =
            d.earlier_transcript(pane, before)
        else {
            panic!("an earlier page");
        };
        let mut page = said(&updates);
        page.append(&mut held);
        held = page;
        earlier = next;
        pages += 1;
        assert!(pages < 100, "paging never reached the start");
    }

    let mut expected = old.clone();
    expected.push("--".to_string());
    expected.extend(new);
    // A window can end inside a line, which the next page then carries.
    held.dedup();
    assert_eq!(held, expected);
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn closing_a_pane_ends_every_watch_of_it() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    let mut rx = d.watch_transcript(pane);
    d.close_pane(pane).unwrap();
    let ended = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Err(broadcast::error::RecvError::Closed) = rx.recv().await {
                break;
            }
        }
    })
    .await;
    assert!(ended.is_ok(), "the watch outlived its pane");
}

#[tokio::test]
async fn a_new_session_names_its_file_on_the_same_report_that_claims_the_pane() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // Claude's SessionStart after /clear carries the new session and its
    // new file together. The claim has to land first, or the file would be
    // judged as coming from someone else's conversation.
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    d.start_hook_server().unwrap();
    d.set_pane_session_id(pane, "s1");
    let file = dir.path().join("s2.jsonl");

    let request = format!(
        "POST {} HTTP/1.1\r\nAuthorization: Bearer {}\r\n{}: s2\r\n{}: {}\r\nContent-Length: 2\r\n\r\ns2",
        argus_protocol::pane_path(pane, argus_protocol::Endpoint::Session),
        d.hook_token,
        argus_protocol::SESSION_HEADER,
        argus_protocol::TRANSCRIPT_HEADER,
        file.display(),
    );
    let port = d.hook_port.load(std::sync::atomic::Ordering::Relaxed);
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();

    assert!(response.starts_with(b"HTTP/1.1 200 OK"));
    assert!(pane_info(&d, pane).has_transcript);
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn only_the_panes_own_agent_can_push_its_conversation() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    d.set_pane_session_id(pane, "s1");

    // A CLI inside the pane is not the pane's conversation.
    d.report_pushed(pane, Some("child"), push(false, &["intruder"]));
    assert!(!pane_info(&d, pane).has_transcript);

    d.report_pushed(pane, Some("s1"), push(true, &["one"]));
    assert!(pane_info(&d, pane).has_transcript);
    d.report_pushed(pane, Some("child"), push(false, &["intruder"]));
    assert_eq!(said(&transcript(d.transcript_tail(pane)).2), ["one"]);
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn a_watch_of_a_pane_that_is_gone_ends_at_once() {
    let d = daemon_with_primary("/tmp/argus-test-conversation");
    let mut rx = d.watch_transcript(PaneId(99));
    assert!(matches!(rx.recv().await, Err(broadcast::error::RecvError::Closed)));
    assert!(!d.transcript_watched(PaneId(99)));
}

#[tokio::test]
async fn a_push_bigger_than_an_ordinary_hook_is_taken_by_the_pane_api() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    d.start_hook_server().unwrap();
    let long = "x".repeat(20_000);
    let body = serde_json::to_string(&push(false, &[long.as_str()])).unwrap();

    let response = post_agent_hook(&d, pane, argus_protocol::Endpoint::Transcript, &body).await;

    assert!(response.starts_with(b"HTTP/1.1 200 OK"), "{}", String::from_utf8_lossy(&response));
    assert!(pane_info(&d, pane).has_transcript);
    let _ = d.close_pane(pane);
}
