//! A comment on picked lines to an agent — from a review, a commit's review
//! or the Code stage, in either client: whether one may be written, which
//! agent hears it as agents come and go, the words each refusal is said in,
//! and the message that sends it. The daemon still decides; this keeps both
//! clients from sending what they can already tell it will refuse.

use crate::{CheckoutInfo, ClientMsg, PaneId, PaneInfo, PaneKind, PaneStatus, ReviewAnchor, MAX_REVIEW_COMMENT_BYTES};

/// A comment that passed: sent as it stands, or waiting on a choice
/// between the agents that could take it.
#[derive(Debug, Clone)]
pub enum Comment {
    Send(Box<ClientMsg>),
    Choose(Vec<PaneId>),
}

/// Said as the daemon's bound, which a test holds it to.
const TOO_LONG: &str = "the comment is longer than 4096 bytes";

/// The agents a comment in `checkout` can go to. A shell has nothing to
/// hear it with, and an exited agent nowhere to put it — the daemon's
/// `is_running_agent`, which refuses any other.
pub fn recipients(checkout: &CheckoutInfo) -> impl Iterator<Item = &PaneInfo> {
    checkout
        .listed_panes()
        .filter(|p| p.kind == PaneKind::Agent && !matches!(p.status, PaneStatus::Exited { .. }))
}

/// Whether a comment could be started: lines to anchor it to and an agent to read it.
pub fn can_comment(anchor: Option<&ReviewAnchor>, checkout: Option<&CheckoutInfo>) -> Result<(), &'static str> {
    if anchor.is_none() {
        return Err("pick a line to comment on");
    }
    if checkout.is_none_or(|c| recipients(c).next().is_none()) {
        return Err("no agent running in this checkout");
    }
    Ok(())
}

/// `body` on `anchor`, to `to` — or with nobody chosen, to the one agent
/// there is, or a choice between several — named `request_id`, so its save
/// and its refusal find the box that sent it. `checkout` is as the tree has
/// it now (None once it is gone): agents come and go while a comment is
/// written, and one chosen that has gone is refused, never swapped for
/// another. A choice carries no id: the send made after it names its own.
pub fn comment(
    anchor: Option<ReviewAnchor>,
    checkout: Option<&CheckoutInfo>,
    to: Option<PaneId>,
    body: &str,
    request_id: u64,
) -> Result<Comment, &'static str> {
    // Trimmed before it is measured, as the daemon measures it.
    let body = body.trim();
    if body.is_empty() {
        return Err("the comment is empty");
    }
    if body.len() > MAX_REVIEW_COMMENT_BYTES {
        return Err(TOO_LONG);
    }
    let anchor = anchor.ok_or("pick a line to comment on")?;
    let Some(checkout) = checkout else {
        return Err("no agent running in this checkout");
    };
    let live: Vec<PaneId> = recipients(checkout).map(|p| p.id).collect();
    let recipient = match (to, live.as_slice()) {
        (_, []) => return Err("no agent running in this checkout"),
        (Some(pane), _) if !live.contains(&pane) => return Err("that agent is no longer running"),
        (Some(pane), _) => pane,
        (None, [only]) => *only,
        (None, _) => return Ok(Comment::Choose(live)),
    };
    Ok(Comment::Send(Box::new(ClientMsg::ReviewComment {
        checkout: checkout.id,
        recipient,
        anchor: Box::new(anchor),
        body: body.to_owned(),
        request_id,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AgentTelemetry, CheckoutId, ReviewBase};

    const CO: CheckoutId = CheckoutId(100);

    fn pane(id: u64, kind: PaneKind, status: PaneStatus) -> PaneInfo {
        PaneInfo {
            id: PaneId(id),
            kind,
            title: format!("pane {id}"),
            status,
            note: None,
            template: None,
            children: Vec::new(),
            background: false,
            telemetry: AgentTelemetry::default(),
            since: None,
            live: false,
        }
    }

    fn checkout(panes: Vec<PaneInfo>) -> CheckoutInfo {
        CheckoutInfo { id: CO, name: "main".into(), path: "/src/main".into(), panes, git: None, primary: true }
    }

    fn sent(comment: Result<Comment, &str>) -> ClientMsg {
        match comment {
            Ok(Comment::Send(msg)) => *msg,
            other => panic!("not sent: {other:?}"),
        }
    }

    fn agent(id: u64) -> PaneInfo {
        pane(id, PaneKind::Agent, PaneStatus::Working)
    }

    /// Picked lines as a review gives them: both sides of a change.
    fn reviewed() -> ReviewAnchor {
        ReviewAnchor {
            base: ReviewBase::Unstaged,
            commit: None,
            path: "a.rs".into(),
            old_path: None,
            old_start: Some(2),
            old_end: Some(2),
            new_start: Some(2),
            new_end: Some(3),
            text: vec!["-old".into(), "+new".into(), "+more".into()],
        }
    }

    /// Picked lines as the Code stage gives them: the version's numbers only.
    fn read_in_code() -> ReviewAnchor {
        ReviewAnchor {
            base: ReviewBase::Commit,
            commit: Some("abc123".into()),
            path: "src/b.rs".into(),
            old_path: None,
            old_start: None,
            old_end: None,
            new_start: Some(5),
            new_end: Some(5),
            text: vec![" fn b() {}".into()],
        }
    }

    #[test]
    fn a_comment_needs_words_and_a_line() {
        let co = checkout(vec![agent(7)]);
        assert_eq!(can_comment(None, Some(&co)), Err("pick a line to comment on"));
        assert_eq!(comment(None, Some(&co), None, "tidy", 1).err(), Some("pick a line to comment on"));
        assert_eq!(comment(Some(reviewed()), Some(&co), None, "   ", 1).err(), Some("the comment is empty"));
    }

    #[test]
    fn a_comment_with_no_live_agent_is_refused() {
        let shells = checkout(vec![
            pane(1, PaneKind::Shell, PaneStatus::Idle),
            pane(2, PaneKind::Agent, PaneStatus::Exited { code: Some(0) }),
        ]);
        let a = reviewed();
        assert_eq!(can_comment(Some(&a), Some(&shells)), Err("no agent running in this checkout"));
        assert_eq!(comment(Some(a.clone()), Some(&shells), None, "hi", 1).err(), Some("no agent running in this checkout"));
        assert_eq!(
            comment(Some(a.clone()), Some(&shells), Some(PaneId(2)), "hi", 1).err(),
            Some("no agent running in this checkout")
        );
        assert_eq!(can_comment(Some(&a), None), Err("no agent running in this checkout"), "the checkout is gone");
        assert_eq!(comment(Some(a), None, None, "hi", 1).err(), Some("no agent running in this checkout"));
    }

    #[test]
    fn a_comment_goes_to_the_one_agent_or_asks_between_several() {
        let one = checkout(vec![pane(1, PaneKind::Shell, PaneStatus::Idle), agent(7)]);
        assert_eq!(can_comment(Some(&reviewed()), Some(&one)), Ok(()));
        let ClientMsg::ReviewComment { checkout: to, recipient, anchor, body, request_id } =
            sent(comment(Some(reviewed()), Some(&one), None, " tidy ", 1))
        else {
            panic!()
        };
        assert_eq!((to, recipient, body.as_str(), request_id), (CO, PaneId(7), "tidy", 1));
        assert_eq!(anchor.path, "a.rs");

        let several = checkout(vec![agent(7), pane(8, PaneKind::Agent, PaneStatus::Exited { code: None }), agent(9)]);
        assert!(matches!(comment(Some(reviewed()), Some(&several), None, "hi", 1), Ok(Comment::Choose(p)) if p == [PaneId(7), PaneId(9)]));
        assert!(matches!(
            sent(comment(Some(reviewed()), Some(&several), Some(PaneId(9)), "hi", 1)),
            ClientMsg::ReviewComment { recipient: PaneId(9), .. }
        ));
        assert_eq!(
            comment(Some(reviewed()), Some(&several), Some(PaneId(8)), "hi", 1).err(),
            Some("that agent is no longer running")
        );
    }

    #[test]
    fn a_comment_longer_than_the_daemon_takes_is_refused() {
        let co = checkout(vec![agent(7)]);
        let over = "x".repeat(MAX_REVIEW_COMMENT_BYTES + 1);
        assert_eq!(comment(Some(reviewed()), Some(&co), None, &over, 1).err(), Some(TOO_LONG));
        let full = "x".repeat(MAX_REVIEW_COMMENT_BYTES);
        sent(comment(Some(reviewed()), Some(&co), None, &full, 1));
        let padded = format!("  {full}  ");
        let ClientMsg::ReviewComment { body, .. } = sent(comment(Some(reviewed()), Some(&co), None, &padded, 1)) else {
            panic!()
        };
        assert_eq!(body.len(), MAX_REVIEW_COMMENT_BYTES, "the bound counts after the trim");
    }

    #[test]
    fn the_length_refusal_names_the_daemons_limit() {
        assert_eq!(TOO_LONG, format!("the comment is longer than {MAX_REVIEW_COMMENT_BYTES} bytes"));
    }

    #[test]
    fn a_review_anchor_and_a_code_anchor_are_refused_and_sent_alike() {
        let none = checkout(Vec::new());
        let one = checkout(vec![agent(7)]);
        for anchor in [reviewed(), read_in_code()] {
            assert_eq!(can_comment(Some(&anchor), Some(&none)), Err("no agent running in this checkout"));
            assert_eq!(comment(Some(anchor.clone()), Some(&none), None, "hi", 1).err(), Some("no agent running in this checkout"));
            assert_eq!(comment(Some(anchor.clone()), Some(&one), None, "  ", 1).err(), Some("the comment is empty"));
            assert_eq!(
                comment(Some(anchor.clone()), Some(&one), Some(PaneId(8)), "hi", 1).err(),
                Some("that agent is no longer running")
            );
            let ClientMsg::ReviewComment { recipient, anchor: rode, .. } =
                sent(comment(Some(anchor.clone()), Some(&one), None, "hi", 1))
            else {
                panic!()
            };
            assert_eq!((recipient, *rode), (PaneId(7), anchor), "the anchor rides through unchanged");
        }
    }

    #[test]
    fn the_message_names_the_checkout_its_agent_is_in() {
        let mut co = checkout(vec![agent(7)]);
        co.id = CheckoutId(42);
        let ClientMsg::ReviewComment { checkout: to, .. } = sent(comment(Some(reviewed()), Some(&co), None, "hi", 1)) else {
            panic!()
        };
        assert_eq!(to, CheckoutId(42));
    }

    #[test]
    fn the_message_names_the_request_it_was_given() {
        let co = checkout(vec![agent(7)]);
        let ClientMsg::ReviewComment { request_id, .. } = sent(comment(Some(reviewed()), Some(&co), None, "hi", 7)) else {
            panic!()
        };
        assert_eq!(request_id, 7);
    }

    #[test]
    fn a_chosen_agent_that_has_gone_is_refused_even_when_one_other_runs() {
        let co = checkout(vec![agent(9)]);
        assert_eq!(comment(Some(reviewed()), Some(&co), Some(PaneId(8)), "hi", 1).err(), Some("that agent is no longer running"));
    }
}
