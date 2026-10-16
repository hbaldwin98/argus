//! The comment box the review and the Code stage share: whether it is open,
//! the agent it is lit on, whether a comment the daemon saved was its own,
//! and what the page draws of it. Whether a comment may go, and to whom, is
//! `argus_protocol::agent_comment`'s.

use argus_protocol::agent_comment::{self, Comment};
use argus_protocol::{CheckoutInfo, ClientMsg, PaneId, ReviewAnchor};
use serde::Serialize;

#[derive(Debug, Default)]
pub struct CommentBox {
    open: bool,
    /// The agent picked, or lit when the box opened; kept for the next box
    /// while it still runs.
    to: Option<PaneId>,
    /// A comment sent from this box the daemon has yet to say it saved.
    /// `ReviewCommentSaved` names no request, so this is how a save finds
    /// its box.
    sending: bool,
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

    /// Escape, Cancel, or the lines under it gone: shut unsent.
    pub fn close(&mut self) {
        self.open = false;
        self.sending = false;
    }

    /// The comment as typed, to the agent lit. Refused while shut: the
    /// lines under the box may have changed since it closed.
    pub fn send(&mut self, anchor: Option<ReviewAnchor>, checkout: Option<&CheckoutInfo>, body: &str) -> Result<ClientMsg, &'static str> {
        if !self.open {
            return Err("no comment box is open");
        }
        match agent_comment::comment(anchor, checkout, self.to, body)? {
            Comment::Send(msg) => {
                self.sending = true;
                Ok(*msg)
            }
            // `open` lights an agent, so an open box always names one.
            Comment::Choose(_) => Err("choose the agent to send it to"),
        }
    }

    /// The daemon saved a comment. True when this box sent it: it shuts,
    /// and the caller lets its lines go.
    pub fn saved(&mut self) -> bool {
        let mine = std::mem::take(&mut self.sending);
        if mine {
            self.open = false;
        }
        mine
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
        assert_eq!(recipient(b.send(Some(lines()), Some(&co), "hi")), PaneId(7));
    }

    #[test]
    fn a_box_keeps_the_agent_chosen_across_comments_while_it_runs() {
        let co = checkout(100, "main", true, vec![agent(7), agent(8)]);
        let mut b = CommentBox::default();
        b.open(Some(&lines()), Some(&co)).unwrap();
        b.choose(PaneId(8));
        assert_eq!(recipient(b.send(Some(lines()), Some(&co), "one")), PaneId(8));
        assert!(b.saved());
        b.open(Some(&lines()), Some(&co)).unwrap();
        assert_eq!(recipient(b.send(Some(lines()), Some(&co), "two")), PaneId(8));
    }

    #[test]
    fn an_agent_chosen_that_has_since_exited_is_refused_not_replaced() {
        let mut co = checkout(100, "main", true, vec![agent(7), agent(8)]);
        let mut b = CommentBox::default();
        b.open(Some(&lines()), Some(&co)).unwrap();
        b.choose(PaneId(8));
        co.panes[1] = gone(8);
        assert_eq!(b.send(Some(lines()), Some(&co), "hi").err(), Some("that agent is no longer running"));
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
    fn a_save_shuts_only_the_box_that_sent() {
        let co = checkout(100, "main", true, vec![agent(7)]);
        let (mut sent, mut other) = (CommentBox::default(), CommentBox::default());
        sent.open(Some(&lines()), Some(&co)).unwrap();
        other.open(Some(&lines()), Some(&co)).unwrap();
        sent.send(Some(lines()), Some(&co), "hi").unwrap();
        assert!(sent.saved());
        assert!(!sent.is_open());
        assert!(!other.saved());
        assert!(other.is_open(), "a save from another box leaves this one open");
    }

    #[test]
    fn a_shut_box_sends_nothing() {
        let co = checkout(100, "main", true, vec![agent(7)]);
        let mut b = CommentBox::default();
        assert_eq!(b.send(Some(lines()), Some(&co), "hi").err(), Some("no comment box is open"));
        b.open(Some(&lines()), Some(&co)).unwrap();
        b.close();
        assert_eq!(b.send(Some(lines()), Some(&co), "hi").err(), Some("no comment box is open"));
    }

    #[test]
    fn a_box_put_away_after_sending_is_not_shut_again_by_its_save() {
        let co = checkout(100, "main", true, vec![agent(7)]);
        let mut b = CommentBox::default();
        b.open(Some(&lines()), Some(&co)).unwrap();
        b.send(Some(lines()), Some(&co), "hi").unwrap();
        b.close();
        assert!(!b.saved());
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
