//! What the review dialog holds — the checkout, its unstaged and staged
//! sides, the diff on screen and the lines picked, and who a comment goes
//! to, and the file being jumped to — and what its gestures ask of the
//! daemon. The rows, the cursor and what a comment anchors to are
//! `argus_protocol::review_view`'s, so a comment written here means what
//! one written in the terminal client does.

use argus_protocol::fuzzy::Fuzzy;
use argus_protocol::review_view::{marker, ReviewView, Row};
use argus_protocol::{
    CheckoutId, CheckoutInfo, ChangeKind, ClientMsg, DiffLine, HighlightKind, LineKind, PaneId, PaneKind, Review,
    ReviewBase,
};

use crate::page::{DiffRow, DiffSide, Recipient, ReviewFile, ReviewJump, ReviewPage};
use crate::settings::ReviewLayout;

pub struct ReviewDialog {
    checkout: CheckoutId,
    name: String,
    branch: String,
    base: ReviewBase,
    /// The commit a review of one commit is of, as its oid.
    commit: Option<String>,
    split: bool,
    unstaged: Option<ReviewView>,
    staged: Option<ReviewView>,
    of_commit: Option<ReviewView>,
    /// Requests out, and the side each is for.
    pending: Vec<(u64, ReviewBase)>,
    /// Why a side could not be read, in place of its diff.
    failed: Option<String>,
    composing: bool,
    recipient: Option<PaneId>,
    /// The side and layout the person chose, which the next review opens
    /// on. Not always the side on screen: one that gave way from an empty
    /// side was not chosen.
    chosen: ReviewLayout,
    /// Whether the side on screen is settled: until both sides are read
    /// and nobody has picked one, an empty side may still give way.
    settled: bool,
    jump: Option<Jump>,
}

/// The Ctrl-P field: what is typed, and which of its matches is lit.
#[derive(Default)]
struct Jump {
    query: String,
    at: usize,
}

impl ReviewDialog {
    /// A review of `checkout`'s working changes, on the side and in the
    /// layout last chosen. Both sides are asked for at once so the toggle
    /// can say how many files each has, and an empty one can give way.
    pub fn working(
        checkout: &CheckoutInfo,
        layout: ReviewLayout,
        mut id: impl FnMut() -> u64,
    ) -> (ReviewDialog, Vec<ClientMsg>) {
        let base = if layout.staged { ReviewBase::Staged } else { ReviewBase::Unstaged };
        let mut dialog = ReviewDialog::new(checkout, base, None, layout);
        let msgs = [ReviewBase::Unstaged, ReviewBase::Staged]
            .into_iter()
            .map(|base| dialog.ask(base, id()))
            .collect();
        (dialog, msgs)
    }

    /// One commit against its first parent, where a comment records the
    /// commit it was made on.
    pub fn of_commit(checkout: &CheckoutInfo, commit: String, layout: ReviewLayout, id: u64) -> (ReviewDialog, ClientMsg) {
        let mut dialog = ReviewDialog::new(checkout, ReviewBase::Commit, Some(commit), layout);
        let msg = dialog.ask(ReviewBase::Commit, id);
        (dialog, msg)
    }

    fn new(checkout: &CheckoutInfo, base: ReviewBase, commit: Option<String>, layout: ReviewLayout) -> ReviewDialog {
        let recipient = agents(checkout).first().map(|r| PaneId(r.pane));
        ReviewDialog {
            checkout: checkout.id,
            name: checkout.name.clone(),
            branch: checkout.git.as_ref().and_then(|g| g.branch.clone()).unwrap_or_default(),
            base,
            commit,
            split: layout.split,
            unstaged: None,
            staged: None,
            of_commit: None,
            pending: Vec::new(),
            failed: None,
            composing: false,
            recipient,
            chosen: layout,
            settled: base == ReviewBase::Commit,
            jump: None,
        }
    }

    pub fn chosen(&self) -> ReviewLayout {
        self.chosen
    }

    pub fn checkout(&self) -> CheckoutId {
        self.checkout
    }

    fn ask(&mut self, base: ReviewBase, request_id: u64) -> ClientMsg {
        self.pending.retain(|(_, b)| *b != base);
        self.pending.push((request_id, base));
        ClientMsg::Review { request_id, checkout: self.checkout, base, commit: self.commit.clone() }
    }

    /// Reads the sides again: the agent kept working.
    pub fn refresh(&mut self, mut id: impl FnMut() -> u64) -> Vec<ClientMsg> {
        self.failed = None;
        let bases: Vec<ReviewBase> = match self.base {
            ReviewBase::Commit => vec![ReviewBase::Commit],
            _ => vec![ReviewBase::Unstaged, ReviewBase::Staged],
        };
        bases.into_iter().map(|base| self.ask(base, id())).collect()
    }

    /// A diff the daemon read, kept only if this dialog asked for it.
    pub fn receive(&mut self, review: Review) -> bool {
        let Some(at) = self.pending.iter().position(|(id, _)| *id == review.request_id) else {
            return false;
        };
        let (_, base) = self.pending.remove(at);
        let mut view = ReviewView::new(review, self.split);
        // A refresh keeps the place it was at, where that line still is.
        if let Some(old) = self.side(base) {
            let file = old.rows.get(old.sel).map(|r| r.file());
            if let Some(file) = file.filter(|f| *f < view.review.files.len()) {
                view.jump_to_file(file);
            }
        }
        *self.side_mut(base) = Some(view);
        self.settle();
        true
    }

    /// Never opens on an empty side: once both are read, a side with
    /// nothing on it gives way to one with something, as the terminal
    /// client's review does.
    fn settle(&mut self) {
        if self.settled {
            return;
        }
        let other = self.base.next();
        let (Some(here), Some(there)) = (self.side(self.base), self.side(other)) else {
            return;
        };
        if here.review.files.is_empty() && !there.review.files.is_empty() {
            self.base = other;
        }
        self.settled = true;
    }

    pub fn failed(&mut self, request_id: u64, message: String) -> bool {
        let Some(at) = self.pending.iter().position(|(id, _)| *id == request_id) else {
            return false;
        };
        self.pending.remove(at);
        self.failed = Some(message);
        true
    }

    fn side(&self, base: ReviewBase) -> Option<&ReviewView> {
        match base {
            ReviewBase::Unstaged => self.unstaged.as_ref(),
            ReviewBase::Staged => self.staged.as_ref(),
            ReviewBase::Commit => self.of_commit.as_ref(),
        }
    }

    fn side_mut(&mut self, base: ReviewBase) -> &mut Option<ReviewView> {
        match base {
            ReviewBase::Unstaged => &mut self.unstaged,
            ReviewBase::Staged => &mut self.staged,
            ReviewBase::Commit => &mut self.of_commit,
        }
    }

    fn view_mut(&mut self) -> Option<&mut ReviewView> {
        self.side_mut(self.base).as_mut()
    }

    /// `b`: the other side of the working changes. A commit has one.
    pub fn switch_side(&mut self) {
        if self.base != ReviewBase::Commit {
            self.show_side(self.base.next() == ReviewBase::Staged);
        }
    }

    pub fn show_side(&mut self, staged: bool) {
        if self.base != ReviewBase::Commit {
            self.base = if staged { ReviewBase::Staged } else { ReviewBase::Unstaged };
            self.composing = false;
            self.settled = true;
            self.chosen.staged = staged;
            if let Some(jump) = &mut self.jump {
                jump.at = 0;
            }
        }
    }

    /// `s`: both sides of a change side by side, or stacked.
    pub fn toggle_split(&mut self) {
        self.split = !self.split;
        self.chosen.split = self.split;
        let split = self.split;
        for view in [&mut self.unstaged, &mut self.staged, &mut self.of_commit].into_iter().flatten() {
            view.set_split(split);
        }
    }

    pub fn move_by(&mut self, delta: isize) {
        if let Some(view) = self.view_mut() {
            view.move_by(delta);
        }
    }

    pub fn jump_file(&mut self, forward: bool) {
        if let Some(view) = self.view_mut() {
            view.jump_file(forward);
        }
    }

    pub fn ends(&mut self, bottom: bool) {
        if let Some(view) = self.view_mut() {
            if bottom {
                view.bottom_of_diff();
            } else {
                view.top_of_diff();
            }
        }
    }

    pub fn open_file(&mut self, file: usize) {
        self.jump = None;
        if let Some(view) = self.view_mut() {
            view.jump_to_file(file);
        }
    }

    /// A click on a line: the cursor lands there, and a shift-click keeps
    /// the range's other end where it was.
    pub fn select_row(&mut self, row: usize, extend: bool) {
        let Some(view) = self.view_mut() else {
            return;
        };
        if !view.rows.get(row).is_some_and(|r| r.is_selectable()) {
            return;
        }
        if extend && view.mark.is_none() {
            view.mark = Some(view.sel);
        } else if !extend {
            view.mark = None;
        }
        view.sel = row;
    }

    /// Ctrl-P: the field over the file list, which jumps to the file typed.
    pub fn open_jump(&mut self) {
        self.jump = Some(Jump::default());
    }

    pub fn type_jump(&mut self, query: String) {
        if let Some(jump) = &mut self.jump {
            jump.query = query;
            jump.at = 0;
        }
    }

    pub fn step_jump(&mut self, by: isize) {
        let last = self.jump_matches().len().saturating_sub(1);
        if let Some(jump) = &mut self.jump {
            jump.at = jump.at.saturating_add_signed(by).min(last);
        }
    }

    /// Enter in the field: the lit match opens and the field closes.
    pub fn choose_jump(&mut self) {
        let file = self.jump.as_ref().and_then(|j| self.jump_matches().get(j.at).copied());
        self.jump = None;
        if let Some(file) = file {
            self.open_file(file);
        }
    }

    pub fn close_jump(&mut self) {
        self.jump = None;
    }

    fn jump_matches(&self) -> Vec<usize> {
        match (&self.jump, self.side(self.base)) {
            (Some(jump), Some(view)) => {
                let paths: Vec<String> = view.review.files.iter().map(|f| f.path.clone()).collect();
                Fuzzy::paths().filter(&jump.query, &paths)
            }
            _ => Vec::new(),
        }
    }

    /// `E`: where the editor opens — at the picked lines' first, else the
    /// cursor's line, else the top of the file on screen.
    pub fn editor_target(&self) -> Option<(CheckoutId, String, Option<u32>)> {
        let view = self.side(self.base)?;
        if let Some(anchor) = view.anchor() {
            return Some((self.checkout, anchor.path.clone(), anchor.preferred_start()));
        }
        let file = view.rows.get(view.sel)?.file();
        Some((self.checkout, view.review.files.get(file)?.path.clone(), None))
    }

    /// `v`: starts a range at the cursor, or lets it go.
    pub fn toggle_pick(&mut self) {
        if let Some(view) = self.view_mut() {
            view.toggle_mark();
        }
    }

    /// `c`: the comment box, under the picked lines. Without an agent to
    /// read it there is nobody to send it to.
    pub fn compose(&mut self) -> Result<(), &'static str> {
        if self.side(self.base).and_then(ReviewView::anchor).is_none() {
            return Err("pick a line to comment on");
        }
        if self.recipient.is_none() {
            return Err("no agent in this checkout to send it to");
        }
        self.composing = true;
        Ok(())
    }

    pub fn cancel_comment(&mut self) {
        self.composing = false;
    }

    pub fn choose_recipient(&mut self, pane: PaneId) {
        self.recipient = Some(pane);
    }

    /// The comment, anchored to the picked lines and addressed to the
    /// chosen agent. The box stays open until the daemon says it is saved.
    pub fn comment(&mut self, body: String) -> Result<ClientMsg, &'static str> {
        let body = body.trim().to_owned();
        if body.is_empty() {
            return Err("the comment is empty");
        }
        let anchor = self.side(self.base).and_then(ReviewView::anchor).ok_or("pick a line to comment on")?;
        let recipient = self.recipient.ok_or("no agent in this checkout to send it to")?;
        Ok(ClientMsg::ReviewComment { checkout: self.checkout, recipient, anchor: Box::new(anchor), body })
    }

    /// The daemon saved the comment: the box closes and the pick with it.
    pub fn commented(&mut self) {
        self.composing = false;
        if let Some(view) = self.view_mut() {
            view.mark = None;
        }
    }

    /// The dialog as the page draws it, given the checkout as the tree has
    /// it now (its agents may have come or gone).
    pub fn page(&self, checkout: Option<&CheckoutInfo>) -> ReviewPage {
        let recipients = checkout.map(agents).unwrap_or_default();
        let recipient = self.recipient.filter(|p| recipients.iter().any(|r| r.pane == p.0)).map(|p| p.0);
        let view = self.side(self.base);
        let files: Vec<ReviewFile> = view.map(|v| v.review.files.iter().map(file).collect()).unwrap_or_default();
        let selected = view.and_then(|v| v.rows.get(v.sel)).map(|r| r.file());
        let (from, to) = view.map(ReviewView::selection).unwrap_or((0, 0));
        let picked = |i: usize| view.is_some_and(|v| v.mark.is_some() && i >= from && i <= to);
        let rows = match (view, selected) {
            (Some(v), Some(f)) => file_rows(v, f, Some(v.sel), picked),
            _ => Vec::new(),
        };
        let empty = match view {
            _ if self.failed.is_some() => self.failed.clone(),
            Some(v) if v.review.files.is_empty() => Some(match self.base {
                ReviewBase::Unstaged => format!("Nothing unstaged in {}.", self.name),
                ReviewBase::Staged => format!("Nothing staged in {}.", self.name),
                ReviewBase::Commit => "The commit changed nothing.".to_owned(),
            }),
            _ => None,
        };
        let count = |base| self.side(base).map(|v: &ReviewView| v.review.files.len());
        ReviewPage {
            open: true,
            checkout: self.checkout.0,
            name: self.name.clone(),
            branch: self.branch.clone(),
            base: match self.base {
                ReviewBase::Unstaged => "unstaged",
                ReviewBase::Staged => "staged",
                ReviewBase::Commit => "commit",
            },
            commit: view.and_then(|v| v.review.commit.as_ref()).map(|c| format!("{}  {}", c.short, c.summary)),
            unstaged: count(ReviewBase::Unstaged),
            staged: count(ReviewBase::Staged),
            split: self.split,
            added: files.iter().map(|f| f.added).sum(),
            removed: files.iter().map(|f| f.removed).sum(),
            files,
            file: selected,
            pick: view.filter(|v| v.mark.is_some() || self.composing).and_then(ReviewView::anchor).map(|a| pick_label(&a)),
            rows,
            loading: view.is_none() && self.failed.is_none(),
            empty,
            composing: self.composing,
            recipients,
            recipient,
            jump: self.jump.as_ref().map(|j| {
                let matches = self.jump_matches();
                ReviewJump { query: j.query.clone(), at: matches.get(j.at).copied(), matches }
            }),
        }
    }
}

/// The rows of one file of `view`, the cursor's marked.
pub fn file_rows(view: &ReviewView, file: usize, cursor: Option<usize>, picked: impl Fn(usize) -> bool) -> Vec<DiffRow> {
    view.rows
        .iter()
        .enumerate()
        .filter(|(_, r)| r.file() == file && !matches!(r, Row::File { .. }))
        .map(|(i, r)| diff_row(view, i, *r, Some(i) == cursor, picked(i)))
        .collect()
}

pub fn files_of(view: &ReviewView) -> Vec<ReviewFile> {
    view.review.files.iter().map(file).collect()
}

/// The agents a comment from this checkout can go to.
fn agents(checkout: &CheckoutInfo) -> Vec<Recipient> {
    checkout
        .listed_panes()
        .filter(|p| p.kind == PaneKind::Agent)
        .map(|p| Recipient { pane: p.id.0, title: p.title.clone() })
        .collect()
}

fn file(diff: &argus_protocol::FileDiff) -> ReviewFile {
    let (dir, name) = match diff.path.rfind('/') {
        Some(at) => (diff.path[..=at].to_owned(), diff.path[at + 1..].to_owned()),
        None => (String::new(), diff.path.clone()),
    };
    ReviewFile {
        path: diff.path.clone(),
        dir,
        name,
        added: diff.added_lines(),
        removed: diff.removed_lines(),
        kind: match diff.kind {
            ChangeKind::Added => "added",
            ChangeKind::Modified => "modified",
            ChangeKind::Deleted => "deleted",
            ChangeKind::Renamed => "renamed",
            ChangeKind::Untracked => "untracked",
        },
    }
}

fn diff_row(view: &ReviewView, index: usize, row: Row, cursor: bool, picked: bool) -> DiffRow {
    let files = &view.review.files;
    let line = |file: usize, hunk: usize, line: usize| side(&files[file].hunks[hunk].lines[line]);
    let (kind, text, left, right) = match row {
        Row::Hunk { file, hunk } => ("hunk", files[file].hunks[hunk].header.clone(), None, None),
        Row::Note { file } => ("note", files[file].note.clone().unwrap_or_default(), None, None),
        Row::Line { file, hunk, line: l } => ("line", String::new(), Some(line(file, hunk, l)), None),
        Row::Pair { file, hunk, left, right } => (
            "pair",
            String::new(),
            left.map(|l| line(file, hunk, l)),
            right.map(|l| line(file, hunk, l)),
        ),
        Row::File { .. } => ("hunk", String::new(), None, None),
    };
    DiffRow { row: index, kind, text, left, right, cursor, picked }
}

/// A line with its highlighting cut into pieces here, where the byte
/// offsets the daemon sends mean what they say: the page counts in UTF-16.
fn side(l: &DiffLine) -> DiffSide {
    let mut parts = Vec::new();
    let mut at = 0usize;
    for span in &l.spans {
        let (start, end) = (span.start as usize, span.end as usize);
        if start < at || end > l.text.len() || !l.text.is_char_boundary(start) || !l.text.is_char_boundary(end) {
            continue;
        }
        if start > at {
            parts.push((l.text[at..start].to_owned(), ""));
        }
        parts.push((l.text[start..end].to_owned(), highlight(span.kind)));
        at = end;
    }
    if at < l.text.len() {
        parts.push((l.text[at..].to_owned(), ""));
    }
    DiffSide {
        old: l.old_lineno,
        new: l.new_lineno,
        marker: match marker(l.kind) {
            '+' => "+",
            '-' => "−",
            _ => " ",
        },
        kind: match l.kind {
            LineKind::Added => "added",
            LineKind::Removed => "removed",
            LineKind::Context => "context",
        },
        parts,
    }
}

fn highlight(kind: HighlightKind) -> &'static str {
    match kind {
        HighlightKind::Keyword => "k",
        HighlightKind::Str => "s",
        HighlightKind::Comment => "c",
        HighlightKind::Number | HighlightKind::Constant => "n",
        HighlightKind::Type => "t",
        HighlightKind::Function => "f",
        HighlightKind::Property => "p",
        HighlightKind::Operator | HighlightKind::Punctuation => "o",
    }
}

/// What the picked lines are, as the header and the comment box say it:
/// `new lines 39–40`, or `old line 12` for a removal.
fn pick_label(anchor: &argus_protocol::ReviewAnchor) -> String {
    let range = |side: &str, start: u32, end: u32| {
        if start == end {
            format!("{side} line {start}")
        } else {
            format!("{side} lines {start}–{end}")
        }
    };
    match (anchor.new_start, anchor.new_end, anchor.old_start, anchor.old_end) {
        (Some(s), Some(e), _, _) => range("new", s, e),
        (_, _, Some(s), Some(e)) => range("old", s, e),
        _ => anchor.path.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{checkout, pane};
    use argus_protocol::{FileDiff, Hunk, PaneStatus};

    fn line(kind: LineKind, old: Option<u32>, new: Option<u32>, text: &str) -> DiffLine {
        DiffLine { kind, old_lineno: old, new_lineno: new, text: text.into(), spans: Vec::new() }
    }

    fn diff(request_id: u64, base: ReviewBase) -> Review {
        Review {
            request_id,
            checkout: CheckoutId(100),
            base,
            files: vec![FileDiff {
                path: "src/brief.rs".into(),
                old_path: None,
                kind: ChangeKind::Modified,
                hunks: vec![Hunk {
                    header: "@@ -1,2 +1,2 @@".into(),
                    lines: vec![
                        line(LineKind::Context, Some(1), Some(1), "fn brief() {"),
                        line(LineKind::Removed, Some(2), None, "    old()"),
                        line(LineKind::Added, None, Some(2), "    new()"),
                    ],
                }],
                note: None,
            }],
            commit: None,
        }
    }

    fn dialog() -> (ReviewDialog, Vec<ClientMsg>) {
        remembering(ReviewLayout::default())
    }

    fn remembering(layout: ReviewLayout) -> (ReviewDialog, Vec<ClientMsg>) {
        let co = checkout(100, "main", true, vec![pane(7, PaneKind::Agent, "claude", PaneStatus::Working)]);
        let mut ids = 0;
        ReviewDialog::working(&co, layout, || {
            ids += 1;
            ids
        })
    }

    fn nothing(request_id: u64, base: ReviewBase) -> Review {
        Review { files: Vec::new(), ..diff(request_id, base) }
    }

    fn files(request_id: u64, paths: &[&str]) -> Review {
        let mut review = diff(request_id, ReviewBase::Unstaged);
        let one = review.files.remove(0);
        review.files = paths.iter().map(|p| FileDiff { path: (*p).into(), ..one.clone() }).collect();
        review
    }

    #[test]
    fn a_review_opens_on_the_side_and_layout_last_chosen() {
        let (mut d, _) = remembering(ReviewLayout { staged: true, split: true });
        d.receive(diff(1, ReviewBase::Unstaged));
        d.receive(diff(2, ReviewBase::Staged));
        let page = d.page(None);
        assert_eq!((page.base, page.split), ("staged", true));
        assert_eq!(page.rows[1].kind, "pair", "split rows");
    }

    #[test]
    fn an_empty_side_gives_way_to_the_other_once_both_are_read() {
        let (mut d, _) = remembering(ReviewLayout { staged: true, split: false });
        d.receive(nothing(2, ReviewBase::Staged));
        assert_eq!(d.page(None).base, "staged", "the other side may be empty too");
        d.receive(diff(1, ReviewBase::Unstaged));
        assert_eq!(d.page(None).base, "unstaged");
        assert!(d.chosen().staged, "giving way is not choosing");
    }

    #[test]
    fn both_sides_empty_stays_on_the_remembered_one() {
        let (mut d, _) = remembering(ReviewLayout { staged: true, split: false });
        d.receive(nothing(1, ReviewBase::Unstaged));
        d.receive(nothing(2, ReviewBase::Staged));
        assert_eq!(d.page(None).empty.as_deref(), Some("Nothing staged in main."));
    }

    #[test]
    fn a_side_picked_before_both_are_read_stays_picked() {
        let (mut d, _) = dialog();
        d.show_side(true);
        d.receive(nothing(2, ReviewBase::Staged));
        d.receive(diff(1, ReviewBase::Unstaged));
        assert_eq!(d.page(None).base, "staged");
    }

    #[test]
    fn the_side_and_layout_picked_are_what_is_remembered() {
        let (mut d, _) = dialog();
        d.switch_side();
        d.toggle_split();
        assert_eq!(d.chosen(), ReviewLayout { staged: true, split: true });
    }

    #[test]
    fn a_commits_review_keeps_the_remembered_side_and_takes_the_layout() {
        let co = checkout(100, "main", true, Vec::new());
        let (mut d, _) = ReviewDialog::of_commit(&co, "aaaa1".into(), ReviewLayout { staged: true, split: false }, 1);
        d.switch_side();
        d.toggle_split();
        assert_eq!(d.chosen(), ReviewLayout { staged: true, split: true });
    }

    #[test]
    fn the_jump_field_opens_the_best_match_on_enter() {
        let (mut d, _) = dialog();
        d.receive(files(1, &["docs/prereview.md", "src/review.rs", "README.md"]));
        d.open_jump();
        assert_eq!(d.page(None).jump.unwrap().matches, [0, 1, 2], "everything, before a letter");
        d.type_jump("rev".into());
        let jump = d.page(None).jump.unwrap();
        assert_eq!((jump.matches, jump.at), (vec![1, 0], Some(1)));
        d.step_jump(5);
        assert_eq!(d.page(None).jump.unwrap().at, Some(0), "held at the last match");
        d.choose_jump();
        let page = d.page(None);
        assert!(page.jump.is_none());
        assert_eq!(page.file, Some(0));
    }

    #[test]
    fn the_jump_field_with_no_match_opens_nothing() {
        let (mut d, _) = dialog();
        d.receive(files(1, &["src/app.rs", "README.md"]));
        d.jump_file(true);
        d.open_jump();
        d.type_jump("zzz".into());
        assert_eq!(d.page(None).jump.unwrap().at, None);
        d.choose_jump();
        assert_eq!(d.page(None).file, Some(1), "where it was");
    }

    #[test]
    fn the_editor_opens_at_the_cursors_line_of_its_file() {
        let (mut d, _) = dialog();
        assert_eq!(d.editor_target(), None, "nothing read yet");
        d.receive(diff(1, ReviewBase::Unstaged));
        d.move_by(1);
        d.move_by(1);
        assert_eq!(d.editor_target(), Some((CheckoutId(100), "src/brief.rs".into(), Some(2))));
    }

    #[test]
    fn opening_asks_for_both_sides_and_shows_unstaged_once_it_arrives() {
        let (mut d, sent) = dialog();
        assert!(matches!(
            sent.as_slice(),
            [ClientMsg::Review { base: ReviewBase::Unstaged, .. }, ClientMsg::Review { base: ReviewBase::Staged, .. }]
        ));
        assert!(d.page(None).loading);
        assert!(!d.receive(diff(99, ReviewBase::Unstaged)), "an answer to someone else");
        assert!(d.receive(diff(1, ReviewBase::Unstaged)));
        let page = d.page(None);
        assert_eq!((page.unstaged, page.staged), (Some(1), None));
        assert_eq!(page.files[0].name, "brief.rs");
        assert_eq!(page.rows[0].kind, "hunk");
    }

    #[test]
    fn a_picked_range_is_said_by_the_side_it_is_on_and_anchors_the_comment() {
        let (mut d, _) = dialog();
        d.receive(diff(1, ReviewBase::Unstaged));
        d.move_by(1);
        d.toggle_pick();
        assert_eq!(d.page(None).pick.as_deref(), Some("old line 2"), "a removal is on the old side");
        d.move_by(1);
        assert_eq!(d.page(None).pick.as_deref(), Some("new line 2"), "with an addition, the new side");
        d.compose().unwrap();
        let ClientMsg::ReviewComment { recipient, anchor, body, .. } = d.comment(" tidy ".into()).unwrap() else {
            panic!()
        };
        assert_eq!((recipient, body.as_str()), (PaneId(7), "tidy"));
        assert_eq!(anchor.text, ["-    old()", "+    new()"]);
    }

    #[test]
    fn a_comment_needs_an_agent_and_words() {
        let co = checkout(100, "main", true, Vec::new());
        let (mut d, _) = ReviewDialog::working(&co, ReviewLayout::default(), || 1);
        d.receive(diff(1, ReviewBase::Unstaged));
        assert_eq!(d.compose(), Err("no agent in this checkout to send it to"));
        let (mut d, _) = dialog();
        d.receive(diff(1, ReviewBase::Unstaged));
        assert!(d.comment("   ".into()).is_err());
    }

    #[test]
    fn an_empty_side_says_so_rather_than_drawing_nothing() {
        let (mut d, _) = dialog();
        d.receive(nothing(2, ReviewBase::Staged));
        d.switch_side();
        assert_eq!(d.page(None).empty.as_deref(), Some("Nothing staged in main."));
    }

    #[test]
    fn highlighting_is_cut_into_pieces_on_character_boundaries() {
        let mut l = line(LineKind::Added, None, Some(1), "let é = 1;");
        l.spans = vec![
            argus_protocol::HighlightSpan { start: 0, end: 3, kind: HighlightKind::Keyword },
            argus_protocol::HighlightSpan { start: 9, end: 10, kind: HighlightKind::Number },
        ];
        let parts = side(&l).parts;
        assert_eq!(parts, [("let".into(), "k"), (" é = ".into(), ""), ("1".into(), "n"), (";".into(), "")]);
    }
}
