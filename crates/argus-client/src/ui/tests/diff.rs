//! The review viewer, stacked and side by side.

use super::*;

#[test]
fn the_review_never_hides_the_nav_columns() {
    // The standing rule for every view Argus has: one thing opening
    // must not take the others off screen.
    let mut app = app_with_review();
    let out = lines(&draw(&mut app)).join("\n");
    assert!(out.contains("argus"), "the project is still listed:\n{out}");
    assert!(
        out.contains("src/thing.rs"),
        "and the diff is up too:\n{out}"
    );
}

#[test]
fn the_history_never_hides_the_nav_columns() {
    // Same standing rule as the review: opening the log must not take
    // the tree off screen.
    let mut app = app_with_history();
    let out = lines(&draw(&mut app)).join(
        "
",
    );
    assert!(
        out.contains("argus"),
        "the project is still listed:
{out}"
    );
}

#[test]
fn a_commit_row_shows_its_short_hash_summary_and_author() {
    let mut app = app_with_history();
    let out = lines(&draw(&mut app)).join(
        "
",
    );
    assert!(
        out.contains("aaaaaaa"),
        "the short hash:
{out}"
    );
    assert!(
        out.contains("Wake a pane's pump"),
        "the subject:
{out}"
    );
    assert!(
        out.contains("hunt"),
        "the author:
{out}"
    );
}

#[test]
fn the_files_a_commit_touched_are_listed_once_it_is_drilled_into() {
    let mut app = app_with_drilled_history();
    let out = lines(&draw(&mut app)).join(
        "
",
    );
    assert!(out.contains("crates/argusd/src/pty.rs"), "{out}");
    assert!(out.contains("DESIGN.md"), "{out}");
}

/// The reason the overlay opens fast at all: a hundred commits cost a
/// hundred diffs to summarize, so a fresh list shows none of them.
#[test]
fn a_fresh_history_lists_no_files_and_marks_the_commit_as_foldable() {
    let mut app = app_with_history();
    let out = lines(&draw(&mut app)).join(
        "
",
    );
    assert!(!out.contains("crates/argusd/src/pty.rs"), "{out}");
    assert!(
        out.contains("▸"),
        "the header says there is something to open:
{out}"
    );
}

#[test]
fn a_file_header_shows_its_marker_path_and_line_counts() {
    let mut app = app_with_review();
    let out = lines(&draw(&mut app)).join("\n");
    assert!(out.contains("src/thing.rs"), "{out}");
    assert!(out.contains("+1"), "one line added:\n{out}");
    assert!(out.contains("-1"), "one line removed:\n{out}");
}

#[test]
fn diff_lines_keep_gits_markers_and_line_numbers() {
    let mut app = app_with_review();
    let out = lines(&draw(&mut app)).join("\n");
    assert!(out.contains("+arrived"), "{out}");
    assert!(out.contains("-gone"), "{out}");
    assert!(
        out.contains("@@ -10,3 +10,3 @@"),
        "the hunk header too:\n{out}"
    );
}

#[test]
fn added_and_removed_lines_are_told_apart_by_color() {
    // The markers alone are one character wide; color is what makes a
    // diff scannable. The marker keeps its own colour because a
    // terminal that drops backgrounds leaves nothing else.
    let mut app = app_with_review();
    let buf = draw(&mut app);
    let th = app.theme;
    assert_eq!(fg_of(&buf, "+arrived"), Some(th.ok));
    assert_eq!(fg_of(&buf, "-gone"), Some(th.err));
}

/// The foreground now belongs to syntax, so which side a line is on is
/// carried by a background wash that does not depend on the selection.
#[test]
fn added_and_removed_lines_are_washed_whether_or_not_they_are_selected() {
    let mut app = app_with_review();
    let th = app.theme;
    let buf = draw(&mut app);
    assert_eq!(bg_of(&buf, "+arrived"), Some(th.add_bg));
    assert_eq!(bg_of(&buf, "-gone"), Some(th.del_bg));
    assert_ne!(
        bg_of(&buf, "unchanged"),
        Some(th.add_bg),
        "context is not washed"
    );
}

#[test]
fn the_selected_line_is_washed_so_a_range_reads_as_one_block() {
    let mut app = app_with_review();
    let th = app.theme;
    app.review.as_mut().unwrap().toggle_mark();
    app.review.as_mut().unwrap().move_by(1);
    let buf = draw(&mut app);
    assert_eq!(bg_of(&buf, "unchanged"), Some(th.sel_bg));
    // Selecting a removed line brightens its wash rather than replacing
    // it: a selected range must still show which side each line was on.
    assert_eq!(bg_of(&buf, "-gone"), Some(th.del_bg_sel), "the whole range");
    assert_eq!(bg_of(&buf, "+arrived"), Some(th.add_bg), "but no further");
}

/// End to end from the wire: a span the daemon sent reaches the screen as
/// its theme colour, and the text around it stays plain.
#[test]
fn a_highlight_span_colours_only_the_run_it_covers() {
    let mut app = app_with_review();
    {
        let view = app.review.as_mut().unwrap();
        let line = &mut view.review.files[0].hunks[0].lines[0];
        line.text = "let x = 1".to_string();
        line.spans = vec![argus_protocol::HighlightSpan {
            start: 0,
            end: 3,
            kind: argus_protocol::HighlightKind::Keyword,
        }];
    }
    let th = app.theme;
    let buf = draw(&mut app);
    assert_eq!(fg_of(&buf, "let"), Some(th.syntax.keyword));
    assert_eq!(
        fg_of(&buf, "x = 1"),
        Some(th.text),
        "uncovered text stays plain"
    );
}

/// Offsets cross a process boundary, so the renderer treats them as
/// untrusted. A span past the end of the line must not panic or colour
/// anything, and the line must still draw.
#[test]
fn a_span_that_overruns_its_line_is_dropped_rather_than_sliced() {
    let mut app = app_with_review();
    {
        let view = app.review.as_mut().unwrap();
        let line = &mut view.review.files[0].hunks[0].lines[0];
        line.text = "short".to_string();
        line.spans = vec![argus_protocol::HighlightSpan {
            start: 2,
            end: 99,
            kind: argus_protocol::HighlightKind::Keyword,
        }];
    }
    let th = app.theme;
    let buf = draw(&mut app);
    assert_eq!(fg_of(&buf, "short"), Some(th.text));
}

#[test]
fn a_diff_taller_than_the_column_scrolls_to_keep_the_cursor_visible() {
    let mut app = app_with_review();
    app.review.as_mut().unwrap().bottom_of_diff();
    let out = lines(&draw_at(&mut app, 100, 10)).join("\n");
    assert!(
        out.contains("+arrived"),
        "the cursor's line is on screen:\n{out}"
    );
}

// --- the split view -----------------------------------------------------

#[test]
fn a_split_row_draws_both_sides_of_one_change_on_the_same_line() {
    let mut app = app_with_review_split(true);
    let out = lines(&draw(&mut app));
    assert!(
        out.iter()
            .any(|l| l.contains("-gone") && l.contains("+arrived")),
        "the removal and its replacement share a row:
{}",
        out.join(
            "
"
        )
    );
}

#[test]
fn the_unified_view_still_stacks_them() {
    // The whole point of the toggle is that this is the other shape.
    let mut app = app_with_review();
    let out = lines(&draw(&mut app));
    assert!(
        !out.iter()
            .any(|l| l.contains("-gone") && l.contains("+arrived")),
        "{}",
        out.join(
            "
"
        )
    );
}

#[test]
fn each_side_of_a_split_row_keeps_its_own_wash() {
    // One row, two sides: a single line-wide background would say the
    // whole row was added, or removed, and it is both.
    let mut app = app_with_review_split(true);
    let th = app.theme;
    let buf = draw(&mut app);
    assert_eq!(bg_of(&buf, "-gone"), Some(th.del_bg));
    assert_eq!(bg_of(&buf, "+arrived"), Some(th.add_bg));
}

#[test]
fn a_split_row_numbers_each_side_from_its_own_file() {
    // The reason a split view can show both numbers at all: the left is
    // read from the old file, the right from the new one.
    let mut app = app_with_diff(
        true,
        vec![
            diff_line(argus_protocol::LineKind::Removed, Some(11), None, "gone"),
            diff_line(argus_protocol::LineKind::Added, None, Some(207), "arrived"),
        ],
    );
    let buf = draw(&mut app);
    let y = row_of(&buf, "-gone").expect("the removed line is drawn");
    // Inside the overlay only: the panel borders down the screen are
    // the same glyph as the divider.
    let row = row_text(&buf, y, app.layout.overlay.inner);
    let (left, right) = row.split_once('│').expect("a divider between the sides");
    assert!(left.contains("11"), "the old number on the left: {left:?}");
    assert!(
        right.contains("207"),
        "the new number on the right: {right:?}"
    );
}

#[test]
fn a_side_with_nothing_opposite_it_is_recessed_rather_than_blank() {
    // An added line with no removal against it leaves half the row
    // empty; dropping it a step in elevation says "nothing here"
    // instead of "an empty line of code".
    let mut app = app_with_diff(
        true,
        vec![diff_line(
            argus_protocol::LineKind::Added,
            None,
            Some(1),
            "solo",
        )],
    );
    let th = app.theme;
    let buf = draw(&mut app);
    let y = row_of(&buf, "+solo").expect("the added line is drawn");
    assert_eq!(
        buf.cell((app.layout.overlay.inner.x, y)).map(|c| c.bg),
        Some(th.surface),
        "the empty left side"
    );
}

#[test]
fn a_long_line_is_cut_at_its_own_half_rather_than_over_the_divider() {
    // One side overrunning would push the other off the row, and the
    // columns would stop lining up down the screen.
    let mut app = app_with_diff(
        true,
        vec![
            diff_line(
                argus_protocol::LineKind::Removed,
                Some(1),
                None,
                &"x".repeat(400),
            ),
            diff_line(argus_protocol::LineKind::Added, None, Some(1), "short"),
        ],
    );
    let out = lines(&draw(&mut app));
    assert!(
        out.iter().any(|l| l.contains("+short")),
        "the far side survives the long one:
{}",
        out.join(
            "
"
        )
    );
}

#[test]
fn the_status_bar_offers_the_reviews_own_keys_while_it_is_up() {
    let mut app = app_with_review();
    let out = lines(&draw(&mut app)).join("\n");
    assert!(out.contains("c comment"), "{out}");
    assert!(out.contains("b staged/unstaged"), "{out}");
    assert!(!out.contains("s shell"), "not the tree keymap:\n{out}");
}

#[test]
fn the_tree_keymap_advertises_review() {
    let mut app = app_with_tree();
    app.focus = Focus::Checkouts;
    let out = lines(&draw(&mut app)).join("\n");
    assert!(out.contains("R review"), "{out}");
}
