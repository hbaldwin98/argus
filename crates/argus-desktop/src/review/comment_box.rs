//! The comment box the review and the Code stage share: whether it is open,
//! the agent it is lit on, the comment it sent and waits on the daemon to
//! save or refuse, by its id, and what the page draws of it. Whether a
//! comment may go, and to whom, is `argus_protocol::review::agent_comment`'s.

use argus_protocol::review::agent_comment::{self, Comment};
use argus_protocol::{CheckoutInfo, ClientMsg, PaneId, RequestRefusal, ReviewAnchor};
use serde::Serialize;

/// Said when a second comment is sent from a box still waiting on its first.
const WAITING: &str = "the last comment is still being saved";

#[derive(Debug, Default)]
pub struct CommentBox {
    open: bool,
    /// The agent picked, or lit when the box opened; kept for the next box
    /// while it still runs.
    to: Option<PaneId>,
    /// The comment sent from this box the daemon has yet to answer, by the
    /// id it was sent under; zero once an older daemon's bare error may have
    /// refused it (see `refused`).
    sending: Option<u64>,
}

impl CommentBox {
    /// `c`: opens under the picked lines when an agent is there to read
    /// it, lit on the one chosen before while it runs, else the first that
    /// does, so the person sees who it goes to before typing. Already open,
    /// it keeps what it was sending, and changes agent only when that one
    /// has gone: a gesture the person sees answered before anything goes.
    pub fn open(&mut self, anchor: Option<&ReviewAnchor>, checkout: Option<&CheckoutInfo>) -> Result<(), &'static str> {
        agent_comment::can_comment(anchor, checkout)?;
        let live: Vec<PaneId> = checkout.into_iter().flat_map(agent_comment::recipients).map(|p| p.id).collect();
        if !self.to.is_some_and(|p| live.contains(&p)) {
            self.to = live.first().copied();
        }
        self.open = true;
        Ok(())
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn choose(&mut self, pane: PaneId) {
        self.to = Some(pane);
    }

    /// Escape, Cancel, or the lines under it gone: shut unsent. Closing
    /// forgets that it sent one.
    pub fn close(&mut self) {
        self.open = false;
        self.sending = None;
    }

    /// The comment as typed, to the agent lit, named `request_id`. Refused
    /// while shut — the lines under the box may have changed since it
    /// closed — and while one sent from it waits: what the daemon says of
    /// that one is how the box learns to let its words go.
    pub fn send(
        &mut self,
        anchor: Option<ReviewAnchor>,
        checkout: Option<&CheckoutInfo>,
        body: &str,
        request_id: u64,
    ) -> Result<ClientMsg, &'static str> {
        if !self.open {
            return Err("no comment box is open");
        }
        if matches!(self.sending, Some(id) if id != 0) {
            return Err(WAITING);
        }
        match agent_comment::comment(anchor, checkout, self.to, body, request_id)? {
            Comment::Send(msg) => {
                self.sending = Some(request_id);
                Ok(*msg)
            }
            // `open` lights an agent, so an open box always names one.
            Comment::Choose(_) => Err("choose the agent to send it to"),
        }
    }

    /// The daemon saved the comment sent under `request_id`. True when this
    /// box sent it: it shuts, and the caller lets its lines go. A daemon
    /// from before comments were named says zero, naming none, and a box
    /// waiting on a comment takes it, as every box once did.
    pub fn saved(&mut self, request_id: u64) -> bool {
        let mine = self.sending.is_some_and(|id| request_id == id || request_id == 0);
        if mine {
            self.sending = None;
            self.open = false;
        }
        mine
    }

    /// A request was refused. True when it may have been this box's
    /// comment: refused by its id, the box stays open with its words, to be
    /// sent again. An older daemon's bare error names no request: it may
    /// have been this comment's, so the box may send again, but a save
    /// naming none still shuts it.
    pub fn refused(&mut self, refusal: &RequestRefusal) -> bool {
        if !self.sending.is_some_and(|id| refusal.refuses_change(id)) {
            return false;
        }
        self.sending = (refusal.request_id == 0).then_some(0);
        true
    }

    /// A new connection: the daemon on it answers nothing sent on the last,
    /// so the box may send again. Whether that comment was saved is not
    /// known, and the words stay.
    pub fn forget_sent(&mut self) {
        self.sending = None;
    }

    /// The box as the page draws it, while it is open.
    pub fn page(&self, checkout: Option<&CheckoutInfo>, about: impl FnOnce() -> String) -> Option<CommentPage> {
        self.open.then(|| page(checkout, self.to, about()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CommentPage {
    /// What the box is on, as its header says it: `new lines 39–40 · unstaged`,
    /// `lines 3–5 · src/a.rs`.
    pub about: String,
    /// The agents in the checkout it could go to.
    pub recipients: Vec<Recipient>,
    /// The one it goes to, while that one still runs; none lit means one is
    /// to be chosen.
    pub to: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Recipient {
    pub pane: u64,
    pub title: String,
}

/// The box on `about`, offering the live agents of `checkout` and lit on
/// `to` only while it is one of them.
fn page(checkout: Option<&CheckoutInfo>, to: Option<PaneId>, about: String) -> CommentPage {
    let offered: Vec<Recipient> = checkout
        .into_iter()
        .flat_map(agent_comment::recipients)
        .map(|p| Recipient { pane: p.id.0, title: p.title.clone() })
        .collect();
    let to = to.filter(|p| offered.iter().any(|r| r.pane == p.0)).map(|p| p.0);
    CommentPage { about, recipients: offered, to }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{checkout, pane};
    use argus_protocol::{PaneInfo, PaneKind, PaneStatus, ReviewBase};

    fn agent(id: u64) -> PaneInfo {
        pane(id, PaneKind::Agent, &format!("agent {id}"), PaneStatus::Working)
    }

    fn gone(id: u64) -> PaneInfo {
        pane(id, PaneKind::Agent, &format!("agent {id}"), PaneStatus::Exited { code: Some(0) })
    }

    fn lines() -> ReviewAnchor {
        ReviewAnchor {
            base: ReviewBase::Unstaged,
            commit: None,
            path: "a.rs".into(),
            old_path: None,
            old_start: None,
            old_end: None,
            new_start: Some(1),
            new_end: Some(1),
            text: vec!["+new".into()],
        }
    }

    fn recipient(msg: Result<ClientMsg, &str>) -> PaneId {
        match msg {
            Ok(ClientMsg::ReviewComment { recipient, .. }) => recipient,
            other => panic!("not sent: {other:?}"),
        }
    }

    #[test]
    fn a_box_opens_lit_on_the_first_live_agent() {
        let co = checkout(100, "main", true, vec![gone(6), agent(7), agent(8)]);
        let mut b = CommentBox::default();
        b.open(Some(&lines()), Some(&co)).unwrap();
        assert!(b.is_open());
        assert_eq!(b.page(Some(&co), String::new).unwrap().to, Some(7), "the exited agent ahead of it is skipped");
        assert_eq!(recipient(b.send(Some(lines()), Some(&co), "hi", 1)), PaneId(7));
    }

    #[test]
    fn a_box_keeps_the_agent_chosen_across_comments_while_it_runs() {
        let co = checkout(100, "main", true, vec![agent(7), agent(8)]);
        let mut b = CommentBox::default();
        b.open(Some(&lines()), Some(&co)).unwrap();
        b.choose(PaneId(8));
        assert_eq!(recipient(b.send(Some(lines()), Some(&co), "one", 1)), PaneId(8));
        assert!(b.saved(1));
        b.open(Some(&lines()), Some(&co)).unwrap();
        assert_eq!(recipient(b.send(Some(lines()), Some(&co), "two", 2)), PaneId(8));
    }

    #[test]
    fn an_agent_chosen_that_has_since_exited_is_refused_not_replaced() {
        let mut co = checkout(100, "main", true, vec![agent(7), agent(8)]);
        let mut b = CommentBox::default();
        b.open(Some(&lines()), Some(&co)).unwrap();
        b.choose(PaneId(8));
        co.panes[1] = gone(8);
        assert_eq!(b.send(Some(lines()), Some(&co), "hi", 1).err(), Some("that agent is no longer running"));
        assert!(b.is_open(), "the box stays for someone else to be chosen");
    }

    #[test]
    fn a_box_lights_nobody_once_its_agent_has_gone() {
        let mut co = checkout(100, "main", true, vec![agent(7), agent(8)]);
        let mut b = CommentBox::default();
        b.open(Some(&lines()), Some(&co)).unwrap();
        b.choose(PaneId(8));
        co.panes[1] = gone(8);
        let drawn = b.page(Some(&co), String::new).unwrap();
        assert_eq!(drawn.to, None);
        assert_eq!(drawn.recipients.iter().map(|r| r.pane).collect::<Vec<_>>(), [7]);
    }

    #[test]
    fn reopened_after_its_agent_went_a_box_lights_a_live_one() {
        let mut co = checkout(100, "main", true, vec![agent(7), agent(8)]);
        let mut b = CommentBox::default();
        b.open(Some(&lines()), Some(&co)).unwrap();
        b.choose(PaneId(8));
        b.close();
        co.panes[1] = gone(8);
        b.open(Some(&lines()), Some(&co)).unwrap();
        assert_eq!(b.page(Some(&co), String::new).unwrap().to, Some(7));
    }

    #[test]
    fn a_save_shuts_only_the_box_whose_comment_it_names() {
        let co = checkout(100, "main", true, vec![agent(7)]);
        let (mut first, mut second, mut idle) = (CommentBox::default(), CommentBox::default(), CommentBox::default());
        for b in [&mut first, &mut second, &mut idle] {
            b.open(Some(&lines()), Some(&co)).unwrap();
        }
        first.send(Some(lines()), Some(&co), "one", 1).unwrap();
        second.send(Some(lines()), Some(&co), "two", 2).unwrap();
        assert!(!first.saved(2) && first.is_open(), "the other comment's save");
        assert!(second.saved(2) && !second.is_open());
        assert!(first.saved(1) && !first.is_open());
        assert!(!idle.saved(1) && !idle.saved(2));
        assert!(idle.is_open(), "a box that sent nothing is shut by no save");
    }

    #[test]
    fn an_older_daemons_save_names_no_comment_and_shuts_a_box_that_waits() {
        let co = checkout(100, "main", true, vec![agent(7)]);
        let (mut waiting, mut idle) = (CommentBox::default(), CommentBox::default());
        waiting.open(Some(&lines()), Some(&co)).unwrap();
        idle.open(Some(&lines()), Some(&co)).unwrap();
        waiting.send(Some(lines()), Some(&co), "hi", 3).unwrap();
        assert!(waiting.saved(0) && !waiting.is_open());
        assert!(!idle.saved(0) && idle.is_open());
    }

    #[test]
    fn a_shut_box_sends_nothing() {
        let co = checkout(100, "main", true, vec![agent(7)]);
        let mut b = CommentBox::default();
        assert_eq!(b.send(Some(lines()), Some(&co), "hi", 1).err(), Some("no comment box is open"));
        b.open(Some(&lines()), Some(&co)).unwrap();
        b.close();
        assert_eq!(b.send(Some(lines()), Some(&co), "hi", 2).err(), Some("no comment box is open"));
    }

    #[test]
    fn a_box_put_away_after_sending_is_not_shut_again_by_its_save() {
        let co = checkout(100, "main", true, vec![agent(7)]);
        let mut b = CommentBox::default();
        b.open(Some(&lines()), Some(&co)).unwrap();
        b.send(Some(lines()), Some(&co), "hi", 1).unwrap();
        b.close();
        assert!(!b.saved(1));
    }

    fn refusal(request_id: u64) -> RequestRefusal {
        RequestRefusal { request_id, reason: "no".into(), read: false }
    }

    /// An open box on one live agent.
    fn opened() -> (CommentBox, CheckoutInfo) {
        let co = checkout(100, "main", true, vec![agent(7)]);
        let mut b = CommentBox::default();
        b.open(Some(&lines()), Some(&co)).unwrap();
        (b, co)
    }

    #[test]
    fn a_box_sends_one_comment_at_a_time() {
        let (mut b, co) = opened();
        b.send(Some(lines()), Some(&co), "one", 1).unwrap();
        assert_eq!(b.send(Some(lines()), Some(&co), "two", 2).err(), Some(WAITING));
        assert!(b.saved(1));
        b.open(Some(&lines()), Some(&co)).unwrap();
        assert!(b.send(Some(lines()), Some(&co), "three", 3).is_ok());
    }

    #[test]
    fn a_refused_comment_keeps_its_box_and_can_be_sent_again() {
        let (mut b, co) = opened();
        b.send(Some(lines()), Some(&co), "one", 1).unwrap();
        assert!(!b.refused(&refusal(9)), "another request's refusal");
        assert_eq!(b.send(Some(lines()), Some(&co), "one", 2).err(), Some(WAITING));
        assert!(b.refused(&refusal(1)));
        assert!(b.is_open());
        assert!(b.send(Some(lines()), Some(&co), "one", 2).is_ok());
    }

    #[test]
    fn an_older_daemons_bare_refusal_lets_go_of_the_comment_waiting() {
        let (mut b, co) = opened();
        b.send(Some(lines()), Some(&co), "one", 1).unwrap();
        assert!(b.refused(&refusal(0)));
        assert!(b.is_open());
        assert!(b.send(Some(lines()), Some(&co), "one", 2).is_ok());
    }

    #[test]
    fn a_bare_error_from_an_older_daemon_still_lets_the_save_shut_the_box() {
        let (mut b, co) = opened();
        b.send(Some(lines()), Some(&co), "one", 1).unwrap();
        assert!(b.refused(&refusal(0)), "it may have been this comment's");
        assert!(b.saved(0));
        assert!(!b.is_open());
    }

    #[test]
    fn a_new_connection_lets_a_waiting_box_send_again() {
        let (mut b, co) = opened();
        b.send(Some(lines()), Some(&co), "one", 1).unwrap();
        b.forget_sent();
        assert!(b.is_open(), "the words stay");
        assert!(b.send(Some(lines()), Some(&co), "one", 2).is_ok());
        assert!(!b.saved(1), "the last connection's save is not this one's");
    }

    #[test]
    fn the_page_lights_only_an_agent_that_still_runs() {
        let co = checkout(100, "main", true, vec![
            pane(6, PaneKind::Agent, "gone", PaneStatus::Exited { code: Some(0) }),
            pane(7, PaneKind::Agent, "claude", PaneStatus::Working),
            pane(8, PaneKind::Shell, "zsh", PaneStatus::Idle),
        ]);
        let lit = page(Some(&co), Some(PaneId(7)), "line 1 · a.rs".into());
        assert_eq!(lit.recipients, [Recipient { pane: 7, title: "claude".into() }]);
        assert_eq!(lit.to, Some(7));
        assert_eq!(page(Some(&co), Some(PaneId(6)), String::new()).to, None, "an exited agent is not lit");
        assert_eq!(page(None, Some(PaneId(7)), String::new()).to, None, "nor one in a checkout that has gone");
    }
}
