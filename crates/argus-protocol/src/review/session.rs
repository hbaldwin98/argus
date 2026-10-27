//! One review's exchange with the daemon and what it comes to: which sides
//! were asked for and which have answered, the side on screen and when an
//! empty one gives way, and the picked lines a comment from it is anchored
//! to. Both clients drive it and only draw it, so a review behaves the same
//! in either.
//!
//! Working changes ask for both sides at once. One round trip settles
//! everything the review can show — the side to open on, how many files the
//! other holds, whether there is anything at all — and switching sides
//! afterwards is a look at what is already in hand rather than another wait.
//!
//! A review of working changes follows them: the tree says when they moved
//! (`GitStatus::changes`), and the review reads them again without being
//! asked, keeping the person's place in what it held.
//!
//! Against a daemon that lists them lazily (`LAZY_REVIEWS`), working changes
//! come as a list first and each file is read when the cursor comes to it:
//! a large checkout's changes are thousands of files, and reading every one
//! before showing any was most of the wait.

use crate::review::view::ReviewView;
use crate::{CheckoutId, ClientMsg, Review, ReviewAnchor, ReviewBase, ReviewContext, ReviewFile};

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
    /// The fold whose lines are on their way. One at a time: opening
    /// another before they land means the first is no longer wanted.
    unfolding: Option<Unfolding>,
    /// The checkout's working changes as the tree last said them.
    changes: Option<u64>,
    /// They moved since the sides were last asked for.
    behind: bool,
    /// Working changes are asked for listed, each file read when wanted.
    lazy: bool,
    /// The file whose read is on its way. One at a time, like a fold.
    loading: Option<Loading>,
}

/// What a file's read was asked for against.
struct Loading {
    request_id: u64,
    base: ReviewBase,
    file: usize,
    path: String,
}

/// What a fold's lines were asked for against, to know on arrival that they
/// still belong where they were going.
struct Unfolding {
    request_id: u64,
    base: ReviewBase,
    file: usize,
    path: String,
    blob: String,
}

impl ReviewSession {
    /// A review of `checkout`'s working changes, opening on `prefer` when it
    /// has any. The preferred side is asked for first.
    pub fn working(
        checkout: CheckoutId,
        prefer: ReviewBase,
        split: bool,
        lazy: bool,
        mut id: impl FnMut() -> u64,
    ) -> (ReviewSession, Vec<ClientMsg>) {
        let base = if prefer == ReviewBase::Commit { ReviewBase::Unstaged } else { prefer };
        let mut session = ReviewSession::new(checkout, base, None, split).lazily(lazy);
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
            unfolding: None,
            changes: None,
            behind: false,
            lazy: false,
            loading: None,
        }
    }

    /// Asks for working changes listed, each file read when the cursor
    /// comes to it: for a daemon that listed `LAZY_REVIEWS`. Before the
    /// first ask, which it changes.
    pub fn lazily(mut self, lazy: bool) -> ReviewSession {
        self.lazy = lazy && self.commit.is_none();
        self
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
        ClientMsg::Review { request_id, checkout: self.checkout, base, commit: self.commit.clone(), lazy: self.lazy }
    }

    /// Reads every side again: the agent kept working. What was on screen
    /// stays until its fresh copy arrives.
    pub fn refresh(&mut self, mut id: impl FnMut() -> u64) -> Vec<ClientMsg> {
        self.behind = false;
        self.bases().into_iter().map(|b| self.ask(b, id())).collect()
    }

    /// What the tree says of the checkout's working changes now, first as
    /// the review opens, when it is what the reads asked for will show. A
    /// commit never changes, and a checkout outside git says nothing.
    pub fn follow(&mut self, changes: Option<u64>) {
        if self.commit.is_some() || changes.is_none() {
            return;
        }
        if self.changes.is_some() && changes != self.changes {
            self.behind = true;
        }
        self.changes = changes;
    }

    /// Every side asked for again when the working changes moved since
    /// they were. Not while a read is out: asking over it would drop an
    /// answer on its way, and edits coming faster than a diff is read
    /// would keep the review from ever landing one. It catches up once
    /// the answer is in.
    pub fn catch_up(&mut self, id: impl FnMut() -> u64) -> Vec<ClientMsg> {
        let out = self.bases().into_iter().any(|b| self.side(b).asked.is_some());
        if !self.behind || out {
            return Vec::new();
        }
        self.refresh(id)
    }

    /// The side `request_id` answers, if it was asked for here and is the
    /// latest ask for that side. The id alone says so: a client names every
    /// request from one count.
    fn asked_for(&self, request_id: u64) -> Option<ReviewBase> {
        [ReviewBase::Unstaged, ReviewBase::Staged, ReviewBase::Commit]
            .into_iter()
            .find(|&b| self.side(b).asked == Some(request_id))
    }

    pub fn receive(&mut self, review: Review) -> Answered {
        if review.checkout != self.checkout {
            return Answered::Stale;
        }
        let Some(base) = self.asked_for(review.request_id) else {
            return Answered::Stale;
        };
        let (split, jump_to) = (self.split, self.jump_to.take());
        // A read out for this side was of the listing before: the cursor's
        // file is asked for again against this one.
        self.loading.take_if(|l| l.base == base);
        let side = self.side_mut(base);
        side.asked = None;
        match &mut side.view {
            Some(view) => {
                view.reread(review);
            }
            None => {
                let mut view = ReviewView::new(review, split);
                if let Some(at) = jump_to.and_then(|p| view.review.files.iter().position(|f| f.path == p)) {
                    view.jump_to_file(at);
                }
                side.view = Some(view);
            }
        }
        self.settle(base)
    }

    pub fn failed(&mut self, request_id: u64, message: String) -> Answered {
        let Some(base) = self.asked_for(request_id) else {
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

    /// Enter on a fold: asks for the next step of its lines, or with `all`
    /// the whole of it. None when the cursor is not on a fold.
    pub fn unfold(&mut self, all: bool, request_id: u64) -> Option<ClientMsg> {
        let base = self.base;
        let view = self.view()?;
        let (file, start, end) = view.unfold(all)?;
        let diff = &view.review.files[file];
        let blob = diff.new_blob.clone()?;
        let msg = ClientMsg::ReviewContext {
            request_id,
            checkout: self.checkout,
            blob: blob.clone(),
            path: diff.path.clone(),
            start,
            end,
        };
        self.unfolding = Some(Unfolding { request_id, base, file, path: diff.path.clone(), blob });
        Some(msg)
    }

    /// A fold's lines, spliced in when they are the latest asked for and
    /// the file they were read from is still the one on that side: a
    /// refresh that read the file anew has folds of its own.
    pub fn receive_context(&mut self, context: ReviewContext) -> bool {
        if context.checkout != self.checkout {
            return false;
        }
        let Some(asked) = self.unfolding.take_if(|u| u.request_id == context.request_id) else {
            return false;
        };
        let Some(view) = self.side_mut(asked.base).view.as_mut() else {
            return false;
        };
        let same = view
            .review
            .files
            .get(asked.file)
            .is_some_and(|f| f.path == asked.path && f.new_blob.as_deref() == Some(asked.blob.as_str()));
        same && view.reveal(asked.file, context.lines)
    }

    /// The read of the cursor's file on the side on screen, when it is
    /// listed and not yet read and no read is already out.
    pub fn load(&mut self, request_id: impl FnOnce() -> u64) -> Option<ClientMsg> {
        if self.loading.is_some() {
            return None;
        }
        let base = self.base;
        let view = self.view()?;
        let file = view.wanted()?;
        let diff = &view.review.files[file];
        let (path, old_path, kind) = (diff.path.clone(), diff.old_path.clone(), diff.kind);
        let request_id = request_id();
        self.loading = Some(Loading { request_id, base, file, path: path.clone() });
        Some(ClientMsg::ReviewFile { request_id, checkout: self.checkout, base, path, old_path, kind })
    }

    /// A file's read, put in place when it is the latest asked for and the
    /// file is still where it was.
    pub fn receive_file(&mut self, read: ReviewFile) -> bool {
        if read.checkout != self.checkout {
            return false;
        }
        let Some(asked) = self.loading.take_if(|l| l.request_id == read.request_id) else {
            return false;
        };
        let Some(view) = self.side_mut(asked.base).view.as_mut() else {
            return false;
        };
        let mut file = read.file;
        file.pending = false;
        file.path == asked.path && view.fill(asked.file, file)
    }

    /// A file that could not be read: it stands as a note saying why, read
    /// no further until the side is listed again. False when it was not
    /// this review's latest ask.
    pub fn file_failed(&mut self, request_id: u64, message: String) -> bool {
        let Some(asked) = self.loading.take_if(|l| l.request_id == request_id) else {
            return false;
        };
        if let Some(view) = self.side_mut(asked.base).view.as_mut() {
            if let Some(held) = view.review.files.get(asked.file).filter(|f| f.path == asked.path).cloned() {
                let failed = crate::FileDiff { hunks: Vec::new(), note: Some(message), pending: false, ..held };
                view.fill(asked.file, failed);
            }
        }
        true
    }

    /// Whether a fold that could not be read was this review's latest ask.
    pub fn context_failed(&mut self, request_id: u64) -> bool {
        self.unfolding.take_if(|u| u.request_id == request_id).is_some()
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

    /// The picked lines of the side on screen, as a comment on them anchors.
    pub fn anchor(&self) -> Option<ReviewAnchor> {
        self.view().and_then(ReviewView::anchor)
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

    /// The diff of `base`, on screen or not, once it has arrived: what the
    /// Changes pane lists both sides' files from.
    pub fn view_of(&self, base: ReviewBase) -> Option<&ReviewView> {
        self.side(base).view.as_ref()
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
    use crate::{ChangeKind, DiffLine, FileDiff, Hunk, LineKind};
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
            new_blob: None,
            new_lines: None,
            pending: false,
            counted: None,
        }
    }

    fn answer(request_id: u64, base: ReviewBase, paths: &[&str]) -> Review {
        Review { request_id, checkout: CO, base, files: paths.iter().map(|p| file(p)).collect(), commit: None }
    }

    /// A working review asked with ids 1 (unstaged) and 2 (staged).
    fn working() -> (ReviewSession, Vec<ClientMsg>) {
        let mut ids = 0;
        ReviewSession::working(CO, Unstaged, false, false, || {
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

    /// A lazy working review asked with ids 1 (unstaged) and 2 (staged),
    /// each side listed with `paths` not yet read.
    fn listed(unstaged: &[&str], staged: &[&str]) -> (ReviewSession, Vec<ClientMsg>) {
        let mut ids = 0;
        let (mut s, sent) = ReviewSession::working(CO, Unstaged, false, true, || {
            ids += 1;
            ids
        });
        s.receive(pending(1, Unstaged, unstaged));
        s.receive(pending(2, Staged, staged));
        (s, sent)
    }

    fn pending(request_id: u64, base: ReviewBase, paths: &[&str]) -> Review {
        let stub = |p: &&str| FileDiff { hunks: Vec::new(), pending: true, ..file(p) };
        Review { request_id, checkout: CO, base, files: paths.iter().map(stub).collect(), commit: None }
    }

    fn read(request_id: u64, path: &str) -> ReviewFile {
        ReviewFile { request_id, checkout: CO, base: Unstaged, file: file(path) }
    }

    fn asked_file(msg: Option<ClientMsg>) -> Option<(u64, String)> {
        match msg? {
            ClientMsg::ReviewFile { request_id, path, base: Unstaged, .. } => Some((request_id, path)),
            other => panic!("not a file read: {other:?}"),
        }
    }

    #[test]
    fn a_lazy_review_asks_for_its_sides_listed() {
        let (_, sent) = listed(&[], &[]);
        assert!(sent.iter().all(|m| matches!(m, ClientMsg::Review { lazy: true, .. })), "{sent:?}");
        let (_, sent) = working();
        assert!(sent.iter().all(|m| matches!(m, ClientMsg::Review { lazy: false, .. })), "a daemon that cannot list is asked for all");
    }

    #[test]
    fn the_file_the_cursor_comes_to_is_read_once_and_put_in_place() {
        let (mut s, _) = listed(&["a.rs", "b.rs"], &[]);
        assert_eq!(asked_file(s.load(|| 10)), Some((10, "a.rs".into())), "the cursor opens on the first file");
        assert!(s.load(|| 11).is_none(), "one read out at a time");
        assert!(s.receive_file(read(10, "a.rs")));
        let view = s.view().unwrap();
        assert!(view.rows[view.sel].is_selectable(), "the cursor is on its first line once it is read");
        assert!(!view.review.files[0].pending);
        assert!(s.load(|| 12).is_none(), "the file on screen is read");
        s.view_mut().unwrap().jump_file(true);
        assert_eq!(asked_file(s.load(|| 13)), Some((13, "b.rs".into())), "the next file, once the cursor is on it");
    }

    #[test]
    fn a_read_for_a_listing_since_replaced_is_dropped_and_asked_again() {
        let (mut s, _) = listed(&["a.rs"], &[]);
        assert!(asked_file(s.load(|| 10)).is_some());
        s.refresh(|| 20);
        s.receive(pending(20, Unstaged, &["a.rs"]));
        assert!(!s.receive_file(read(10, "a.rs")), "read against the listing before");
        assert_eq!(asked_file(s.load(|| 30)), Some((30, "a.rs".into())));
    }

    #[test]
    fn a_file_read_before_stays_on_screen_while_a_new_listing_reads_it_again() {
        let (mut s, _) = listed(&["a.rs"], &[]);
        s.load(|| 10);
        s.receive_file(read(10, "a.rs"));
        s.refresh(|| 20);
        s.receive(pending(20, Unstaged, &["a.rs"]));
        let view = s.view().unwrap();
        assert!(view.review.files[0].pending && !view.review.files[0].hunks.is_empty(), "its lines kept, to be read again");
        assert!(view.rows[view.sel].is_selectable(), "the cursor stays on its line");
        assert_eq!(asked_file(s.load(|| 30)), Some((30, "a.rs".into())));
    }

    #[test]
    fn a_file_that_cannot_be_read_says_why_and_is_not_asked_for_again() {
        let (mut s, _) = listed(&["a.rs"], &[]);
        s.load(|| 10);
        assert!(!s.file_failed(99, "not ours".into()));
        assert!(s.file_failed(10, "gone".into()));
        let view = s.view().unwrap();
        assert_eq!(view.review.files[0].note.as_deref(), Some("gone"));
        assert!(s.load(|| 11).is_none());
    }

    #[test]
    fn working_changes_ask_for_both_sides_the_preferred_one_first() {
        let (_, sent) = ReviewSession::working(CO, Staged, false, false, || 7);
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
        assert_eq!(s.failed(2, "gone".into()), Answered::Stale);
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
        assert_eq!(s.failed(1, "no repository".into()), Answered::Failed("no repository".into()));
        assert_eq!(s.failure(), Some("no repository"));
        assert_eq!(s.failed(1, "again".into()), Answered::Stale);
        assert_eq!(s.receive(answer(2, Staged, &["b.rs"])), Answered::Shown { base: Staged, gave_way: Some(Unstaged) });

        let (mut s, _) = working();
        s.receive(answer(1, Unstaged, &["a.rs"]));
        assert_eq!(s.failed(2, "locked".into()), Answered::Failed("locked".into()));
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

    // --- following the working changes -------------------------------------

    /// A working review read under ids 1 and 2 while the tree said `0`.
    fn followed() -> ReviewSession {
        let (mut s, _) = working();
        s.follow(Some(0));
        s.receive(answer(1, Unstaged, &["a.rs"]));
        s.receive(answer(2, Staged, &[]));
        s
    }

    #[test]
    fn a_working_review_reads_again_when_its_changes_move_and_only_then() {
        let mut s = followed();
        s.follow(Some(0));
        assert!(s.catch_up(|| 3).is_empty(), "nothing moved");
        s.follow(Some(7));
        let sent = s.catch_up(|| 3);
        assert!(matches!(sent.as_slice(), [ClientMsg::Review { base: Unstaged, .. }, ClientMsg::Review { base: Staged, .. }]));
        assert!(s.catch_up(|| 5).is_empty(), "asked once");
    }

    #[test]
    fn a_move_while_a_read_is_out_waits_for_its_answer() {
        let (mut s, _) = working();
        s.follow(Some(0));
        s.follow(Some(1));
        assert!(s.catch_up(|| 3).is_empty(), "both sides are still out");
        s.receive(answer(1, Unstaged, &["a.rs"]));
        assert!(s.catch_up(|| 3).is_empty(), "the staged side is still out");
        s.receive(answer(2, Staged, &[]));
        assert_eq!(s.catch_up(|| 3).len(), 2);
    }

    #[test]
    fn a_refresh_asked_for_settles_a_move_already_seen() {
        let mut s = followed();
        s.follow(Some(1));
        s.refresh(|| 3);
        s.receive(answer(3, Unstaged, &["a.rs"]));
        s.receive(answer(3, Staged, &[]));
        assert!(s.catch_up(|| 5).is_empty());
    }

    #[test]
    fn a_commit_and_a_checkout_outside_git_never_read_again() {
        let (mut s, _) = ReviewSession::of_commit(CO, "aaaa".into(), None, false, 4);
        s.receive(answer(4, Commit, &["a.rs"]));
        s.follow(Some(0));
        s.follow(Some(1));
        assert!(s.catch_up(|| 5).is_empty());

        let mut s = followed();
        s.follow(None);
        assert!(s.catch_up(|| 5).is_empty(), "no word is not a move");
    }

    /// A file of three added lines, the first reading `first`.
    fn three_lines(request_id: u64, first: &str) -> Review {
        let mut review = answer(request_id, Unstaged, &["a.rs"]);
        review.files[0].hunks[0].lines = (1..=3).map(|n| numbered(LineKind::Added, None, Some(n))).collect();
        review.files[0].hunks[0].lines[0].text = first.into();
        review
    }

    fn cursor_line(s: &ReviewSession) -> Option<u32> {
        let v = s.view()?;
        let (f, h, l) = v.rows[v.sel].lines().next()?;
        v.review.files[f].hunks[h].lines[l].new_lineno
    }

    #[test]
    fn a_read_again_keeps_the_cursor_on_its_line_not_the_top_of_its_file() {
        let (mut s, _) = working();
        s.follow(Some(0));
        s.receive(three_lines(1, "one"));
        s.receive(answer(2, Staged, &[]));
        s.view_mut().unwrap().move_by(2);
        assert_eq!(cursor_line(&s), Some(3));
        s.follow(Some(1));
        s.catch_up(|| 3);
        s.receive(three_lines(3, "one, edited"));
        assert_eq!(cursor_line(&s), Some(3));
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
    fn the_anchor_is_the_picked_lines_of_the_side_on_screen() {
        let (s, _) = working();
        assert!(s.anchor().is_none(), "nothing read yet");
        let s = showing_a_line();
        assert_eq!(s.anchor().map(|a| a.path), Some("a.rs".into()));
    }

    #[test]
    fn a_saved_comment_lets_its_lines_go() {
        let mut s = showing_a_line();
        s.view_mut().unwrap().toggle_mark();
        s.commented();
        assert!(s.view().unwrap().mark.is_none());
    }

    // --- folds --------------------------------------------------------------

    fn numbered(kind: LineKind, old: Option<u32>, new: Option<u32>) -> DiffLine {
        DiffLine { kind, old_lineno: old, new_lineno: new, text: String::new(), spans: Vec::new() }
    }

    /// A ten-line file with a line added at 2: seven lines lie below the
    /// hunk, read from blob `blob`.
    fn foldable(request_id: u64, blob: &str) -> Review {
        let mut review = answer(request_id, Unstaged, &["a.rs"]);
        let file = &mut review.files[0];
        file.hunks[0].lines = vec![
            numbered(LineKind::Context, Some(1), Some(1)),
            numbered(LineKind::Added, None, Some(2)),
            numbered(LineKind::Context, Some(2), Some(3)),
        ];
        file.new_blob = Some(blob.into());
        file.new_lines = Some(10);
        review
    }

    fn on_the_fold(s: &mut ReviewSession) {
        let view = s.view_mut().unwrap();
        view.bottom_of_diff();
        view.move_by(1);
    }

    fn below(request_id: u64) -> ReviewContext {
        let lines = (4..=10).map(|n| numbered(LineKind::Context, None, Some(n))).collect();
        ReviewContext { request_id, checkout: CO, lines }
    }

    #[test]
    fn opening_a_fold_asks_for_its_lines_from_the_blob_and_splices_them_in() {
        let (mut s, _) = working();
        s.receive(foldable(1, "b10b"));
        on_the_fold(&mut s);
        let asked = s.unfold(false, 50).unwrap();
        assert!(matches!(
            &asked,
            ClientMsg::ReviewContext { request_id: 50, checkout: CO, blob, path, start: 4, end: 10 }
                if blob == "b10b" && path == "a.rs"
        ));
        assert!(s.receive_context(below(50)));
        let lines = &s.view().unwrap().review.files[0].hunks[0].lines;
        assert_eq!(lines.len(), 10);
        assert_eq!(lines[9].old_lineno, Some(9), "numbered on the old side too");
    }

    #[test]
    fn off_a_fold_nothing_is_asked_for() {
        let (mut s, _) = working();
        s.receive(foldable(1, "b10b"));
        assert!(s.unfold(true, 50).is_none());
    }

    #[test]
    fn lines_for_a_file_read_again_since_are_dropped() {
        // A refresh read the file anew, with folds of its own.
        let (mut s, _) = working();
        s.receive(foldable(1, "b10b"));
        on_the_fold(&mut s);
        s.unfold(false, 50).unwrap();
        let mut ids = 2;
        s.refresh(|| {
            ids += 1;
            ids
        });
        s.receive(foldable(3, "c0de"));
        assert!(!s.receive_context(below(50)));
        assert_eq!(s.view().unwrap().review.files[0].hunks[0].lines.len(), 3);
    }

    #[test]
    fn only_the_latest_fold_asked_for_is_taken() {
        let (mut s, _) = working();
        s.receive(foldable(1, "b10b"));
        on_the_fold(&mut s);
        s.unfold(false, 50).unwrap();
        s.unfold(true, 51).unwrap();
        assert!(!s.receive_context(below(50)));
        assert!(!s.context_failed(50));
        assert!(s.context_failed(51));
        assert!(!s.context_failed(51), "and answered once");
    }
}
