//! What the review dialog holds beyond the review itself — the checkout's
//! name, the side and layout the person chose, its comment box, and its
//! go-to-file field — and the page it draws. Which sides were asked for
//! and which one shows are `review::session`'s, and whether a comment may
//! go and to whom is `review::agent_comment`'s, so the dialog behaves as
//! the terminal client's review does.

use argus_protocol::review::session::{Answered, ReviewSession};
use argus_protocol::review::view::{fold_of, marker, ReviewView, Row, FOLD_STEP};
use argus_protocol::{
    CheckoutId, CheckoutInfo, ChangeKind, ClientMsg, DiffLine, HighlightKind, HighlightSpan, LineKind, PaneId, Review,
    CodeVersion, RequestRefusal, ReviewBase, ReviewContext,
};

use crate::comment_box::CommentBox;
use crate::file_jump::{FileJump, Jumped};
use crate::connection::page::{DiffRow, DiffSide, ReviewFile, ReviewJump, ReviewJumpGesture, ReviewPage};
use crate::settings::remembered::ReviewLayout;

pub struct ReviewDialog {
    session: ReviewSession,
    name: String,
    branch: String,
    comment_box: CommentBox,
    /// The side and layout the person chose, which the next review opens
    /// on. Not always the side on screen: one that gave way from an empty
    /// side was not chosen.
    chosen: ReviewLayout,
    jump: Option<FileJump>,
}

impl ReviewDialog {
    /// A review of `checkout`'s working changes, on the side and in the
    /// layout last chosen.
    pub fn working(checkout: &CheckoutInfo, layout: ReviewLayout, id: impl FnMut() -> u64) -> (ReviewDialog, Vec<ClientMsg>) {
        let base = if layout.staged { ReviewBase::Staged } else { ReviewBase::Unstaged };
        let (session, msgs) = ReviewSession::working(checkout.id, base, layout.split, id);
        (ReviewDialog::new(checkout, session, layout), msgs)
    }

    /// One commit against its first parent, where a comment records the
    /// commit it was made on, opening on `file` when one was picked.
    pub fn of_commit(
        checkout: &CheckoutInfo,
        commit: String,
        file: Option<String>,
        layout: ReviewLayout,
        id: u64,
    ) -> (ReviewDialog, ClientMsg) {
        let (session, msg) = ReviewSession::of_commit(checkout.id, commit, file, layout.split, id);
        (ReviewDialog::new(checkout, session, layout), msg)
    }

    fn new(checkout: &CheckoutInfo, session: ReviewSession, layout: ReviewLayout) -> ReviewDialog {
        ReviewDialog {
            session,
            name: checkout.name.clone(),
            branch: checkout.git.as_ref().and_then(|g| g.branch.clone()).unwrap_or_default(),
            comment_box: CommentBox::default(),
            chosen: layout,
            jump: None,
        }
    }

    pub fn chosen(&self) -> ReviewLayout {
        self.chosen
    }

    pub fn checkout(&self) -> CheckoutId {
        self.session.checkout()
    }

    /// Reads the sides again: the agent kept working.
    pub fn refresh(&mut self, id: impl FnMut() -> u64) -> Vec<ClientMsg> {
        self.session.refresh(id)
    }

    /// A diff the daemon read, kept only if this dialog asked for it.
    pub fn receive(&mut self, review: Review) -> Answered {
        self.session.receive(review)
    }

    pub fn failed(&mut self, request_id: u64, message: String) -> Answered {
        self.session.failed(request_id, message)
    }

    /// Enter on a fold, or one of its buttons (`row`, which the cursor
    /// moves to): a step of its lines, or all of them, asked for.
    pub fn unfold(&mut self, row: Option<usize>, all: bool, id: u64) -> Option<ClientMsg> {
        if let Some(row) = row {
            let view = self.view_mut()?;
            if !matches!(view.rows.get(row), Some(Row::Fold { .. })) {
                return None;
            }
            view.sel = row;
        }
        self.session.unfold(all, id)
    }

    /// A fold's lines, spliced in if they are still wanted.
    pub fn receive_context(&mut self, context: ReviewContext) -> bool {
        self.session.receive_context(context)
    }

    /// Whether a fold that could not be read was this dialog's to say so.
    pub fn context_failed(&mut self, request_id: u64) -> bool {
        self.session.context_failed(request_id)
    }

    fn view(&self) -> Option<&ReviewView> {
        self.session.view()
    }

    fn view_mut(&mut self) -> Option<&mut ReviewView> {
        self.session.view_mut()
    }

    /// `b`: the other side of the working changes.
    pub fn switch_side(&mut self) -> Result<(), String> {
        self.show_side(self.session.base().next() == ReviewBase::Staged)
    }

    pub fn show_side(&mut self, staged: bool) -> Result<(), String> {
        let base = if staged { ReviewBase::Staged } else { ReviewBase::Unstaged };
        if base == self.session.base() {
            return Ok(());
        }
        self.session.show_side(base)?;
        self.comment_box.close();
        self.chosen.staged = staged;
        if let Some(jump) = &mut self.jump {
            jump.rewind();
        }
        Ok(())
    }

    /// `s`: both sides of a change side by side, or stacked.
    pub fn toggle_split(&mut self) {
        let split = !self.session.split();
        self.session.set_split(split);
        self.chosen.split = split;
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
        if !view.rows.get(row).is_some_and(|r| r.is_stop()) {
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
        self.jump = Some(FileJump::new(usize::MAX));
    }

    /// A letter, a step, Enter or Escape in the field: Enter opens the lit
    /// match, and it and Escape close the field.
    pub fn jump(&mut self, gesture: ReviewJumpGesture) {
        let paths = self.paths();
        let Some(jump) = &mut self.jump else {
            return;
        };
        match jump.gesture(gesture, &paths) {
            Jumped::Kept => {}
            Jumped::Closed => self.jump = None,
            Jumped::Open(file) => self.open_file(file),
        }
    }

    /// The files of the side on screen, as the jump field matches them.
    fn paths(&self) -> Vec<String> {
        self.view().map(|v| v.review.files.iter().map(|f| f.path.clone()).collect()).unwrap_or_default()
    }

    /// `E`: where the editor opens — at the picked lines' first, else the
    /// cursor's line, else the top of the file on screen.
    pub fn editor_target(&self) -> Option<(CheckoutId, String, Option<u32>)> {
        let view = self.view()?;
        let checkout = self.session.checkout();
        if let Some(anchor) = view.anchor() {
            return Some((checkout, anchor.path.clone(), anchor.preferred_start()));
        }
        let file = view.rows.get(view.sel)?.file();
        Some((checkout, view.review.files.get(file)?.path.clone(), None))
    }

    /// `O`: the file the cursor is in and its line, and the version of the
    /// checkout that line is in — the working tree for unstaged work, the
    /// index for staged, the commit for one commit.
    pub fn code_target(&self) -> Option<(CheckoutId, String, u32, CodeVersion)> {
        let view = self.view()?;
        let row = *view.rows.get(view.sel)?;
        let file = view.review.files.get(row.file())?;
        // A removed line has no number in the version read, so the cursor
        // goes to the line that replaced it, or the nearest one after.
        let line = match row {
            Row::Fold { gap, .. } => fold_of(file, gap).map(|f| f.first),
            _ => row
                .lines()
                .find_map(|(f, h, l)| view.review.files[f].hunks[h].lines[l].new_lineno)
                .or_else(|| {
                    view.rows[view.sel..]
                        .iter()
                        .filter(|r| r.file() == row.file())
                        .flat_map(|r| r.lines())
                        .find_map(|(f, h, l)| view.review.files[f].hunks[h].lines[l].new_lineno)
                }),
        }
        .unwrap_or(1);
        let version = match (self.session.base(), self.session.commit()) {
            (ReviewBase::Staged, _) => CodeVersion::Index,
            (ReviewBase::Commit, Some(oid)) => CodeVersion::Commit(oid.to_owned()),
            _ => CodeVersion::WorkingTree,
        };
        Some((self.session.checkout(), file.path.clone(), line, version))
    }

    /// `v`: starts a range at the cursor, or lets it go.
    pub fn toggle_pick(&mut self) {
        if let Some(view) = self.view_mut() {
            view.toggle_mark();
        }
    }

    /// `c`: the comment box, under the picked lines, when an agent is
    /// there to read it.
    pub fn compose(&mut self, checkout: Option<&CheckoutInfo>) -> Result<(), &'static str> {
        self.comment_box.open(self.session.anchor().as_ref(), checkout)
    }

    pub fn cancel_comment(&mut self) {
        self.comment_box.close();
    }

    pub fn choose_recipient(&mut self, pane: PaneId) {
        self.comment_box.choose(pane);
    }

    /// The comment, anchored to the picked lines, addressed to the agent
    /// lit and named `request_id`. The box stays open until the daemon says
    /// it is saved.
    pub fn comment(&mut self, checkout: Option<&CheckoutInfo>, body: &str, request_id: u64) -> Result<ClientMsg, &'static str> {
        self.comment_box.send(self.session.anchor(), checkout, body, request_id)
    }

    /// The daemon saved the comment sent under `request_id`. When it was
    /// this box's, the box closes and the pick with it; another box's save
    /// leaves both.
    pub fn commented(&mut self, request_id: u64) {
        if self.comment_box.saved(request_id) {
            self.session.commented();
        }
    }

    /// A refusal that may be of this box's comment: the box lets go of it
    /// and keeps its words.
    pub fn comment_refused(&mut self, refusal: &RequestRefusal) -> bool {
        self.comment_box.refused(refusal)
    }

    /// A new connection: the comment sent on the last is answered on none.
    pub fn forget_sent_comment(&mut self) {
        self.comment_box.forget_sent();
    }

    /// The dialog as the page draws it, given the checkout as the tree has
    /// it now (its agents may have come or gone).
    pub fn page(&self, checkout: Option<&CheckoutInfo>) -> ReviewPage {
        let base = self.session.base();
        let view = self.view();
        let files: Vec<ReviewFile> = view.map(|v| v.review.files.iter().map(file).collect()).unwrap_or_default();
        let selected = view.and_then(|v| v.rows.get(v.sel)).map(|r| r.file());
        let (from, to) = view.map(ReviewView::selection).unwrap_or((0, 0));
        let picked = |i: usize| view.is_some_and(|v| v.mark.is_some() && i >= from && i <= to);
        let rows = match (view, selected) {
            (Some(v), Some(f)) => file_rows(v, f, Some(v.sel), picked),
            _ => Vec::new(),
        };
        let failed = self.session.failure();
        let empty = match view {
            _ if failed.is_some() => failed.map(str::to_owned),
            Some(v) if v.review.files.is_empty() => Some(match base {
                ReviewBase::Unstaged => format!("Nothing unstaged in {}.", self.name),
                ReviewBase::Staged => format!("Nothing staged in {}.", self.name),
                ReviewBase::Commit => "The commit changed nothing.".to_owned(),
            }),
            _ => None,
        };
        let base_label = match base {
            ReviewBase::Unstaged => "unstaged",
            ReviewBase::Staged => "staged",
            ReviewBase::Commit => "commit",
        };
        let pick =
            view.filter(|v| v.mark.is_some() || self.comment_box.is_open()).and_then(ReviewView::anchor).map(|a| pick_label(&a));
        let comment = self.comment_box.page(checkout, || format!("{} · {base_label}", pick.as_deref().unwrap_or("")));
        ReviewPage {
            open: true,
            checkout: self.session.checkout().0,
            name: self.name.clone(),
            branch: self.branch.clone(),
            base: base_label,
            commit: view.and_then(|v| v.review.commit.as_ref()).map(|c| format!("{}  {}", c.short, c.summary)),
            unstaged: self.session.files(ReviewBase::Unstaged),
            staged: self.session.files(ReviewBase::Staged),
            split: self.session.split(),
            added: files.iter().map(|f| f.added).sum(),
            removed: files.iter().map(|f| f.removed).sum(),
            files,
            file: selected,
            pick,
            rows,
            loading: view.is_none() && failed.is_none(),
            empty,
            comment,
            jump: self.jump.as_ref().map(|j| {
                let m = j.matches(&self.paths());
                ReviewJump { query: j.query().to_owned(), at: m.lit.map(|l| m.found[l]), matches: m.found }
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
    if let Row::Fold { file, gap } = row {
        let hidden = fold_of(&files[file], gap).map_or(0, |f| f.hidden());
        let lines = if hidden == 1 { "line" } else { "lines" };
        return DiffRow {
            row: index,
            kind: "fold",
            text: format!("{hidden} {lines} not in the diff"),
            hidden: Some(hidden),
            step: (hidden > FOLD_STEP).then_some(FOLD_STEP),
            left: None,
            right: None,
            cursor,
            picked: false,
        };
    }
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
        Row::File { .. } | Row::Fold { .. } => ("hunk", String::new(), None, None),
    };
    DiffRow { row: index, kind, text, hidden: None, step: None, left, right, cursor, picked }
}

/// A line with its highlighting cut into pieces here, where the byte
/// offsets the daemon sends mean what they say: the page counts in UTF-16.
fn side(l: &DiffLine) -> DiffSide {
    let parts = parts(&l.text, &l.spans);
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

/// A line's text cut at its highlighting, as `[text, highlight]`. A span
/// that does not land on character boundaries inside the text is dropped:
/// colour is not worth a wrong slice.
pub fn parts(text: &str, spans: &[HighlightSpan]) -> Vec<(String, &'static str)> {
    let mut parts = Vec::new();
    let mut at = 0usize;
    for span in spans {
        let (start, end) = (span.start as usize, span.end as usize);
        if start < at || end > text.len() || !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            continue;
        }
        if start > at {
            parts.push((text[at..start].to_owned(), ""));
        }
        parts.push((text[start..end].to_owned(), highlight(span.kind)));
        at = end;
    }
    if at < text.len() {
        parts.push((text[at..].to_owned(), ""));
    }
    parts
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
    use argus_protocol::{FileDiff, Hunk, PaneKind, PaneStatus};

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
                new_blob: None,
                new_lines: None,
            }],
            commit: None,
        }
    }

    fn dialog() -> (ReviewDialog, Vec<ClientMsg>) {
        remembering(ReviewLayout::default())
    }

    fn agent_checkout() -> CheckoutInfo {
        checkout(100, "main", true, vec![pane(7, PaneKind::Agent, "claude", PaneStatus::Working)])
    }

    fn remembering(layout: ReviewLayout) -> (ReviewDialog, Vec<ClientMsg>) {
        let mut ids = 0;
        ReviewDialog::working(&agent_checkout(), layout, || {
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
        // The remembered side is asked for first.
        let (mut d, _) = remembering(ReviewLayout { staged: true, split: false });
        d.receive(nothing(1, ReviewBase::Staged));
        assert_eq!(d.page(None).base, "staged", "the other side may be empty too");
        d.receive(diff(2, ReviewBase::Unstaged));
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
    fn a_side_picked_before_it_is_read_still_gives_way_when_it_is_empty() {
        let (mut d, _) = dialog();
        d.show_side(true).unwrap();
        d.receive(nothing(2, ReviewBase::Staged));
        d.receive(diff(1, ReviewBase::Unstaged));
        assert_eq!(d.page(None).base, "unstaged");
        assert!(d.chosen().staged, "the pick is still what is remembered");
    }

    #[test]
    fn the_side_and_layout_picked_are_what_is_remembered() {
        let (mut d, _) = dialog();
        d.switch_side().unwrap();
        d.toggle_split();
        assert_eq!(d.chosen(), ReviewLayout { staged: true, split: true });
    }

    #[test]
    fn a_commits_review_keeps_the_remembered_side_and_takes_the_layout() {
        let co = checkout(100, "main", true, Vec::new());
        let (mut d, _) = ReviewDialog::of_commit(&co, "aaaa1".into(), None, ReviewLayout { staged: true, split: false }, 1);
        assert!(d.switch_side().is_err(), "a commit has one side");
        d.toggle_split();
        assert_eq!(d.chosen(), ReviewLayout { staged: true, split: true });
    }

    #[test]
    fn the_jump_field_opens_the_best_match_on_enter() {
        let (mut d, _) = dialog();
        d.receive(files(1, &["docs/prereview.md", "src/review.rs", "README.md"]));
        d.open_jump();
        assert_eq!(d.page(None).jump.unwrap().matches, [0, 1, 2], "everything, before a letter");
        d.jump(ReviewJumpGesture::Query { text: "rev".into() });
        let jump = d.page(None).jump.unwrap();
        assert_eq!((jump.matches, jump.at), (vec![1, 0], Some(1)));
        d.jump(ReviewJumpGesture::Step { by: 5 });
        assert_eq!(d.page(None).jump.unwrap().at, Some(0), "held at the last match");
        d.jump(ReviewJumpGesture::Choose);
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
        d.jump(ReviewJumpGesture::Query { text: "zzz".into() });
        assert_eq!(d.page(None).jump.unwrap().at, None);
        d.jump(ReviewJumpGesture::Choose);
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
        assert_eq!(d.receive(diff(99, ReviewBase::Unstaged)), Answered::Stale, "an answer to someone else");
        d.receive(diff(1, ReviewBase::Unstaged));
        let page = d.page(None);
        assert_eq!((page.unstaged, page.staged), (Some(1), None));
        assert_eq!(page.files[0].name, "brief.rs");
        assert_eq!(page.rows[0].kind, "hunk");
    }

    #[test]
    fn a_picked_range_is_said_by_the_side_it_is_on_and_anchors_the_comment() {
        let co = agent_checkout();
        let (mut d, _) = dialog();
        d.receive(diff(1, ReviewBase::Unstaged));
        d.move_by(1);
        d.toggle_pick();
        assert_eq!(d.page(None).pick.as_deref(), Some("old line 2"), "a removal is on the old side");
        d.move_by(1);
        assert_eq!(d.page(None).pick.as_deref(), Some("new line 2"), "with an addition, the new side");
        d.compose(Some(&co)).unwrap();
        let ClientMsg::ReviewComment { recipient, anchor, body, .. } = d.comment(Some(&co), " tidy ", 1).unwrap() else {
            panic!()
        };
        assert_eq!((recipient, body.as_str()), (PaneId(7), "tidy"));
        assert_eq!(anchor.text, ["-    old()", "+    new()"]);
    }

    #[test]
    fn a_comment_box_needs_an_agent_to_open() {
        let co = checkout(100, "main", true, Vec::new());
        let (mut d, _) = ReviewDialog::working(&co, ReviewLayout::default(), || 1);
        d.receive(diff(1, ReviewBase::Unstaged));
        assert_eq!(d.compose(Some(&co)), Err("no agent running in this checkout"));
        assert!(d.page(Some(&co)).comment.is_none());
    }

    #[test]
    fn an_exited_agent_is_not_offered_and_one_chosen_is_not_sent_to() {
        let mut co = agent_checkout();
        co.panes.insert(0, pane(6, PaneKind::Agent, "gone", PaneStatus::Exited { code: Some(0) }));
        let (mut d, _) = dialog();
        d.receive(diff(1, ReviewBase::Unstaged));
        d.compose(Some(&co)).unwrap();
        assert_eq!(d.page(Some(&co)).comment.unwrap().to, Some(7), "lit on the live one");
        d.choose_recipient(PaneId(6));
        let comment = d.page(Some(&co)).comment.unwrap();
        assert_eq!(comment.recipients.iter().map(|r| r.pane).collect::<Vec<_>>(), [7]);
        assert_eq!(comment.to, None);
        assert_eq!(d.comment(Some(&co), "hi", 1).err(), Some("that agent is no longer running"));
    }

    fn with_agents(ids: &[u64]) -> CheckoutInfo {
        let panes = ids.iter().map(|&id| pane(id, PaneKind::Agent, "claude", PaneStatus::Working)).collect();
        checkout(100, "main", true, panes)
    }

    #[test]
    fn an_agent_that_exits_while_the_comment_is_written_is_refused_and_unlit() {
        let mut co = with_agents(&[7, 8]);
        let (mut d, _) = dialog();
        d.receive(diff(1, ReviewBase::Unstaged));
        d.compose(Some(&co)).unwrap();
        d.choose_recipient(PaneId(8));
        co.panes[1].status = PaneStatus::Exited { code: Some(0) };
        assert_eq!(d.comment(Some(&co), "tidy", 1).err(), Some("that agent is no longer running"));
        let comment = d.page(Some(&co)).comment.expect("the box stays open");
        assert_eq!(comment.to, None);
        d.choose_recipient(PaneId(7));
        assert!(matches!(d.comment(Some(&co), "tidy", 1), Ok(ClientMsg::ReviewComment { recipient: PaneId(7), .. })));
    }

    #[test]
    fn a_side_switch_puts_the_box_away_and_a_late_send_is_refused() {
        let co = agent_checkout();
        let (mut d, _) = dialog();
        d.receive(diff(1, ReviewBase::Unstaged));
        d.receive(diff(2, ReviewBase::Staged));
        d.compose(Some(&co)).unwrap();
        d.switch_side().unwrap();
        assert!(d.page(Some(&co)).comment.is_none());
        assert_eq!(d.comment(Some(&co), "tidy", 1).err(), Some("no comment box is open"));
    }

    #[test]
    fn only_the_save_of_its_own_comment_lets_the_picked_lines_go() {
        let co = agent_checkout();
        let (mut d, _) = dialog();
        d.receive(diff(1, ReviewBase::Unstaged));
        d.toggle_pick();
        d.compose(Some(&co)).unwrap();
        d.comment(Some(&co), "tidy", 4).unwrap();
        d.commented(5);
        let page = d.page(Some(&co));
        assert!(page.pick.is_some() && page.comment.is_some(), "another box's save");
        d.commented(4);
        let page = d.page(Some(&co));
        assert!(page.pick.is_none() && page.comment.is_none());
    }

    #[test]
    fn switching_onto_an_empty_side_keeps_the_diff_and_says_why() {
        let (mut d, _) = dialog();
        d.receive(diff(1, ReviewBase::Unstaged));
        d.receive(nothing(2, ReviewBase::Staged));
        assert_eq!(d.switch_side(), Err("nothing staged; still showing unstaged".into()));
        assert_eq!(d.page(None).base, "unstaged");
        assert!(!d.chosen().staged);
    }

    /// The diff, its file forty lines long and readable from a blob: a fold
    /// of 38 lines lies below the hunk.
    fn foldable(request_id: u64) -> Review {
        let mut review = diff(request_id, ReviewBase::Unstaged);
        review.files[0].new_blob = Some("b10b".into());
        review.files[0].new_lines = Some(40);
        review.files[0].hunks[0].lines.push(line(LineKind::Context, Some(3), Some(3), "}"));
        review
    }

    #[test]
    fn a_fold_is_drawn_with_what_it_hides_and_how_much_a_step_shows() {
        let (mut d, _) = dialog();
        d.receive(foldable(1));
        let fold = d.page(None).rows.into_iter().find(|r| r.kind == "fold").unwrap();
        assert_eq!(fold.text, "37 lines not in the diff");
        assert_eq!((fold.hidden, fold.step), (Some(37), Some(20)));
    }

    #[test]
    fn a_folds_button_asks_for_its_lines_and_they_join_the_diff() {
        let (mut d, _) = dialog();
        d.receive(foldable(1));
        let row = d.page(None).rows.iter().find(|r| r.kind == "fold").unwrap().row;
        let asked = d.unfold(Some(row), true, 9).unwrap();
        assert!(matches!(asked, ClientMsg::ReviewContext { request_id: 9, start: 4, end: 40, .. }));
        let lines = (4..=40).map(|n| line(LineKind::Context, None, Some(n), "")).collect();
        assert!(d.receive_context(ReviewContext { request_id: 9, checkout: CheckoutId(100), lines }));
        let page = d.page(None);
        assert!(page.rows.iter().all(|r| r.kind != "fold"), "the fold is gone once it is all shown");
        assert_eq!(page.rows.iter().filter(|r| r.kind == "line").count(), 41);
    }

    #[test]
    fn unfolding_off_a_fold_asks_for_nothing() {
        let (mut d, _) = dialog();
        d.receive(foldable(1));
        assert!(d.unfold(None, false, 9).is_none(), "the cursor starts on a line");
        assert!(d.unfold(Some(1), false, 9).is_none(), "a click that names a line");
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
