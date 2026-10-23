//! The Changes pane in the whole window: both sides' lists and what a press on a file asks, the
//! boxes and the hunks' buttons, the commit box, and the review's drafts and its send. Nothing here
//! changes on a press: each sends its call, and what is staged moves when the session says so.

use super::*;
use argus_desktop_core::review::drafts::{DraftView, DraftsPage};
use argus_desktop_core::review::staging::{ReviewSection, SectionFile, StagingPage};

fn listed(path: &str, kind: &'static str, on: bool, partly: bool) -> SectionFile {
    SectionFile { file: ReviewFile { kind, ..file(path, 2, 1) }, partly, on }
}

fn staging(staged: bool, commits: u64) -> StagingPage {
    StagingPage { staged, busy: false, committing: false, commits, can_commit: true }
}

/// The review of working changes as the Changes pane hears it: `src/lib.rs` partly staged and on
/// screen from the working side, `README.md` staged, `new.rs` untracked.
pub(super) fn changes() -> ReviewPage {
    ReviewPage {
        sections: vec![
            ReviewSection { side: "staged", files: vec![listed("README.md", "modified", false, false), listed("src/lib.rs", "modified", false, true)] },
            ReviewSection { side: "unstaged", files: vec![listed("src/lib.rs", "modified", true, true)] },
            ReviewSection { side: "untracked", files: vec![listed("new.rs", "untracked", false, false)] },
        ],
        staging: Some(staging(false, 0)),
        ..review()
    }
}

fn drafting(drafts: Vec<DraftView>) -> ReviewPage {
    let recipients = vec![Recipient { pane: 7, title: "claude".into() }];
    ReviewPage { drafts: Some(DraftsPage { drafts, recipients, to: Some(7), sending: false }), ..changes() }
}

fn draft(id: u64, row: Option<usize>) -> DraftView {
    DraftView { id, path: "src/lib.rs".into(), at: "src/lib.rs:3".into(), body: "why two?".into(), staged: false, row }
}

fn asked(opened: &Opened) -> Vec<String> {
    let said = taken(opened);
    forget(opened);
    said
}

#[gpui_kit::test]
fn a_working_review_lists_both_sides_and_the_commit_box_and_says_what_is_in_both(cx: &mut TestAppContext) {
    let opened = on_main(cx);
    hear(cx, &opened, vec![reviewing(changes())]);
    assert!(present(cx, &opened, ("review-section", 0usize)), "staged");
    assert!(present(cx, &opened, ("review-section", 1usize)), "changed");
    assert!(present(cx, &opened, ("review-section", 2usize)), "untracked, since it holds a file");
    assert!(!present(cx, &opened, "review-side-staged"), "both sides are listed, so there is no side to pick");
    assert_eq!(selected(cx, &opened, ("review-change", 10_000usize)), Some(true), "the file on screen, from its own list");
    assert_eq!(selected(cx, &opened, ("review-change", 1usize)), Some(false), "the same file in the other list is not lit");
    assert!(present(cx, &opened, ("review-change-partly", 1usize)), "partly staged");
    assert!(present(cx, &opened, "review-commit-box"));
    assert_eq!(label(cx, &opened, "review-commit").as_deref(), Some("Commit 2 staged files"));
}

#[gpui_kit::test]
fn a_click_opens_a_file_on_its_side_and_the_lists_buttons_move_files_across(cx: &mut TestAppContext) {
    let opened = on_main(cx);
    hear(cx, &opened, vec![reviewing(changes())]);
    click(cx, &opened, ("review-change", 0usize));
    assert_eq!(asked(&opened), [r#"OpenChange { staged: true, path: "README.md" }"#]);
    click(cx, &opened, ("review-change-move", 10_000usize));
    assert_eq!(asked(&opened), [r#"Stage { gesture: File { staged: false, path: "src/lib.rs" } }"#]);
    click(cx, &opened, ("review-section-all", 0usize));
    assert_eq!(asked(&opened), [r#"Stage { gesture: Files { staged: false, paths: ["README.md", "src/lib.rs"] } }"#], "unstage all");
    click(cx, &opened, ("review-section-all", 1usize));
    assert_eq!(asked(&opened), [r#"Stage { gesture: Files { staged: true, paths: ["src/lib.rs"] } }"#], "stage all");
    assert_eq!(selected(cx, &opened, ("review-change", 0usize)), Some(false), "nothing moves before the session says so");
}

#[gpui_kit::test]
fn a_box_sends_its_row_a_shift_tick_the_changed_rows_since_and_a_hunk_its_header(cx: &mut TestAppContext) {
    let opened = on_main(cx);
    hear(cx, &opened, vec![reviewing(changes())]);
    assert!(!present(cx, &opened, ("diff-tick", 2usize)), "a context line has no box");
    assert_eq!(label(cx, &opened, ("diff-tick", 4usize)).as_deref(), Some("Stage the line"));
    click(cx, &opened, ("diff-tick", 4usize));
    assert_eq!(asked(&opened), ["Stage { gesture: Rows { rows: [2] } }"]);
    shift_click(cx, &opened, ("diff-tick", 6usize));
    assert_eq!(asked(&opened), ["Stage { gesture: Rows { rows: [2, 3] } }"]);
    click(cx, &opened, ("diff-hunk-stage", 0usize));
    assert_eq!(asked(&opened), ["Stage { gesture: Hunk { row: 0 } }"]);
    assert_eq!(label(cx, &opened, ("diff-hunk-stage", 0usize)).as_deref(), Some("Stage hunk"));
    hear(cx, &opened, vec![reviewing(ReviewPage { staging: Some(staging(true, 0)), ..changes() })]);
    assert_eq!(label(cx, &opened, ("diff-tick", 4usize)).as_deref(), Some("Unstage the line"), "the index's diff is ticked");
    assert_eq!(label(cx, &opened, ("diff-hunk-stage", 0usize)).as_deref(), Some("Unstage hunk"));
}

#[gpui_kit::test]
fn a_review_that_cannot_stage_draws_no_boxes_and_no_commit_box(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    assert!(!present(cx, &opened, ("diff-tick", 4usize)));
    assert!(!present(cx, &opened, ("diff-hunk-stage", 0usize)));
    assert!(!present(cx, &opened, "review-commit-box"));
}

#[gpui_kit::test]
fn the_commit_button_sends_the_message_and_the_box_empties_only_once_a_commit_is_counted(cx: &mut TestAppContext) {
    let opened = on_main(cx);
    hear(cx, &opened, vec![reviewing(changes())]);
    click(cx, &opened, "review-commit-box");
    type_text(cx, &opened, "Say why");
    forget(&opened);
    click(cx, &opened, "review-commit");
    assert_eq!(asked(&opened), [r#"Commit { message: "Say why" }"#]);
    hear(cx, &opened, vec![reviewing(changes())]);
    click(cx, &opened, "review-commit");
    assert_eq!(asked(&opened), [r#"Commit { message: "Say why" }"#], "a refused commit keeps its words");
    hear(cx, &opened, vec![reviewing(ReviewPage { staging: Some(staging(false, 1)), ..changes() })]);
    click(cx, &opened, "review-commit");
    assert_eq!(asked(&opened), [r#"Commit { message: "" }"#], "the message went with the commit");
}

#[gpui_kit::test]
fn a_draft_hangs_under_its_line_and_its_delete_and_the_review_send_go_to_the_session(cx: &mut TestAppContext) {
    let opened = on_main(cx);
    hear(cx, &opened, vec![reviewing(drafting(vec![draft(5, Some(3)), draft(6, None)]))]);
    assert!(present(cx, &opened, ("draft-card", 5usize)), "on the file on screen");
    assert!(!present(cx, &opened, ("draft-card", 6usize)), "written elsewhere, so only in the review");
    click(cx, &opened, ("draft-delete", 5usize));
    assert_eq!(asked(&opened), ["Drafts { gesture: Delete { id: 5 } }"]);
    assert!(!present(cx, &opened, "review-submit"), "shut until the Review button opens it");
    assert_eq!(label(cx, &opened, "review-open-submit").as_deref(), Some("Review, 2 pending"));
    click(cx, &opened, "review-open-submit");
    assert!(present(cx, &opened, "review-submit"));
    assert!(present(cx, &opened, ("review-draft", 6usize)));
    click(cx, &opened, ("review-draft", 6usize));
    assert_eq!(asked(&opened), [r#"OpenChange { staged: false, path: "src/lib.rs" }"#]);
    click(cx, &opened, "review-send");
    assert_eq!(asked(&opened), [r#"Drafts { gesture: Submit { summary: "", to: Some(7) } }"#]);
    click(cx, &opened, "review-discard");
    assert_eq!(asked(&opened), ["Drafts { gesture: Discard }"]);
}

#[gpui_kit::test]
fn a_sent_review_that_comes_back_empty_shuts_the_panel(cx: &mut TestAppContext) {
    let opened = on_main(cx);
    hear(cx, &opened, vec![reviewing(drafting(vec![draft(5, Some(3))]))]);
    click(cx, &opened, "review-open-submit");
    let sending = drafting(vec![draft(5, Some(3))]);
    let sending = ReviewPage { drafts: sending.drafts.map(|d| DraftsPage { sending: true, ..d }), ..sending };
    hear(cx, &opened, vec![reviewing(sending)]);
    assert!(present(cx, &opened, "review-submit"));
    hear(cx, &opened, vec![reviewing(drafting(Vec::new()))]);
    assert!(!present(cx, &opened, "review-submit"));
}

#[gpui_kit::test]
fn the_comment_box_of_a_review_being_drafted_adds_to_it(cx: &mut TestAppContext) {
    let opened = on_main(cx);
    let comment = CommentPage { about: "new line 3 · unstaged".into(), recipients: vec![Recipient { pane: 7, title: "claude".into() }], to: Some(7) };
    hear(cx, &opened, vec![reviewing(ReviewPage { comment: Some(comment), ..drafting(Vec::new()) })]);
    assert_eq!(label(cx, &opened, "comment-send").as_deref(), Some("Add to review"));
    type_text(cx, &opened, "why two?");
    forget(&opened);
    click(cx, &opened, "comment-send");
    assert_eq!(asked(&opened), [r#"DraftComment { body: "why two?" }"#]);
}

/// The prototype's ruler down the diff's edge: a mark for each run of changed rows, of both kinds
/// when it holds both, and a press on one scrolls the diff, which is the window's own.
#[gpui_kit::test]
fn the_ruler_marks_each_run_of_changed_rows_and_a_press_only_scrolls(cx: &mut TestAppContext) {
    let opened = on_main(cx);
    hear(cx, &opened, vec![reviewing(changes())]);
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    assert!(present(cx, &opened, "review-ruler"));
    assert_eq!(label(cx, &opened, ("review-mark", 0usize)).as_deref(), Some("Lines changed"), "a line removed and one added");
    assert!(!present(cx, &opened, ("review-mark", 1usize)), "one run");
    forget(&opened);
    click(cx, &opened, ("review-mark", 0usize));
    assert!(asked(&opened).is_empty());
}
