use argus_protocol::{DiffLine, FileDiff, Hunk, LineKind, PickedLine, Review};

use super::*;

fn line(kind: LineKind, old: Option<u32>, new: Option<u32>, text: &str) -> DiffLine {
    DiffLine { kind, old_lineno: old, new_lineno: new, text: text.into(), spans: Vec::new() }
}

fn file(path: &str, kind: ChangeKind, hunks: Vec<Hunk>) -> FileDiff {
    FileDiff { path: path.into(), old_path: None, kind, hunks, note: None, new_blob: None, new_lines: None, pending: false }
}

fn hunk(lines: Vec<DiffLine>) -> Hunk {
    Hunk { header: "@@ -1 +1 @@".into(), lines }
}

/// `a.rs` with two hunks and a new `n.rs` unstaged, `a.rs` and a renamed
/// `r.rs` staged, read in that order.
fn session() -> ReviewSession {
    let mut ids = 0;
    let (mut s, _) = ReviewSession::working(CheckoutId(1), ReviewBase::Unstaged, false, false, || {
        ids += 1;
        ids
    });
    let unstaged = vec![
        file(
            "a.rs",
            ChangeKind::Modified,
            vec![
                hunk(vec![line(LineKind::Context, Some(1), Some(1), "a"), line(LineKind::Removed, Some(2), None, "b"), line(LineKind::Added, None, Some(2), "B")]),
                hunk(vec![line(LineKind::Added, None, Some(20), "late")]),
            ],
        ),
        file("n.rs", ChangeKind::Untracked, vec![hunk(vec![line(LineKind::Added, None, Some(1), "new")])]),
    ];
    let mut renamed = file("r.rs", ChangeKind::Renamed, Vec::new());
    renamed.old_path = Some("old.rs".into());
    let staged = vec![file("a.rs", ChangeKind::Modified, vec![hunk(vec![line(LineKind::Added, None, Some(9), "staged")])]), renamed];
    s.receive(Review { request_id: 1, checkout: CheckoutId(1), base: ReviewBase::Unstaged, files: unstaged, commit: None });
    s.receive(Review { request_id: 2, checkout: CheckoutId(1), base: ReviewBase::Staged, files: staged, commit: None });
    s
}

fn change(msg: Result<ClientMsg, String>) -> IndexChange {
    match msg {
        Ok(ClientMsg::Stage { change, .. }) => change,
        other => panic!("not a stage: {other:?}"),
    }
}

fn row_of(s: &ReviewSession, want: impl Fn(&Row) -> bool) -> usize {
    s.view().unwrap().rows.iter().position(want).unwrap()
}

#[test]
fn a_ticked_line_of_the_working_side_is_staged_by_its_number_and_text() {
    let s = session();
    let removed = row_of(&s, |r| matches!(r, Row::Line { file: 0, hunk: 0, line: 1 }));
    let staged = change(Staging::default().stage(StageGesture::Rows { rows: vec![removed] }, &s, 7));
    assert_eq!(
        staged,
        IndexChange::Lines {
            from: ReviewBase::Unstaged,
            path: "a.rs".into(),
            lines: vec![PickedLine { kind: LineKind::Removed, lineno: 2, text: "b".into() }],
        }
    );
}

#[test]
fn a_hunk_takes_its_own_changed_lines_and_no_other_hunks() {
    let s = session();
    let header = row_of(&s, |r| matches!(r, Row::Hunk { file: 0, hunk: 0 }));
    let IndexChange::Lines { lines, .. } = change(Staging::default().stage(StageGesture::Hunk { row: header }, &s, 7)) else {
        panic!("not lines");
    };
    assert_eq!(lines.iter().map(|l| l.lineno).collect::<Vec<_>>(), [2, 2], "line 20 is the next hunk's");
    let line = row_of(&s, |r| matches!(r, Row::Line { .. }));
    assert!(Staging::default().stage(StageGesture::Hunk { row: line }, &s, 7).is_err());
}

#[test]
fn a_file_moves_to_the_list_it_is_not_in() {
    let s = session();
    let into = change(Staging::default().stage(StageGesture::File { staged: false, path: "n.rs".into() }, &s, 7));
    assert_eq!(into, IndexChange::Stage { paths: vec!["n.rs".into()] });
    let out = change(Staging::default().stage(StageGesture::File { staged: true, path: "a.rs".into() }, &s, 7));
    assert_eq!(out, IndexChange::Unstage { paths: vec!["a.rs".into()] });
}

#[test]
fn a_renamed_file_goes_back_with_its_old_name() {
    let s = session();
    let out = change(Staging::default().stage(StageGesture::Files { staged: false, paths: vec!["r.rs".into()] }, &s, 7));
    assert_eq!(out, IndexChange::Unstage { paths: vec!["r.rs".into(), "old.rs".into()] });
    assert!(Staging::default().stage(StageGesture::Files { staged: true, paths: Vec::new() }, &s, 7).is_err());
}

#[test]
fn a_write_is_out_until_the_daemon_answers_it_either_way() {
    let s = session();
    let mut staging = Staging::default();
    staging.stage(StageGesture::File { staged: false, path: "n.rs".into() }, &s, 7).unwrap();
    staging.stage(StageGesture::File { staged: false, path: "a.rs".into() }, &s, 8).unwrap();
    assert!(staging.page(&s).busy);
    assert_eq!(staging.written(7), Some(Landed::Staged));
    assert!(staging.page(&s).busy, "8 is still out");
    assert!(staging.refused(8));
    assert!(!staging.page(&s).busy);
    assert_eq!(staging.written(99), None, "not one of these");
}

#[test]
fn a_commit_needs_a_message_something_staged_and_the_last_one_done() {
    let s = session();
    let mut staging = Staging::default();
    assert!(staging.commit(CheckoutId(1), "  ", Some(2), 1).is_err());
    assert!(staging.commit(CheckoutId(1), "Say", Some(0), 1).is_err());
    let Ok(ClientMsg::Commit { message, .. }) = staging.commit(CheckoutId(1), "Say why\n\nbody\n", Some(2), 5) else {
        panic!("not a commit");
    };
    assert_eq!(message, "Say why\n\nbody");
    assert!(staging.commit(CheckoutId(1), "again", Some(2), 6).is_err(), "one at a time");
    assert!(staging.page(&s).committing);
    assert_eq!(staging.written(5), Some(Landed::Committed));
    assert_eq!(staging.page(&s).commits, 1);
    assert!(staging.commit(CheckoutId(1), "next", Some(2), 7).is_ok());
    assert!(staging.refused(7));
    assert_eq!(staging.page(&s).commits, 1, "a refused commit is not counted");
}

#[test]
fn the_lists_hold_each_side_and_say_which_files_are_in_both() {
    let s = session();
    let lists = sections(&s, Some("a.rs"));
    let names = |i: usize| lists[i].files.iter().map(|f| (f.file.path.as_str(), f.partly, f.on)).collect::<Vec<_>>();
    assert_eq!(lists.iter().map(|l| l.side).collect::<Vec<_>>(), ["staged", "unstaged", "untracked"]);
    assert_eq!(names(0), [("a.rs", true, false), ("r.rs", false, false)], "on screen is the unstaged a.rs");
    assert_eq!(names(1), [("a.rs", true, true)]);
    assert_eq!(names(2), [("n.rs", false, false)]);
}
