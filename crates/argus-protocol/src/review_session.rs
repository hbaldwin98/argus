//! One review's exchange with the daemon and what it comes to: which sides
//! were asked for and which have answered, the side on screen and when an
//! empty one gives way, and who a comment from it can go to. Both clients
//! drive it and only draw it, so a review behaves the same in either.
//!
//! Working changes ask for both sides at once. One round trip settles
//! everything the review can show — the side to open on, how many files the
//! other holds, whether there is anything at all — and switching sides
//! afterwards is a look at what is already in hand rather than another wait.

use crate::review_view::ReviewView;
use crate::{CheckoutId, CheckoutInfo, ClientMsg, PaneId, PaneInfo, PaneKind, PaneStatus, Review, ReviewBase};

/// What an answer from the daemon came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answered {
    /// Not this review's: another checkout's, or a request since asked again.
    Stale,
    /// Kept, with nothing yet to show for it: the side on screen is still
    /// out, or came back empty while the other side has yet to say.
    Waiting,
    /// The side on screen arrived with changes. `gave_way` is the side that
    /// was wanted and came back empty, when this is the other one instead.
    Shown { base: ReviewBase, gave_way: Option<ReviewBase> },
    /// The side not on screen arrived, behind one that is.
    Updated,
    /// Every side asked for came back empty.
    Nothing,
    /// The daemon could not read a side.
    Failed(String),
}

/// A comment that passed: sent as it stands, or waiting on a choice
/// between the agents that could take it.
#[derive(Debug, Clone)]
pub enum Comment {
    Send(Box<ClientMsg>),
    Choose(Vec<PaneId>),
}

#[derive(Default)]
struct Side {
    view: Option<ReviewView>,
    /// The request out for this side, if one is.
    asked: Option<u64>,
    failed: Option<String>,
}

pub struct ReviewSession {
    checkout: CheckoutId,
    /// The commit a review of one commit is of, as its oid.
    commit: Option<String>,
    base: ReviewBase,
    split: bool,
    unstaged: Side,
    staged: Side,
    of_commit: Side,
    /// The file to open on once the commit arrives, when it was opened from
    /// a file row of the history.
    jump_to: Option<String>,
}

/// The agents a comment from `checkout` can go to. A shell has nothing to
/// hear it with, and an exited agent nowhere to put it.
pub fn recipients(checkout: &CheckoutInfo) -> impl Iterator<Item = &PaneInfo> {
    checkout
        .listed_panes()
        .filter(|p| p.kind == PaneKind::Agent && !matches!(p.status, PaneStatus::Exited { .. }))
}

impl ReviewSession {
    /// A review of `checkout`'s working changes, opening on `prefer` when it
    /// has any. The preferred side is asked for first.
    pub fn working(
        checkout: CheckoutId,
        prefer: ReviewBase,
        split: bool,
        mut id: impl FnMut() -> u64,
    ) -> (ReviewSession, Vec<ClientMsg>) {
        let base = if prefer == ReviewBase::Commit { ReviewBase::Unstaged } else { prefer };
        let mut session = ReviewSession::new(checkout, base, None, split);
        let msgs = session.bases().into_iter().map(|b| session.ask(b, id())).collect();
        (session, msgs)
    }

    /// One commit against its first parent, opening on `file` when it is
    /// among the files the commit touched.
    pub fn of_commit(
        checkout: CheckoutId,
        commit: String,
        file: Option<String>,
        split: bool,
        id: u64,
    ) -> (ReviewSession, ClientMsg) {
        let mut session = ReviewSession::new(checkout, ReviewBase::Commit, Some(commit), split);
        session.jump_to = file;
        let msg = session.ask(ReviewBase::Commit, id);
        (session, msg)
    }

    fn new(checkout: CheckoutId, base: ReviewBase, commit: Option<String>, split: bool) -> ReviewSession {
        ReviewSession {
            checkout,
            commit,
            base,
            split,
            unstaged: Side::default(),
            staged: Side::default(),
            of_commit: Side::default(),
            jump_to: None,
        }
    }

    /// The sides this review is made of, the one on screen first.
    fn bases(&self) -> Vec<ReviewBase> {
        match self.base {
            ReviewBase::Commit => vec![ReviewBase::Commit],
            base => vec![base, base.next()],
        }
    }

    fn ask(&mut self, base: ReviewBase, request_id: u64) -> ClientMsg {
        let side = self.side_mut(base);
        side.asked = Some(request_id);
        side.failed = None;
        ClientMsg::Review { request_id, checkout: self.checkout, base, commit: self.commit.clone() }
    }

    /// Reads every side again: the agent kept working. What was on screen
    /// stays until its fresh copy arrives.
    pub fn refresh(&mut self, mut id: impl FnMut() -> u64) -> Vec<ClientMsg> {
        self.bases().into_iter().map(|b| self.ask(b, id())).collect()
    }

    /// The side `review` answers, if it was asked for here and is the
    /// latest ask for that side.
    fn asked_for(&self, checkout: CheckoutId, request_id: u64) -> Option<ReviewBase> {
        if checkout != self.checkout {
            return None;
        }
        [ReviewBase::Unstaged, ReviewBase::Staged, ReviewBase::Commit]
            .into_iter()
            .find(|&b| self.side(b).asked == Some(request_id))
    }

    pub fn receive(&mut self, review: Review) -> Answered {
        let Some(base) = self.asked_for(review.checkout, review.request_id) else {
            return Answered::Stale;
        };
        let mut view = ReviewView::new(review, self.split);
        // A refresh keeps the file the cursor was in, by path: a file added
        // ahead of it shifts every index after it.
        let path = match self.jump_to.take() {
            Some(path) => Some(path),
            None => self.side(base).view.as_ref().and_then(|old| {
                let file = old.rows.get(old.sel)?.file();
                Some(old.review.files.get(file)?.path.clone())
            }),
        };
        if let Some(at) = path.and_then(|p| view.review.files.iter().position(|f| f.path == p)) {
            view.jump_to_file(at);
        }
        let side = self.side_mut(base);
        side.asked = None;
        side.view = Some(view);
        self.settle(base)
    }

    pub fn failed(&mut self, checkout: CheckoutId, request_id: u64, message: String) -> Answered {
        let Some(base) = self.asked_for(checkout, request_id) else {
            return Answered::Stale;
        };
        let side = self.side_mut(base);
        side.asked = None;
        side.failed = Some(message.clone());
        Answered::Failed(message)
    }

    /// What `arrived` coming in means for the side on screen.
    fn settle(&mut self, arrived: ReviewBase) -> Answered {
        let shown = self.base;
        let has_files = |s: &Side| s.view.as_ref().is_some_and(|v| !v.review.is_empty());
        if arrived != shown && (self.side(shown).asked.is_some() || has_files(self.side(shown))) {
            return Answered::Updated;
        }
        if has_files(self.side(shown)) {
            return Answered::Shown { base: shown, gave_way: None };
        }
        // The side on screen is empty, and the toggle that would reach the
        // other lives inside the review: opening on nothing would be a dead
        // end when the other side has the work.
        if shown == ReviewBase::Commit {
            return Answered::Nothing;
        }
        let other = self.side(shown.next());
        if other.asked.is_some() {
            return Answered::Waiting;
        }
        if has_files(other) {
            self.base = shown.next();
            return Answered::Shown { base: self.base, gave_way: Some(shown) };
        }
        Answered::Nothing
    }

    /// `b`: the other side of the working changes.
    pub fn switch_side(&mut self) -> Result<(), String> {
        self.show_side(self.base.next())
    }

    /// Puts `base` on screen unless it is known to have nothing. An empty
    /// side is refused, keeping the diff already up: the toggle says how
    /// many files each side has, so nothing is hidden by staying. A side
    /// still loading is shown, and gives way like any other if it comes
    /// back empty.
    pub fn show_side(&mut self, base: ReviewBase) -> Result<(), String> {
        if self.base == ReviewBase::Commit || base == ReviewBase::Commit {
            return Err("a commit has no other side".to_owned());
        }
        if base == self.base {
            return Ok(());
        }
        let side = self.side(base);
        if let Some(failed) = side.failed.as_ref().filter(|_| side.asked.is_none()) {
            return Err(failed.clone());
        }
        match &side.view {
            Some(view) if view.review.is_empty() && side.asked.is_none() => {
                Err(format!("nothing {}; still showing {}", base.label(), self.base.label()))
            }
            _ => {
                self.base = base;
                Ok(())
            }
        }
    }

    /// Both sides of a change side by side, or stacked, for every side held.
    pub fn set_split(&mut self, split: bool) {
        self.split = split;
        for side in [&mut self.unstaged, &mut self.staged, &mut self.of_commit] {
            if let Some(view) = &mut side.view {
                view.set_split(split);
            }
        }
    }

    /// The daemon saved a comment: the lines it was on are let go.
    pub fn commented(&mut self) {
        if let Some(view) = self.view_mut() {
            view.mark = None;
        }
    }

    /// Whether a comment could be written now: there are lines to anchor it
    /// to and an agent to read it.
    pub fn can_comment(&self, checkout: Option<&CheckoutInfo>) -> Result<(), &'static str> {
        if self.view().and_then(ReviewView::anchor).is_none() {
            return Err("pick a line to comment on");
        }
        if checkout.is_none_or(|c| recipients(c).next().is_none()) {
            return Err("no agent running in this checkout");
        }
        Ok(())
    }

    /// `body` on the picked lines, addressed to `to` — or, with nobody
    /// chosen, to the one agent there is, or a choice between several.
    /// `checkout` is as the tree has it now: agents come and go while a
    /// comment is written.
    pub fn comment(
        &self,
        checkout: Option<&CheckoutInfo>,
        to: Option<PaneId>,
        body: &str,
    ) -> Result<Comment, &'static str> {
        let body = body.trim();
        if body.is_empty() {
            return Err("the comment is empty");
        }
        let anchor = self.view().and_then(ReviewView::anchor).ok_or("pick a line to comment on")?;
        let live: Vec<PaneId> = checkout.into_iter().flat_map(recipients).map(|p| p.id).collect();
        let recipient = match (to, live.as_slice()) {
            (_, []) => return Err("no agent running in this checkout"),
            (Some(pane), _) if !live.contains(&pane) => return Err("that agent is no longer running"),
            (Some(pane), _) => pane,
            (None, [only]) => *only,
            (None, _) => return Ok(Comment::Choose(live)),
        };
        Ok(Comment::Send(Box::new(ClientMsg::ReviewComment {
            checkout: self.checkout,
            recipient,
            anchor: Box::new(anchor),
            body: body.to_owned(),
        })))
    }

    pub fn checkout(&self) -> CheckoutId {
        self.checkout
    }

    /// The side on screen.
    pub fn base(&self) -> ReviewBase {
        self.base
    }

    /// The oid of the commit a commit review is of.
    pub fn commit(&self) -> Option<&str> {
        self.commit.as_deref()
    }

    pub fn split(&self) -> bool {
        self.split
    }

    /// The diff on screen, once its side has arrived.
    pub fn view(&self) -> Option<&ReviewView> {
        self.side(self.base).view.as_ref()
    }

    pub fn view_mut(&mut self) -> Option<&mut ReviewView> {
        self.side_mut(self.base).view.as_mut()
    }

    /// Why the side on screen could not be read.
    pub fn failure(&self) -> Option<&str> {
        self.side(self.base).failed.as_deref()
    }

    /// How many files `base` changed, once it has said.
    pub fn files(&self, base: ReviewBase) -> Option<usize> {
        self.side(base).view.as_ref().map(|v| v.review.files.len())
    }

    fn side(&self, base: ReviewBase) -> &Side {
        match base {
            ReviewBase::Unstaged => &self.unstaged,
            ReviewBase::Staged => &self.staged,
            ReviewBase::Commit => &self.of_commit,
        }
    }

    fn side_mut(&mut self, base: ReviewBase) -> &mut Side {
        match base {
            ReviewBase::Unstaged => &mut self.unstaged,
            ReviewBase::Staged => &mut self.staged,
            ReviewBase::Commit => &mut self.of_commit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AgentTelemetry, ChangeKind, DiffLine, FileDiff, Hunk, LineKind};
    use ReviewBase::{Commit, Staged, Unstaged};

    const CO: CheckoutId = CheckoutId(100);

    fn file(path: &str) -> FileDiff {
        FileDiff {
            path: path.into(),
            old_path: None,
            kind: ChangeKind::Modified,
            hunks: vec![Hunk {
                header: "@@ -1 +1 @@".into(),
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_lineno: None,
                    new_lineno: Some(1),
                    text: "new".into(),
                    spans: Vec::new(),
                }],
            }],
            note: None,
        }
    }

    fn answer(request_id: u64, base: ReviewBase, paths: &[&str]) -> Review {
        Review { request_id, checkout: CO, base, files: paths.iter().map(|p| file(p)).collect(), commit: None }
    }

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

    /// A working review asked with ids 1 (unstaged) and 2 (staged).
    fn working() -> (ReviewSession, Vec<ClientMsg>) {
        let mut ids = 0;
        ReviewSession::working(CO, Unstaged, false, || {
            ids += 1;
            ids
        })
    }

    fn showing_a_line() -> ReviewSession {
        let (mut s, _) = working();
        s.receive(answer(1, Unstaged, &["a.rs"]));
        s.receive(answer(2, Staged, &[]));
        s
    }

    #[test]
    fn working_changes_ask_for_both_sides_the_preferred_one_first() {
        let (_, sent) = ReviewSession::working(CO, Staged, false, || 7);
        assert!(matches!(
            sent.as_slice(),
            [ClientMsg::Review { base: Staged, commit: None, .. }, ClientMsg::Review { base: Unstaged, .. }]
        ));
    }

    #[test]
    fn the_side_on_screen_shows_whichever_order_the_answers_come_in() {
        let (mut s, _) = working();
        assert_eq!(s.receive(answer(2, Staged, &["b.rs"])), Answered::Updated);
        assert_eq!(s.receive(answer(1, Unstaged, &["a.rs"])), Answered::Shown { base: Unstaged, gave_way: None });
        assert_eq!((s.files(Unstaged), s.files(Staged)), (Some(1), Some(1)));

        let (mut s, _) = working();
        assert_eq!(s.receive(answer(1, Unstaged, &["a.rs"])), Answered::Shown { base: Unstaged, gave_way: None });
        assert_eq!(s.receive(answer(2, Staged, &["b.rs"])), Answered::Updated);
        assert_eq!(s.base(), Unstaged);
    }

    #[test]
    fn an_answer_to_another_checkout_or_an_older_ask_is_stale() {
        let (mut s, _) = working();
        let mut elsewhere = answer(1, Unstaged, &["a.rs"]);
        elsewhere.checkout = CheckoutId(9);
        assert_eq!(s.receive(elsewhere), Answered::Stale);
        assert_eq!(s.receive(answer(99, Unstaged, &["a.rs"])), Answered::Stale);
        assert!(s.view().is_none());

        let msgs = s.refresh(|| 5);
        assert_eq!(msgs.len(), 2);
        assert_eq!(s.receive(answer(1, Unstaged, &["a.rs"])), Answered::Stale, "asked again since");
        assert_eq!(s.failed(CO, 2, "gone".into()), Answered::Stale);
    }

    #[test]
    fn an_answer_is_taken_once() {
        let (mut s, _) = working();
        s.receive(answer(1, Unstaged, &["a.rs"]));
        assert_eq!(s.receive(answer(1, Unstaged, &["a.rs", "b.rs"])), Answered::Stale);
        assert_eq!(s.files(Unstaged), Some(1));
    }

    #[test]
    fn a_failure_is_said_in_either_order_and_stale_ones_are_not() {
        let (mut s, _) = working();
        assert_eq!(s.failed(CO, 1, "no repository".into()), Answered::Failed("no repository".into()));
        assert_eq!(s.failure(), Some("no repository"));
        assert_eq!(s.failed(CO, 1, "again".into()), Answered::Stale);
        assert_eq!(s.receive(answer(2, Staged, &["b.rs"])), Answered::Shown { base: Staged, gave_way: Some(Unstaged) });

        let (mut s, _) = working();
        s.receive(answer(1, Unstaged, &["a.rs"]));
        assert_eq!(s.failed(CO, 2, "locked".into()), Answered::Failed("locked".into()));
        assert_eq!(s.failure(), None, "the side on screen read fine");
        assert_eq!(s.switch_side(), Err("locked".into()));
    }

    #[test]
    fn an_empty_side_gives_way_to_the_other_once_it_has_said() {
        let (mut s, _) = working();
        assert_eq!(s.receive(answer(1, Unstaged, &[])), Answered::Waiting);
        assert_eq!(s.receive(answer(2, Staged, &["b.rs"])), Answered::Shown { base: Staged, gave_way: Some(Unstaged) });
        assert_eq!(s.base(), Staged);

        let (mut s, _) = working();
        s.receive(answer(2, Staged, &["b.rs"]));
        assert_eq!(s.receive(answer(1, Unstaged, &[])), Answered::Shown { base: Staged, gave_way: Some(Unstaged) });
    }

    #[test]
    fn nothing_on_either_side_is_nothing_and_keeps_the_side_asked_for() {
        let (mut s, _) = working();
        s.receive(answer(1, Unstaged, &[]));
        assert_eq!(s.receive(answer(2, Staged, &[])), Answered::Nothing);
        assert_eq!(s.base(), Unstaged);
    }

    #[test]
    fn switching_onto_an_empty_side_keeps_the_diff_on_screen() {
        let (mut s, _) = working();
        s.receive(answer(1, Unstaged, &["a.rs"]));
        s.receive(answer(2, Staged, &[]));
        assert_eq!(s.switch_side(), Err("nothing staged; still showing unstaged".into()));
        assert_eq!(s.base(), Unstaged);

        let (mut s, _) = working();
        s.receive(answer(1, Unstaged, &["a.rs"]));
        assert_eq!(s.switch_side(), Ok(()), "a side still loading may be picked");
        assert!(s.view().is_none());
        assert_eq!(s.receive(answer(2, Staged, &[])), Answered::Shown { base: Unstaged, gave_way: Some(Staged) });

        let (mut s, _) = working();
        s.receive(answer(1, Unstaged, &["a.rs"]));
        s.receive(answer(2, Staged, &["b.rs"]));
        assert_eq!(s.switch_side(), Ok(()));
        assert_eq!(s.view().unwrap().review.files[0].path, "b.rs");
    }

    #[test]
    fn a_refresh_keeps_the_file_the_cursor_was_in_even_when_a_file_lands_ahead_of_it() {
        let (mut s, _) = working();
        s.receive(answer(1, Unstaged, &["b.rs", "c.rs"]));
        s.view_mut().unwrap().jump_to_file(1);
        let sent = s.refresh(|| 3);
        assert!(matches!(sent.as_slice(), [ClientMsg::Review { request_id: 3, base: Unstaged, .. }, _]));
        assert!(s.view().is_some(), "the old diff stays up meanwhile");
        s.receive(answer(3, Unstaged, &["a.rs", "b.rs", "c.rs"]));
        let v = s.view().unwrap();
        assert_eq!(v.review.files[v.rows[v.sel].file()].path, "c.rs");
    }

    #[test]
    fn a_commit_opened_from_a_file_row_opens_on_that_file() {
        let (mut s, sent) = ReviewSession::of_commit(CO, "aaaa".into(), Some("b.rs".into()), false, 4);
        assert!(matches!(&sent, ClientMsg::Review { base: Commit, commit: Some(c), .. } if c == "aaaa"));
        assert_eq!(s.receive(answer(4, Commit, &["a.rs", "b.rs"])), Answered::Shown { base: Commit, gave_way: None });
        let v = s.view().unwrap();
        assert_eq!(v.rows[v.sel].file(), 1);
        assert!(s.switch_side().is_err(), "a commit has one side");

        let (mut s, _) = ReviewSession::of_commit(CO, "aaaa".into(), None, false, 4);
        assert_eq!(s.receive(answer(4, Commit, &[])), Answered::Nothing);
    }

    #[test]
    fn a_comment_needs_words_and_a_line() {
        let co = checkout(vec![agent(7)]);
        let (s, _) = working();
        assert_eq!(s.can_comment(Some(&co)), Err("pick a line to comment on"));
        assert_eq!(s.comment(Some(&co), None, "tidy").err(), Some("pick a line to comment on"));
        let s = showing_a_line();
        assert_eq!(s.comment(Some(&co), None, "   ").err(), Some("the comment is empty"));
    }

    #[test]
    fn a_comment_with_no_live_agent_is_refused() {
        let s = showing_a_line();
        let shells = checkout(vec![
            pane(1, PaneKind::Shell, PaneStatus::Idle),
            pane(2, PaneKind::Agent, PaneStatus::Exited { code: Some(0) }),
        ]);
        assert_eq!(s.can_comment(Some(&shells)), Err("no agent running in this checkout"));
        assert_eq!(s.comment(Some(&shells), None, "hi").err(), Some("no agent running in this checkout"));
        assert_eq!(s.comment(Some(&shells), Some(PaneId(2)), "hi").err(), Some("no agent running in this checkout"));
        assert_eq!(s.comment(None, None, "hi").err(), Some("no agent running in this checkout"), "the checkout is gone");
    }

    #[test]
    fn a_comment_goes_to_the_one_agent_or_asks_between_several() {
        let s = showing_a_line();
        let one = checkout(vec![pane(1, PaneKind::Shell, PaneStatus::Idle), agent(7)]);
        assert_eq!(s.can_comment(Some(&one)), Ok(()));
        let ClientMsg::ReviewComment { checkout: to, recipient, anchor, body } = sent(s.comment(Some(&one), None, " tidy "))
        else {
            panic!()
        };
        assert_eq!((to, recipient, body.as_str()), (CO, PaneId(7), "tidy"));
        assert_eq!(anchor.path, "a.rs");

        let several = checkout(vec![agent(7), pane(8, PaneKind::Agent, PaneStatus::Exited { code: None }), agent(9)]);
        assert!(matches!(s.comment(Some(&several), None, "hi"), Ok(Comment::Choose(p)) if p == [PaneId(7), PaneId(9)]));
        assert!(matches!(
            sent(s.comment(Some(&several), Some(PaneId(9)), "hi")),
            ClientMsg::ReviewComment { recipient: PaneId(9), .. }
        ));
        assert_eq!(s.comment(Some(&several), Some(PaneId(8)), "hi").err(), Some("that agent is no longer running"));
    }

    #[test]
    fn a_saved_comment_lets_its_lines_go() {
        let mut s = showing_a_line();
        s.view_mut().unwrap().toggle_mark();
        s.commented();
        assert!(s.view().unwrap().mark.is_none());
    }
}
