//! The diff block: an agent's unified diff drawn by the review's diff view, bounded in height
//! (PATCH-1 to PATCH-3).

use gpui_kit::TestAppContext;
use serde_json::{json, Value};

use super::*;

const SMALL: &str = "--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1,3 +1,3 @@\n keep\n-old\n+new\n tail\n";

fn diff(path: Option<&str>, patch: &str) -> Value {
    json!([{"kind": "diff", "path": path, "patch": patch}])
}

fn height(cx: &mut TestAppContext, rig: &Rig, id: &str) -> f32 {
    f32::from(bounds(cx, rig, id).expect("drawn").size.height)
}

// PATCH-3
#[gpui_kit::test]
fn a_diff_says_its_file_and_counts_what_was_added_and_removed(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, diff(None, SMALL));
    assert_eq!(label(cx, &rig, "7/v.0-diff-body").as_deref(), Some("Diff of src/a.rs"), "the +++ name stands in for a path the agent did not give");
    assert_eq!(label(cx, &rig, "7/v.0-diff-added").as_deref(), Some("+1"));
    assert_eq!(label(cx, &rig, "7/v.0-diff-removed").as_deref(), Some("\u{2212}1"));
}

#[gpui_kit::test]
fn the_agents_path_wins_over_the_name_in_the_diff_and_a_diff_with_neither_is_just_a_diff(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, diff(Some("lib/b.rs"), SMALL));
    assert_eq!(label(cx, &rig, "7/v.0-diff-body").as_deref(), Some("Diff of lib/b.rs"));
    let bare = open(cx, 700.0, diff(None, "@@ -1 +1 @@\n-a\n+b\n"));
    assert_eq!(label(cx, &bare, "7/v.0-diff-body").as_deref(), Some("Diff"));
}

// PATCH-1
#[gpui_kit::test]
fn text_with_no_hunk_is_drawn_as_a_diff_code_block(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, diff(Some("x.rs"), "not a diff at all"));
    assert!(exists(cx, &rig, "7/v.0-code"), "what the agent wrote is still shown, as code");
    assert!(!exists(cx, &rig, "7/v.0-diff-body"));
}

// The diff view owns its scroll, so the block gives it a height: its rows', to the most `--diff-max-h` allows.
#[gpui_kit::test]
fn a_short_diff_is_as_tall_as_its_rows_and_a_long_one_stops_at_the_limit(cx: &mut TestAppContext) {
    let short = open(cx, 700.0, diff(None, SMALL));
    let few = height(cx, &short, "7/v.0-diff-body");
    assert!(few > 0.0 && few < 200.0, "{few}");
    let lines: String = (0..200).map(|n| format!("+line {n}\n")).collect();
    let long = open(cx, 700.0, diff(None, &format!("@@ -0,0 +1,200 @@\n{lines}")));
    let limit = crate::theme::themes::ledger::tokens().diff_max_h;
    assert!((height(cx, &long, "7/v.0-diff-body") - limit).abs() < 0.5, "it scrolls inside the limit instead of running on");
}

// PATCH-1: the rows drawn are only what is on screen, however long the diff.
#[gpui_kit::test]
fn a_long_diff_builds_only_the_rows_on_screen(cx: &mut TestAppContext) {
    let lines: String = (0..2000).map(|n| format!("+line {n}\n")).collect();
    let rig = open(cx, 700.0, diff(None, &format!("@@ -0,0 +1,2000 @@\n{lines}")));
    settle(cx, &rig);
    let built = crate::kit::diff::built_by("7/v.0-diff-view");
    assert!(built > 0 && built < 600, "built {built} of 2001 rows");
}
