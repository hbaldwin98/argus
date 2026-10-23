//! A review written as drafts and sent to an agent as one.

use argus_protocol::{DraftAction, PaneId};

use super::*;

fn add(line: u32, body: &str) -> DraftAction {
    DraftAction::Add { anchor: Box::new(review_anchor(line)), body: body.into() }
}

fn send(recipient: PaneId) -> DraftAction {
    DraftAction::Submit { recipient, summary: "Two things".into() }
}

#[tokio::test]
async fn drafts_are_held_until_the_review_is_sent_and_then_read_as_comments() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_claude_aliases(dir.path(), &["claude"]);
    let checkout = only_checkout(&d);
    let agent = d.spawn_agent(checkout, "claude").unwrap();

    d.review_draft(checkout, add(3, " why? ")).unwrap();
    let drafted = d.review_draft(checkout, add(8, "rename")).unwrap();
    assert_eq!(drafted.drafts.iter().map(|x| x.body.as_str()).collect::<Vec<_>>(), ["why?", "rename"]);
    assert!(d.review_comments_for_agent(agent).unwrap().is_empty(), "a draft is not sent");

    let sent = d.review_draft(checkout, send(agent)).unwrap();

    assert_eq!(sent.sent, Some((2, true)));
    assert!(sent.drafts.is_empty());
    let comments = d.review_comments_for_agent(agent).unwrap();
    assert_eq!(comments.iter().map(|c| c.body.as_str()).collect::<Vec<_>>(), ["why?", "rename"]);
    close_all(&d);
}

#[tokio::test]
async fn a_review_for_a_pane_that_is_no_live_agent_stays_a_draft() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_claude_aliases(dir.path(), &["claude"]);
    let checkout = only_checkout(&d);
    let shell = d.spawn_shell(checkout).unwrap();
    d.review_draft(checkout, add(3, "why?")).unwrap();

    assert!(d.review_draft(checkout, send(shell)).is_err());

    assert_eq!(d.review_draft(checkout, DraftAction::List).unwrap().drafts.len(), 1);
    close_all(&d);
}

#[tokio::test]
async fn an_empty_review_and_an_empty_comment_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_claude_aliases(dir.path(), &["claude"]);
    let checkout = only_checkout(&d);
    let agent = d.spawn_agent(checkout, "claude").unwrap();

    assert!(d.review_draft(checkout, send(agent)).unwrap_err().to_string().contains("no comments"));
    assert!(d.review_draft(checkout, add(1, "  \n ")).is_err());
    let id = d.review_draft(checkout, add(1, "fine")).unwrap().drafts[0].id;
    assert!(d.review_draft(checkout, DraftAction::Edit { id, body: " ".into() }).is_err());
    close_all(&d);
}

#[tokio::test]
async fn a_draft_is_edited_deleted_and_discarded_by_id() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_claude_aliases(dir.path(), &["claude"]);
    let checkout = only_checkout(&d);
    let id = d.review_draft(checkout, add(1, "one")).unwrap().drafts[0].id;
    d.review_draft(checkout, add(2, "two")).unwrap();

    let edited = d.review_draft(checkout, DraftAction::Edit { id, body: "first".into() }).unwrap();
    assert_eq!(edited.drafts[0].body, "first");
    let deleted = d.review_draft(checkout, DraftAction::Delete { id }).unwrap();
    assert_eq!(deleted.drafts.iter().map(|x| x.body.as_str()).collect::<Vec<_>>(), ["two"]);
    let error = d.review_draft(checkout, DraftAction::Delete { id }).unwrap_err();
    assert!(error.to_string().contains("no longer in the review"), "{error}");
    assert!(d.review_draft(checkout, DraftAction::Discard).unwrap().drafts.is_empty());
    close_all(&d);
}
