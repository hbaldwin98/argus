//! A checkout's recent commits flattened into the rows the cursor moves
//! over. Commit headers and the files they touched are both selectable —
//! unlike the review viewer, there is no line a comment would attach to,
//! so a header is a real place to land.

use argus_protocol::{CheckoutId, CommitFile, HistoryCommit};

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

pub struct HistoryView {
    pub checkout: CheckoutId,
    pub commits: Vec<HistoryCommit>,
    pub rows: Vec<HistoryRow>,
    pub sel: usize,
    pub top: usize,
}

impl HistoryView {
    pub fn new(checkout: CheckoutId, commits: Vec<HistoryCommit>) -> Self {
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
        let commit = self.rows.get(self.sel)?.commit();
        Some(self.commits.get(commit)?.info.oid.as_str())
    }

    pub fn selected_file(&self) -> Option<&CommitFile> {
        match self.rows.get(self.sel)? {
            HistoryRow::File { commit, file } => self.commits.get(*commit)?.files.get(*file),
            HistoryRow::Commit { .. } => None,
        }
    }
}

fn flatten(commits: &[HistoryCommit]) -> Vec<HistoryRow> {
    let mut rows = Vec::new();
    for (c, commit) in commits.iter().enumerate() {
        rows.push(HistoryRow::Commit { commit: c });
        for f in 0..commit.files.len() {
            rows.push(HistoryRow::File { commit: c, file: f });
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_protocol::{ChangeKind, CommitInfo};

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
            vec![
                HistoryCommit {
                    info: info("aaaaaaa111", "newest"),
                    files: vec![file("a.rs"), file("b.rs")],
                },
                HistoryCommit {
                    info: info("bbbbbbb222", "older"),
                    files: vec![file("c.rs")],
                },
            ],
        )
    }

    #[test]
    fn history_flattens_to_a_header_then_its_files() {
        let v = two_commits();
        assert_eq!(v.rows.len(), 5);
        assert!(matches!(v.rows[0], HistoryRow::Commit { commit: 0 }));
        assert!(matches!(v.rows[1], HistoryRow::File { commit: 0, file: 0 }));
        assert!(matches!(v.rows[3], HistoryRow::Commit { commit: 1 }));
    }

    #[test]
    fn the_cursor_starts_on_the_newest_commit() {
        let v = two_commits();
        assert_eq!(v.selected_oid(), Some("aaaaaaa111"));
        assert!(v.selected_file().is_none());
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
        v.move_by(1);
        assert_eq!(v.selected_file().unwrap().path, "a.rs");
        assert_eq!(v.selected_oid(), Some("aaaaaaa111"));
    }
}
