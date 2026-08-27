//! A checkout's recent commits flattened into the rows the cursor moves
//! over. Commit headers and the files they touched are both selectable —
//! unlike the review viewer, there is no line a comment would attach to,
//! so a header is a real place to land.
//!
//! Only the headers arrive with the list. What a commit touched costs a
//! diff against its parent, so it is fetched when the cursor drills into
//! that commit and kept for as long as the overlay is open.

use argus_protocol::{CheckoutId, CommitFile, CommitInfo};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryRow {
    Commit { commit: usize },
    File { commit: usize, file: usize },
}

impl HistoryRow {
    pub fn commit(self) -> usize {
        match self {
            HistoryRow::Commit { commit } | HistoryRow::File { commit, .. } => commit,
        }
    }
}

/// What drilling into the selected row asks of the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Drill {
    /// Nothing left to unfold here: open the commit itself.
    Open,
    /// Unfolded, with the files already in hand.
    Shown,
    /// Unfolded, but this commit has never been summarized — ask for it.
    Fetch(String),
}

pub struct HistoryEntry {
    pub info: CommitInfo,
    /// What the commit touched, once it has been asked for. `None` is the
    /// state the whole list starts in.
    pub files: Option<Vec<CommitFile>>,
    pub expanded: bool,
    /// A summary request is out for this commit, so drilling in again must
    /// not send a second one.
    pub pending: bool,
}

impl HistoryEntry {
    fn new(info: CommitInfo) -> Self {
        HistoryEntry {
            info,
            files: None,
            expanded: false,
            pending: false,
        }
    }
}

pub struct HistoryView {
    pub checkout: CheckoutId,
    pub commits: Vec<HistoryEntry>,
    pub rows: Vec<HistoryRow>,
    pub sel: usize,
    pub top: usize,
}

impl HistoryView {
    pub fn new(checkout: CheckoutId, commits: Vec<CommitInfo>) -> Self {
        let commits: Vec<HistoryEntry> = commits.into_iter().map(HistoryEntry::new).collect();
        let rows = flatten(&commits);
        HistoryView {
            checkout,
            commits,
            rows,
            sel: 0,
            top: 0,
        }
    }

    pub fn move_by(&mut self, delta: isize) {
        if self.rows.is_empty() {
            return;
        }
        let next = (self.sel as isize + delta).clamp(0, self.rows.len() as isize - 1) as usize;
        self.sel = next;
    }

    pub fn jump_commit(&mut self, forward: bool) {
        let here = self.rows.get(self.sel).map(|r| r.commit());
        let candidates: Vec<usize> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| matches!(r, HistoryRow::Commit { .. }) && Some(r.commit()) != here)
            .map(|(i, _)| i)
            .collect();
        let next = if forward {
            candidates.iter().find(|&&i| i > self.sel).copied()
        } else {
            candidates.iter().rev().find(|&&i| i < self.sel).copied()
        };
        if let Some(i) = next {
            self.sel = i;
        }
    }

    pub fn top_of_list(&mut self) {
        self.sel = 0;
    }

    pub fn bottom_of_list(&mut self) {
        self.sel = self.rows.len().saturating_sub(1);
    }

    pub fn scroll_into_view(&mut self, height: usize) {
        if height == 0 {
            return;
        }
        if self.sel < self.top {
            self.top = self.sel;
        } else if self.sel >= self.top + height {
            self.top = self.sel + 1 - height;
        }
        let max_top = self.rows.len().saturating_sub(height);
        self.top = self.top.min(max_top);
    }

    pub fn selected_oid(&self) -> Option<&str> {
        Some(self.selected_commit()?.info.oid.as_str())
    }

    pub fn selected_commit(&self) -> Option<&HistoryEntry> {
        self.commits.get(self.rows.get(self.sel)?.commit())
    }

    pub fn selected_file(&self) -> Option<&CommitFile> {
        self.file_at(*self.rows.get(self.sel)?)
    }

    pub fn file_at(&self, row: HistoryRow) -> Option<&CommitFile> {
        match row {
            HistoryRow::File { commit, file } => self.commits.get(commit)?.files.as_ref()?.get(file),
            HistoryRow::Commit { .. } => None,
        }
    }

    /// One step further in: a folded commit unfolds to its file list, and
    /// anything already unfolded opens as a review.
    pub fn drill(&mut self) -> Drill {
        let Some(HistoryRow::Commit { commit }) = self.rows.get(self.sel).copied() else {
            return Drill::Open;
        };
        let Some(entry) = self.commits.get_mut(commit) else {
            return Drill::Open;
        };
        if entry.expanded {
            return Drill::Open;
        }
        entry.expanded = true;
        let fetch = if entry.files.is_none() && !entry.pending {
            entry.pending = true;
            Some(entry.info.oid.clone())
        } else {
            None
        };
        self.rebuild();
        match fetch {
            Some(oid) => Drill::Fetch(oid),
            None => Drill::Shown,
        }
    }

    /// Fold the commit the cursor is in, landing on its header. False when
    /// there was nothing unfolded, so `h` can close the overlay instead.
    pub fn collapse(&mut self) -> bool {
        let Some(row) = self.rows.get(self.sel).copied() else {
            return false;
        };
        let Some(entry) = self.commits.get_mut(row.commit()) else {
            return false;
        };
        if !entry.expanded {
            return false;
        }
        entry.expanded = false;
        self.sel = self
            .rows
            .iter()
            .position(|r| matches!(r, HistoryRow::Commit { commit } if *commit == row.commit()))
            .unwrap_or(self.sel);
        self.rebuild();
        true
    }

    /// A summary arrived. False when it names a commit this view no longer
    /// has — a reply that outlived the list it was asked for.
    pub fn receive_files(&mut self, oid: &str, files: Vec<CommitFile>) -> bool {
        let Some(entry) = self.entry_mut(oid) else {
            return false;
        };
        entry.pending = false;
        entry.files = Some(files);
        self.rebuild();
        true
    }

    /// A summary failed. The commit folds back up: an unfolded header with
    /// nothing under it reads as a commit that touched no files.
    pub fn fail_files(&mut self, oid: &str) -> bool {
        let Some(entry) = self.entry_mut(oid) else {
            return false;
        };
        entry.pending = false;
        entry.expanded = false;
        self.rebuild();
        true
    }

    fn entry_mut(&mut self, oid: &str) -> Option<&mut HistoryEntry> {
        self.commits.iter_mut().find(|c| c.info.oid == oid)
    }

    /// Rows are index-addressed, so re-flattening has to put the cursor
    /// back on the row it was on, not the position that row used to hold.
    fn rebuild(&mut self) {
        let anchor = self.rows.get(self.sel).copied();
        self.rows = flatten(&self.commits);
        self.sel = anchor
            .and_then(|row| {
                self.rows
                    .iter()
                    .position(|r| *r == row)
                    // A file row that folded away: its header is where the
                    // cursor belongs.
                    .or_else(|| self.rows.iter().position(|r| r.commit() == row.commit()))
            })
            .unwrap_or(0)
            .min(self.rows.len().saturating_sub(1));
    }
}

fn flatten(commits: &[HistoryEntry]) -> Vec<HistoryRow> {
    let mut rows = Vec::new();
    for (c, commit) in commits.iter().enumerate() {
        rows.push(HistoryRow::Commit { commit: c });
        if !commit.expanded {
            continue;
        }
        let files = commit.files.as_ref().map_or(0, Vec::len);
        for f in 0..files {
            rows.push(HistoryRow::File { commit: c, file: f });
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_protocol::ChangeKind;

    fn info(oid: &str, summary: &str) -> CommitInfo {
        CommitInfo {
            oid: oid.to_string(),
            short: oid.chars().take(7).collect(),
            summary: summary.to_string(),
            author: "t".to_string(),
            time: 0,
        }
    }

    fn file(path: &str) -> CommitFile {
        CommitFile {
            path: path.to_string(),
            old_path: None,
            kind: ChangeKind::Modified,
            added: 1,
            removed: 1,
        }
    }

    fn two_commits() -> HistoryView {
        HistoryView::new(
            CheckoutId(1),
            vec![info("aaaaaaa111", "newest"), info("bbbbbbb222", "older")],
        )
    }

    /// The cursor on the newest commit and nothing unfolded: what one
    /// `git log` walk answers without diffing a single commit.
    #[test]
    fn history_arrives_as_headers_alone() {
        let v = two_commits();
        assert_eq!(v.rows.len(), 2);
        assert!(matches!(v.rows[0], HistoryRow::Commit { commit: 0 }));
        assert!(matches!(v.rows[1], HistoryRow::Commit { commit: 1 }));
        assert_eq!(v.selected_oid(), Some("aaaaaaa111"));
        assert!(v.selected_file().is_none());
    }

    #[test]
    fn drilling_into_a_commit_asks_for_its_files_once() {
        let mut v = two_commits();
        assert_eq!(v.drill(), Drill::Fetch("aaaaaaa111".to_string()));
        // No rows yet: there is nothing to show until the reply lands.
        assert_eq!(v.rows.len(), 2);

        assert!(v.receive_files("aaaaaaa111", vec![file("a.rs"), file("b.rs")]));
        assert_eq!(v.rows.len(), 4);
        assert!(matches!(v.rows[1], HistoryRow::File { commit: 0, file: 0 }));
        assert!(matches!(v.rows[3], HistoryRow::Commit { commit: 1 }));

        // The cursor never left the header, and drilling again opens it.
        assert_eq!(v.sel, 0);
        assert_eq!(v.drill(), Drill::Open);
    }

    #[test]
    fn folding_and_unfolding_again_reuses_the_summary() {
        let mut v = two_commits();
        v.drill();
        v.receive_files("aaaaaaa111", vec![file("a.rs")]);
        assert!(v.collapse());
        assert_eq!(v.rows.len(), 2);
        assert_eq!(v.drill(), Drill::Shown);
        assert_eq!(v.rows.len(), 3);
    }

    #[test]
    fn drilling_twice_before_the_reply_sends_one_request() {
        let mut v = two_commits();
        assert!(matches!(v.drill(), Drill::Fetch(_)));
        assert!(v.collapse());
        assert_eq!(v.drill(), Drill::Shown, "the first request is still out");
    }

    #[test]
    fn folding_from_a_file_row_lands_on_the_header() {
        let mut v = two_commits();
        v.drill();
        v.receive_files("aaaaaaa111", vec![file("a.rs"), file("b.rs")]);
        v.move_by(2);
        assert_eq!(v.selected_file().unwrap().path, "b.rs");

        assert!(v.collapse());
        assert!(matches!(v.rows[v.sel], HistoryRow::Commit { commit: 0 }));
        assert!(!v.collapse(), "a folded header has nothing left to fold");
    }

    #[test]
    fn files_landing_above_the_cursor_carry_it_along() {
        let mut v = two_commits();
        v.drill();
        v.move_by(1);
        assert!(matches!(v.rows[v.sel], HistoryRow::Commit { commit: 1 }));

        v.receive_files("aaaaaaa111", vec![file("a.rs"), file("b.rs")]);
        assert!(
            matches!(v.rows[v.sel], HistoryRow::Commit { commit: 1 }),
            "the cursor stays on the older commit, not on a new file row"
        );
    }

    #[test]
    fn a_failed_summary_folds_the_commit_back_up() {
        let mut v = two_commits();
        v.drill();
        assert!(v.fail_files("aaaaaaa111"));
        assert!(!v.commits[0].expanded);
        // Askable again, rather than stuck pending forever.
        assert_eq!(v.drill(), Drill::Fetch("aaaaaaa111".to_string()));
    }

    #[test]
    fn a_summary_for_an_unknown_commit_is_dropped() {
        let mut v = two_commits();
        assert!(!v.receive_files("ccccccc333", vec![file("c.rs")]));
        assert!(!v.fail_files("ccccccc333"));
        assert_eq!(v.rows.len(), 2);
    }

    #[test]
    fn jumping_commits_lands_on_headers() {
        let mut v = two_commits();
        v.jump_commit(true);
        assert!(matches!(v.rows[v.sel], HistoryRow::Commit { commit: 1 }));
        v.jump_commit(false);
        assert!(matches!(v.rows[v.sel], HistoryRow::Commit { commit: 0 }));
    }

    #[test]
    fn a_file_row_names_the_path_and_keeps_the_commit() {
        let mut v = two_commits();
        v.drill();
        v.receive_files("aaaaaaa111", vec![file("a.rs")]);
        v.move_by(1);
        assert_eq!(v.selected_file().unwrap().path, "a.rs");
        assert_eq!(v.selected_oid(), Some("aaaaaaa111"));
        assert_eq!(v.drill(), Drill::Open, "a file row opens the commit");
    }
}
