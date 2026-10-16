//! What the history dialog holds — a checkout's newest commits, the ones
//! unfolded to their files, the filter over them, and the diff of the file
//! under the cursor — and what its gestures ask of the daemon. The rows
//! and what unfolding asks for are `argus_protocol::history_view`'s, as in
//! the terminal client; a commit's diff is asked for once and kept while
//! the dialog is open; a diff that could not be read says why, and is
//! asked for again when the cursor comes back to it.

use std::collections::HashMap;

use argus_protocol::history_view::{Drill, HistoryRow, HistoryView};
use argus_protocol::review_view::ReviewView;
use argus_protocol::{CheckoutId, CheckoutInfo, ClientMsg, CommitFile, CommitInfo, Review, ReviewBase};

use crate::page::{HistoryPage, HistoryRowView};
use crate::review::{file_rows, files_of};

pub struct HistoryDialog {
    checkout: CheckoutId,
    name: String,
    branch: String,
    asked: u64,
    view: Option<HistoryView>,
    failed: Option<String>,
    /// Each opened commit's diff against its parent, by oid.
    diffs: HashMap<String, ReviewView>,
    /// Diffs asked for and not yet answered: request id to oid. The daemon
    /// answers every ask, so an entry leaves on a diff or a failure.
    pending: HashMap<u64, String>,
    /// Commits whose diff could not be read, by oid, with why. The cursor
    /// landing on one of their files again asks again.
    unread: HashMap<String, String>,
    /// What the filter field says: the commits shown are those whose hash,
    /// subject or author holds every word of it.
    filter: String,
    /// A commit the dialog was opened for, by the hash an agent gave: the
    /// cursor lands on it when the list arrives.
    want: Option<String>,
}

/// What a gesture in the dialog comes to.
pub enum Then {
    Nothing,
    Ask(Box<ClientMsg>),
    /// Open this commit as a review, where comments record it, on the file
    /// the cursor was on when it was on one.
    Review { commit: String, file: Option<String> },
}

impl HistoryDialog {
    pub fn open(checkout: &CheckoutInfo, request_id: u64) -> (HistoryDialog, ClientMsg) {
        let dialog = HistoryDialog {
            checkout: checkout.id,
            name: checkout.name.clone(),
            branch: checkout.git.as_ref().and_then(|g| g.branch.clone()).unwrap_or_default(),
            asked: request_id,
            view: None,
            failed: None,
            diffs: HashMap::new(),
            pending: HashMap::new(),
            unread: HashMap::new(),
            filter: String::new(),
            want: None,
        };
        (dialog, ClientMsg::ListCommits { request_id, checkout: checkout.id })
    }

    /// Opens on `sha`, a full hash or a prefix of one. A commit older than
    /// the list the daemon sends leaves the cursor at the top.
    pub fn open_at(checkout: &CheckoutInfo, request_id: u64, sha: &str) -> (HistoryDialog, ClientMsg) {
        let (mut dialog, msg) = HistoryDialog::open(checkout, request_id);
        dialog.want = Some(sha.to_lowercase());
        (dialog, msg)
    }

    pub fn checkout(&self) -> CheckoutId {
        self.checkout
    }

    pub fn receive_commits(&mut self, request_id: u64, commits: Vec<CommitInfo>) {
        if request_id == self.asked {
            let mut view = HistoryView::new(self.checkout, commits);
            let landed = self.want.take().and_then(|sha| {
                let commit = view.commits.iter().position(|c| c.info.oid.starts_with(&sha))?;
                view.rows.iter().position(|r| *r == HistoryRow::Commit { commit })
            });
            view.sel = landed.unwrap_or(0);
            self.view = Some(view);
            // A filter typed while they loaded already hides some.
            self.keep_cursor_shown();
        }
    }

    pub fn commits_failed(&mut self, request_id: u64, message: String) {
        if request_id == self.asked {
            self.failed = Some(message);
        }
    }

    pub fn receive_files(&mut self, checkout: CheckoutId, commit: &str, files: Vec<CommitFile>) {
        if let Some(view) = self.view.as_mut().filter(|_| checkout == self.checkout) {
            view.receive_files(commit, files);
        }
    }

    pub fn files_failed(&mut self, checkout: CheckoutId, commit: &str) {
        if let Some(view) = self.view.as_mut().filter(|_| checkout == self.checkout) {
            view.fail_files(commit);
        }
    }

    /// A commit's diff, kept if this dialog asked for it; given back if not.
    pub fn receive_diff(&mut self, review: Review) -> Option<Review> {
        let Some(oid) = self.pending.remove(&review.request_id) else {
            return Some(review);
        };
        self.diffs.insert(oid, ReviewView::new(review, false));
        None
    }

    /// A commit's diff the daemon could not read, taken if this dialog asked
    /// for it.
    pub fn diff_failed(&mut self, request_id: u64, message: String) -> bool {
        let Some(oid) = self.pending.remove(&request_id) else {
            return false;
        };
        self.unread.insert(oid, message);
        true
    }

    /// The cursor moves over the rows the filter shows, and stops at
    /// either end of them.
    pub fn move_by(&mut self, delta: isize, id: impl FnMut() -> u64) -> Then {
        let shown = self.shown();
        if let Some(view) = &mut self.view {
            let at = shown.iter().position(|r| *r >= view.sel).unwrap_or(shown.len());
            let to = at.saturating_add_signed(delta).min(shown.len().saturating_sub(1));
            if let Some(row) = shown.get(to) {
                view.sel = *row;
            }
        }
        self.want_diff(id)
    }

    pub fn ends(&mut self, bottom: bool, id: impl FnMut() -> u64) -> Then {
        let shown = self.shown();
        if let Some(view) = &mut self.view {
            let row = if bottom { shown.last() } else { shown.first() };
            if let Some(row) = row {
                view.sel = *row;
            }
        }
        self.want_diff(id)
    }

    /// The filter field changed. A cursor left on a row it hides moves to
    /// the first it shows.
    pub fn set_filter(&mut self, filter: String, id: impl FnMut() -> u64) -> Then {
        self.filter = filter;
        self.keep_cursor_shown();
        self.want_diff(id)
    }

    fn keep_cursor_shown(&mut self) {
        let shown = self.shown();
        if let Some(view) = self.view.as_mut().filter(|v| !shown.contains(&v.sel)) {
            if let Some(first) = shown.first() {
                view.sel = *first;
            }
        }
    }

    /// The rows the filter shows: each matching commit with its files.
    fn shown(&self) -> Vec<usize> {
        let Some(view) = &self.view else {
            return Vec::new();
        };
        let words: Vec<String> = self.filter.split_whitespace().map(str::to_lowercase).collect();
        let matches = |entry: &argus_protocol::history_view::HistoryEntry| {
            let info = &entry.info;
            let text = format!("{} {} {}", info.oid, info.summary, info.author).to_lowercase();
            words.iter().all(|w| text.contains(w.as_str()))
        };
        (0..view.rows.len())
            .filter(|&i| view.commits.get(view.rows[i].commit()).is_some_and(matches))
            .collect()
    }

    pub fn select_row(&mut self, row: usize, id: impl FnMut() -> u64) -> Then {
        let shown = self.shown();
        if let Some(view) = self.view.as_mut().filter(|_| shown.contains(&row)) {
            view.sel = row;
        }
        self.want_diff(id)
    }

    /// `l`: unfolds a commit to its files, asking what they are the first
    /// time.
    pub fn unfold(&mut self) -> Then {
        let hidden = !self.cursor_shown();
        let Some(view) = self.view.as_mut().filter(|_| !hidden) else {
            return Then::Nothing;
        };
        match view.drill() {
            Drill::Fetch(commit) => Then::Ask(Box::new(ClientMsg::ListCommitFiles { checkout: self.checkout, commit })),
            Drill::Open | Drill::Shown => Then::Nothing,
        }
    }

    /// `h`: folds the commit back up.
    pub fn fold(&mut self) {
        if let Some(view) = &mut self.view {
            view.collapse();
        }
    }

    /// Enter: a commit unfolds; a file opens as a review of its commit.
    pub fn choose(&mut self) -> Then {
        match self.selected_row() {
            Some(HistoryRow::Commit { .. }) => self.unfold(),
            Some(HistoryRow::File { .. }) => self.as_review(),
            None => Then::Nothing,
        }
    }

    /// The commit under the cursor, as a review.
    pub fn as_review(&self) -> Then {
        match self.view.as_ref().filter(|_| self.cursor_shown()).and_then(HistoryView::selected_oid) {
            Some(oid) => Then::Review {
                commit: oid.to_owned(),
                file: self.view.as_ref().and_then(HistoryView::selected_file).map(|f| f.path.clone()),
            },
            None => Then::Nothing,
        }
    }

    /// `c`: the commit under the cursor, and the file when the cursor is on
    /// one, to be read in the Code stage at that commit.
    pub fn code_target(&self) -> Option<(CheckoutId, String, Option<String>)> {
        let view = self.view.as_ref().filter(|_| self.cursor_shown())?;
        let oid = view.selected_oid()?.to_owned();
        Some((self.checkout, oid, view.selected_file().map(|f| f.path.clone())))
    }

    fn selected_row(&self) -> Option<HistoryRow> {
        self.view.as_ref().filter(|_| self.cursor_shown()).and_then(|v| v.rows.get(v.sel).copied())
    }

    /// Whether the filter shows the cursor's row: one it hides is acted
    /// on by nothing.
    fn cursor_shown(&self) -> bool {
        self.view.as_ref().is_some_and(|v| self.shown().contains(&v.sel))
    }

    /// The cursor on a file of a commit whose diff has not been asked for:
    /// ask, once.
    fn want_diff(&mut self, mut id: impl FnMut() -> u64) -> Then {
        let Some(HistoryRow::File { .. }) = self.selected_row() else {
            return Then::Nothing;
        };
        let Some(oid) = self.view.as_ref().and_then(HistoryView::selected_oid).map(str::to_owned) else {
            return Then::Nothing;
        };
        if self.diffs.contains_key(&oid) || self.pending.values().any(|o| *o == oid) {
            return Then::Nothing;
        }
        self.unread.remove(&oid);
        let request_id = id();
        self.pending.insert(request_id, oid.clone());
        Then::Ask(Box::new(ClientMsg::Review {
            request_id,
            checkout: self.checkout,
            base: ReviewBase::Commit,
            commit: Some(oid),
        }))
    }

    pub fn page(&self, now: u64) -> HistoryPage {
        let Some(view) = &self.view else {
            return HistoryPage {
                open: true,
                name: self.name.clone(),
                branch: self.branch.clone(),
                filter: self.filter.clone(),
                loading: self.failed.is_none(),
                empty: self.failed.clone(),
                ..HistoryPage::default()
            };
        };
        let shown = self.shown();
        let rows: Vec<HistoryRowView> = view
            .rows
            .iter()
            .enumerate()
            .filter(|(i, _)| shown.contains(i))
            .filter_map(|(i, row)| {
                let entry = view.commits.get(row.commit())?;
                let on = i == view.sel;
                Some(match *row {
                    HistoryRow::Commit { .. } => HistoryRowView {
                        row: i,
                        kind: "commit",
                        short: entry.info.short.clone(),
                        text: entry.info.summary.clone(),
                        ago: argus_protocol::ago(u64::try_from(entry.info.time).ok(), now),
                        files: entry.files.as_ref().map(Vec::len),
                        loading: entry.pending,
                        expanded: entry.expanded,
                        added: 0,
                        removed: 0,
                        on,
                    },
                    HistoryRow::File { .. } => {
                        let file = view.file_at(*row)?;
                        HistoryRowView {
                            row: i,
                            kind: "file",
                            short: String::new(),
                            text: file.path.clone(),
                            ago: String::new(),
                            files: None,
                            loading: false,
                            expanded: false,
                            added: file.added,
                            removed: file.removed,
                            on,
                        }
                    }
                })
            })
            .collect();
        // The diff of the file under the cursor, from its commit's diff;
        // none while the filter hides the cursor's row.
        let on = shown.contains(&view.sel);
        let file = view.selected_file().filter(|_| on).map(|f| f.path.clone());
        let commit = view.selected_commit().filter(|_| on).map(|c| c.info.clone());
        let empty = if view.commits.is_empty() {
            Some(format!("No commits in {} yet.", self.name))
        } else if rows.is_empty() {
            Some(format!("No commit matches “{}”.", self.filter.trim()))
        } else {
            None
        };
        let held = view.selected_oid().filter(|_| file.is_some());
        let diff = held.and_then(|oid| self.diffs.get(oid));
        let unread = held.and_then(|oid| self.unread.get(oid));
        let shown = match (diff, &file) {
            (Some(d), Some(path)) => d.review.files.iter().position(|f| &f.path == path).map(|at| (d, at)),
            _ => None,
        };
        let diff_empty = match (unread, &file) {
            (Some(why), _) => Some(why.clone()),
            (None, Some(path)) if diff.is_some() && shown.is_none() => Some(format!("{path} is not in this commit's diff.")),
            _ => None,
        };
        HistoryPage {
            open: true,
            name: self.name.clone(),
            branch: self.branch.clone(),
            filter: self.filter.clone(),
            loading: false,
            empty,
            rows,
            commit: commit.map(|c| c.short),
            path: file.clone(),
            // A preview opens no folds; the review it opens into does.
            diff: shown
                .map(|(d, at)| file_rows(d, at, None, |_| false))
                .unwrap_or_default()
                .into_iter()
                .filter(|r| r.kind != "fold")
                .collect(),
            // Neither held nor refused: asked and not yet answered, since
            // every landing on a file row asks.
            diff_loading: file.is_some() && diff.is_none() && unread.is_none(),
            diff_empty,
            files: shown.map(|(d, _)| files_of(d).len()).unwrap_or(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::checkout;
    use argus_protocol::{ChangeKind, DiffLine, FileDiff, Hunk, LineKind};

    fn commit(oid: &str, summary: &str) -> CommitInfo {
        CommitInfo { oid: oid.into(), short: oid[..4].into(), summary: summary.into(), author: "t".into(), time: 0 }
    }

    fn opened() -> HistoryDialog {
        let (mut d, msg) = HistoryDialog::open(&checkout(100, "main", true, Vec::new()), 5);
        assert!(matches!(msg, ClientMsg::ListCommits { request_id: 5, .. }));
        d.receive_commits(5, vec![commit("aaaa1", "Newest"), commit("bbbb2", "Older")]);
        d
    }

    #[test]
    fn opened_for_a_hash_the_cursor_lands_on_that_commit_and_stays_at_the_top_when_it_is_not_there() {
        let list = || vec![commit("aaaa1", "Newest"), commit("bbbb2", "Older")];
        let (mut d, _) = HistoryDialog::open_at(&checkout(100, "main", true, Vec::new()), 5, "BBB");
        d.receive_commits(5, list());
        assert!(d.page(0).rows[1].on, "a prefix, in either case, finds it");
        let (mut d, _) = HistoryDialog::open_at(&checkout(100, "main", true, Vec::new()), 5, "ffff");
        d.receive_commits(5, list());
        assert!(d.page(0).rows[0].on);
    }

    #[test]
    fn the_filter_shows_the_commits_holding_every_word_and_moves_the_cursor_onto_them() {
        let mut d = opened();
        assert!(matches!(d.set_filter("OLD".into(), || 1), Then::Nothing));
        let page = d.page(0);
        assert_eq!(page.rows.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(), ["Older"]);
        assert!(page.rows[0].on, "the cursor left the hidden row for the first shown");
        d.set_filter("older bbbb".into(), || 1);
        assert_eq!(d.page(0).rows.len(), 1, "a word of the hash counts");
        d.set_filter("older newest".into(), || 1);
        assert_eq!(d.page(0).empty.as_deref(), Some("No commit matches “older newest”."));
        assert!(matches!(d.as_review(), Then::Nothing), "nothing under a hidden cursor");
        d.set_filter(String::new(), || 1);
        assert_eq!(d.page(0).rows.len(), 2);
    }

    #[test]
    fn a_filter_typed_while_loading_lights_the_first_match() {
        let (mut d, _) = HistoryDialog::open(&checkout(100, "main", true, Vec::new()), 5);
        d.set_filter("fix".into(), || 1);
        d.receive_commits(5, vec![commit("aaaa1", "Other"), commit("bbbb2", "Fix one"), commit("cccc3", "Fix two")]);
        assert!(d.page(0).rows[0].on, "the first match is lit");
        assert!(matches!(d.as_review(), Then::Review { commit, .. } if commit == "bbbb2"));
        d.move_by(1, || 1);
        assert!(d.page(0).rows[1].on, "j steps to the second match, not past it");
    }

    #[test]
    fn the_cursor_moves_over_only_the_rows_the_filter_shows() {
        let (mut d, _) = HistoryDialog::open(&checkout(100, "main", true, Vec::new()), 5);
        d.receive_commits(5, vec![commit("aaaa1", "Fix one"), commit("bbbb2", "Other"), commit("cccc3", "Fix two")]);
        d.set_filter("fix".into(), || 1);
        d.move_by(1, || 1);
        assert!(d.page(0).rows[1].on, "past the hidden commit");
        d.move_by(1, || 1);
        assert!(d.page(0).rows[1].on, "held at the last shown");
        d.ends(false, || 1);
        assert!(d.page(0).rows[0].on);
        assert!(matches!(d.select_row(1, || 1), Then::Nothing));
        assert!(d.page(0).rows[0].on, "a hidden row cannot be clicked");
    }

    #[test]
    fn unfolding_a_commit_asks_for_its_files_once() {
        let mut d = opened();
        assert!(matches!(d.unfold(), Then::Ask(msg) if matches!(&*msg, ClientMsg::ListCommitFiles { commit, .. } if commit == "aaaa1")));
        assert!(d.page(0).rows[0].loading);
        d.receive_files(CheckoutId(100), "aaaa1", vec![CommitFile {
            path: "DESKTOP.md".into(),
            old_path: None,
            kind: ChangeKind::Modified,
            added: 3,
            removed: 1,
        }]);
        let page = d.page(0);
        assert_eq!(page.rows.iter().map(|r| r.kind).collect::<Vec<_>>(), ["commit", "file", "commit"]);
    }

    #[test]
    fn a_file_under_the_cursor_shows_its_commits_diff_asked_for_once() {
        let mut d = opened();
        d.unfold();
        d.receive_files(CheckoutId(100), "aaaa1", vec![CommitFile {
            path: "DESKTOP.md".into(),
            old_path: None,
            kind: ChangeKind::Modified,
            added: 1,
            removed: 0,
        }]);
        let mut ids = 40..;
        let Then::Ask(msg) = d.move_by(1, || ids.next().unwrap()) else {
            panic!("the diff is asked for");
        };
        let ClientMsg::Review { request_id, commit, base, .. } = *msg else {
            panic!("a review of the commit");
        };
        assert_eq!((base, commit.as_deref()), (ReviewBase::Commit, Some("aaaa1")));
        assert!(d.page(0).diff_loading);
        assert!(matches!(d.select_row(1, || 99), Then::Nothing), "asked once");
        assert!(d.receive_diff(diff_of(request_id, "DESKTOP.md")).is_none());
        let page = d.page(0);
        assert_eq!(page.diff.len(), 2, "its hunk and its line");
        assert!(matches!(
            d.choose(),
            Then::Review { commit, file } if commit == "aaaa1" && file.as_deref() == Some("DESKTOP.md")
        ));
    }

    fn file(path: &str) -> CommitFile {
        CommitFile { path: path.into(), old_path: None, kind: ChangeKind::Modified, added: 1, removed: 0 }
    }

    /// A commit's diff, answering `request_id`, that changed `path`.
    fn diff_of(request_id: u64, path: &str) -> Review {
        Review {
            request_id,
            checkout: CheckoutId(100),
            base: ReviewBase::Commit,
            files: vec![FileDiff {
                path: path.into(),
                old_path: None,
                kind: ChangeKind::Modified,
                hunks: vec![Hunk {
                    header: "@@ -1 +1,2 @@".into(),
                    lines: vec![DiffLine { kind: LineKind::Added, old_lineno: None, new_lineno: Some(2), text: "more".into(), spans: Vec::new() }],
                }],
                note: None,
                new_blob: None,
                new_lines: None,
            }],
            commit: None,
        }
    }

    /// The id a gesture asked a commit's diff under.
    fn asked(then: Then) -> u64 {
        let Then::Ask(msg) = then else {
            panic!("the diff is asked for");
        };
        let ClientMsg::Review { request_id, .. } = *msg else {
            panic!("a review of the commit");
        };
        request_id
    }

    /// The newest commit unfolded to its one file, with the cursor on it.
    fn on_a_file() -> (HistoryDialog, u64) {
        let mut d = opened();
        d.unfold();
        d.receive_files(CheckoutId(100), "aaaa1", vec![file("DESKTOP.md")]);
        let id = asked(d.move_by(1, || 40));
        (d, id)
    }

    #[test]
    fn a_commit_diff_that_failed_says_why_and_is_asked_again_when_the_cursor_comes_back() {
        let (mut d, id) = on_a_file();
        assert_eq!(id, 40);
        assert!(d.diff_failed(40, "bad object".into()));
        let page = d.page(0);
        assert!(!page.diff_loading, "not waiting on what was refused");
        assert_eq!(page.diff_empty.as_deref(), Some("bad object"));

        assert!(matches!(d.ends(false, || 99), Then::Nothing), "the commit row asks nothing");
        let Then::Ask(msg) = d.move_by(1, || 41) else {
            panic!("asked again");
        };
        assert!(matches!(*msg, ClientMsg::Review { request_id: 41, ref commit, .. } if commit.as_deref() == Some("aaaa1")));
        let page = d.page(0);
        assert!(page.diff_loading && page.diff_empty.is_none());
    }

    /// The history's stuck preview: the cursor crossed two commits' files
    /// before either diff came, and both must still be kept.
    #[test]
    fn two_commits_asked_back_to_back_each_keep_their_diff() {
        let mut d = opened();
        d.unfold();
        d.receive_files(CheckoutId(100), "aaaa1", vec![file("a.md")]);
        d.select_row(2, || 99);
        d.unfold();
        d.receive_files(CheckoutId(100), "bbbb2", vec![file("b.md")]);
        let a = asked(d.select_row(1, || 40));
        let b = asked(d.select_row(3, || 41));

        assert!(d.receive_diff(diff_of(b, "b.md")).is_none());
        assert!(d.receive_diff(diff_of(a, "a.md")).is_none());
        assert!(matches!(d.select_row(1, || 99), Then::Nothing), "already held");
        let page = d.page(0);
        assert_eq!((page.path.as_deref(), page.diff.len(), page.diff_loading), (Some("a.md"), 2, false));
    }

    #[test]
    fn a_diff_this_dialog_did_not_ask_for_is_given_back() {
        let (mut d, _) = on_a_file();
        assert!(d.receive_diff(diff_of(7, "DESKTOP.md")).is_some());
        assert!(!d.diff_failed(7, "not ours".into()));
        assert!(d.page(0).diff_loading, "its own ask still waits");
    }

    #[test]
    fn a_file_its_commits_diff_does_not_hold_says_so_rather_than_loading() {
        let (mut d, id) = on_a_file();
        assert!(d.receive_diff(diff_of(id, "OTHER.md")).is_none());
        let page = d.page(0);
        assert!(!page.diff_loading);
        assert_eq!(page.diff_empty.as_deref(), Some("DESKTOP.md is not in this commit's diff."));
    }
}
