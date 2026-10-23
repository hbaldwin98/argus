//! What the Changes pane asks of a checkout's index and of `git commit`:
//! a changed line's box ticked, files moved between its lists, a hunk
//! taken whole, and a commit; which of them the daemon has yet to answer,
//! and the lists themselves, built from both sides of the review at once.
//!
//! Nothing here changes what is drawn as staged: the boxes and the lists
//! move when the daemon's next diff says the index moved (the tree's
//! `changes`, which the review follows), so a refused write leaves the
//! pane showing what is true.

use argus_protocol::review::session::ReviewSession;
use argus_protocol::review::staging::picked_lines;
use argus_protocol::review::view::{ReviewView, Row};
use argus_protocol::{ChangeKind, CheckoutId, ClientMsg, IndexChange, ReviewBase};
use serde::{Deserialize, Serialize};

use crate::review::dialog::files_of;
use crate::connection::page::ReviewFile;

/// What was done to stage or unstage, as the window sends it.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", tag = "on")]
pub enum StageGesture {
    /// A file's row double-clicked, or its button pressed: across to the
    /// other list. `staged` is the list it is in.
    File { staged: bool, path: String },
    /// Files dropped on a list, or that list's button: into the index when
    /// `staged`, the list they go to, else out of it.
    Files { staged: bool, paths: Vec<String> },
    /// Changed lines' boxes on the diff on screen, by their rows: ticked
    /// into the index from the working side, given back from the staged.
    Rows { rows: Vec<usize> },
    /// A hunk's button: every changed line under the header at `row`.
    Hunk { row: usize },
}

/// One of the Changes pane's lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ReviewSection {
    #[cfg_attr(test, ts(type = "\"staged\" | \"unstaged\" | \"untracked\""))]
    pub side: &'static str,
    pub files: Vec<SectionFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SectionFile {
    pub file: ReviewFile,
    /// Some of it is staged and some not, so it is in both lists.
    pub partly: bool,
    /// It is the file on screen, from this list.
    pub on: bool,
}

/// What the pane draws of staging: whether the boxes on screen are ticked
/// (the diff is the index's), and what is still out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct StagingPage {
    pub staged: bool,
    /// A write is out: the boxes wait on the diff that answers it.
    pub busy: bool,
    pub committing: bool,
    /// Commits made from this review, counted so the window empties its
    /// message once one lands and not before.
    pub commits: u64,
    /// Files the index holds for the next commit.
    pub can_commit: bool,
}

/// A write the daemon said it made.
#[derive(Debug, PartialEq, Eq)]
pub enum Landed {
    Staged,
    Committed,
}

#[derive(Debug, Default)]
pub struct Staging {
    /// Index writes out, by request id.
    out: Vec<u64>,
    committing: Option<u64>,
    commits: u64,
}

impl Staging {
    /// The message that writes `gesture` to the index, or why there is none
    /// to send.
    pub fn stage(&mut self, gesture: StageGesture, session: &ReviewSession, request_id: u64) -> Result<ClientMsg, String> {
        if session.base() == ReviewBase::Commit {
            return Err("a commit's lines are not staged".into());
        }
        let change = match gesture {
            StageGesture::File { staged, path } => moved(session, !staged, vec![path]),
            StageGesture::Files { staged, paths } => moved(session, staged, paths),
            StageGesture::Rows { rows } => lines(session, &rows)?,
            StageGesture::Hunk { row } => {
                let view = session.view().ok_or("the diff is still being read")?;
                let Some(Row::Hunk { file, hunk }) = view.rows.get(row).copied() else {
                    return Err("that is not a hunk".into());
                };
                let rows: Vec<usize> = (0..view.rows.len())
                    .filter(|&i| matches!(view.rows[i], Row::Line { file: f, hunk: h, .. } | Row::Pair { file: f, hunk: h, .. } if f == file && h == hunk))
                    .collect();
                lines(session, &rows)?
            }
        };
        if matches!(&change, IndexChange::Stage { paths } | IndexChange::Unstage { paths } if paths.is_empty()) {
            return Err("no files to move".into());
        }
        self.out.push(request_id);
        Ok(ClientMsg::Stage { request_id, checkout: session.checkout(), change })
    }

    /// What is staged, committed with `message`. One at a time: a second
    /// press while git runs the person's hooks would be a second commit.
    pub fn commit(&mut self, checkout: CheckoutId, message: &str, staged: Option<usize>, request_id: u64) -> Result<ClientMsg, String> {
        if self.committing.is_some() {
            return Err("the last commit is still being made".into());
        }
        if message.trim().is_empty() {
            return Err("a commit needs a message".into());
        }
        if staged == Some(0) {
            return Err("nothing is staged to commit".into());
        }
        self.committing = Some(request_id);
        Ok(ClientMsg::Commit { request_id, checkout, message: message.trim_end().to_owned() })
    }

    /// The daemon made the write sent under `request_id`, when it was one
    /// of these.
    pub fn written(&mut self, request_id: u64) -> Option<Landed> {
        if self.committing == Some(request_id) {
            self.committing = None;
            self.commits += 1;
            return Some(Landed::Committed);
        }
        let at = self.out.iter().position(|&id| id == request_id)?;
        self.out.remove(at);
        Some(Landed::Staged)
    }

    /// Whether the refusal was of a write sent from here; the caller says why.
    pub fn refused(&mut self, request_id: u64) -> bool {
        if self.committing == Some(request_id) {
            self.committing = None;
            return true;
        }
        let at = self.out.iter().position(|&id| id == request_id);
        at.map(|at| self.out.remove(at)).is_some()
    }

    /// A new connection answers nothing sent on the last.
    pub fn forget_sent(&mut self) {
        self.out.clear();
        self.committing = None;
    }

    pub fn page(&self, session: &ReviewSession) -> StagingPage {
        StagingPage {
            staged: session.base() == ReviewBase::Staged,
            busy: !self.out.is_empty(),
            committing: self.committing.is_some(),
            commits: self.commits,
            can_commit: session.files(ReviewBase::Staged).is_some_and(|n| n > 0),
        }
    }
}

/// `paths` into the index, or out of it, with the old name of any renamed
/// among them: half a rename moved leaves the other half where it was.
fn moved(session: &ReviewSession, into: bool, paths: Vec<String>) -> IndexChange {
    let from = if into { ReviewBase::Unstaged } else { ReviewBase::Staged };
    let mut all = paths.clone();
    if let Some(view) = session.view_of(from) {
        for file in view.review.files.iter().filter(|f| paths.contains(&f.path)) {
            if let Some(old) = file.old_path.clone().filter(|old| !all.contains(old)) {
                all.push(old);
            }
        }
    }
    if into {
        IndexChange::Stage { paths: all }
    } else {
        IndexChange::Unstage { paths: all }
    }
}

fn lines(session: &ReviewSession, rows: &[usize]) -> Result<IndexChange, String> {
    let view = session.view().ok_or("the diff is still being read")?;
    let (path, lines) = picked_lines(view, rows).ok_or("no changed line there to stage")?;
    let renamed = view.review.files.iter().any(|f| f.path == path && f.kind == ChangeKind::Renamed);
    if renamed {
        return Err("a renamed file is staged whole".into());
    }
    Ok(IndexChange::Lines { from: session.base(), path, lines })
}

/// The Changes pane's lists — staged, changed, untracked — from both sides
/// of the working changes, once each has been read; empty for a commit's
/// review. `on` is the file on screen and the side it is on.
pub fn sections(session: &ReviewSession, on: Option<&str>) -> Vec<ReviewSection> {
    if session.base() == ReviewBase::Commit {
        return Vec::new();
    }
    let paths = |view: Option<&ReviewView>| -> Vec<String> {
        view.map(|v| v.review.files.iter().map(|f| f.path.clone()).collect()).unwrap_or_default()
    };
    let staged_paths = paths(session.view_of(ReviewBase::Staged));
    let unstaged_paths = paths(session.view_of(ReviewBase::Unstaged));
    let list = |base: ReviewBase, keep: &dyn Fn(&ReviewFile) -> bool, other: &[String]| -> Vec<SectionFile> {
        let lit = (session.base() == base).then_some(on).flatten();
        session
            .view_of(base)
            .map(files_of)
            .unwrap_or_default()
            .into_iter()
            .filter(|f| keep(f))
            .map(|file| SectionFile { partly: other.contains(&file.path), on: lit == Some(file.path.as_str()), file })
            .collect()
    };
    let untracked = |f: &ReviewFile| f.kind == "untracked";
    vec![
        ReviewSection { side: "staged", files: list(ReviewBase::Staged, &|_| true, &unstaged_paths) },
        ReviewSection { side: "unstaged", files: list(ReviewBase::Unstaged, &|f| !untracked(f), &staged_paths) },
        ReviewSection { side: "untracked", files: list(ReviewBase::Unstaged, &untracked, &staged_paths) },
    ]
}

#[cfg(test)]
mod tests;
