//! Which child of the open list the cursor is on, for the scroll that follows it.

use argus_desktop_core::code::stage::{CodeHit, CodeHitFile, CodeJump, CodePage, CodeSearch, CodeTreeRow};

use super::*;

fn row(name: &str, on: bool, ignored: bool) -> CodeTreeRow {
    CodeTreeRow { path: name.into(), name: name.into(), depth: 0, folder: false, open: false, changed: false, added: 0, removed: 0, on, ignored }
}

/// The page's `.code-list .on` scrolled into view: the tree's lit row is its place among the rows that stay.
#[test]
fn the_tree_scrolls_to_the_lit_row_by_its_place_among_the_rows_shown() {
    let page = CodePage { tree: vec![row("a", false, false), row("target", false, true), row("b", true, false)], ..CodePage::default() };
    assert_eq!(lit_child(&page, false), Some(Lit::Tree(1)), "the ignored row is folded away, so b is the second");
    assert_eq!(lit_child(&CodePage { tree: vec![row("a", false, false)], ..CodePage::default() }, false), None, "no row is lit");
}

/// A lit row inside the closed fold is nowhere to scroll to; opened, it is in the child after the fold.
#[test]
fn a_lit_row_in_the_fold_is_scrolled_to_only_once_the_fold_is_open() {
    let page = CodePage { tree: vec![row("a", false, false), row("target", true, true)], ..CodePage::default() };
    assert_eq!(lit_child(&page, false), None);
    assert_eq!(lit_child(&page, true), Some(Lit::Tree(2)), "the one row that stays, the fold row, then the held rows' group");
}

#[test]
fn the_go_to_list_scrolls_to_the_match_enter_would_read() {
    let jump = |at| CodePage { jump: Some(CodeJump { query: String::new(), matches: vec!["a".into(), "b".into()], at }), ..CodePage::default() };
    assert_eq!(lit_child(&jump(Some(1)), false), Some(Lit::Jump(1)));
    assert_eq!(lit_child(&jump(None), false), None);
}

/// The hits are a header and its lines per open file, a folded file's lines not being children.
#[test]
fn the_search_scrolls_to_the_lit_hit_counted_across_headers_and_open_hits() {
    let hit = |index, on| CodeHit { index, line: 1, parts: Vec::new(), on };
    let file = |open, hits| CodeHitFile { path: String::new(), dir: String::new(), name: String::new(), count: 0, open, hits };
    let found = |current| CodePage {
        search: Some(CodeSearch { current, files: vec![file(true, vec![hit(0, false), hit(1, false)]), file(false, vec![hit(2, false)]), file(true, vec![hit(3, true)])], ..CodeSearch::default() }),
        ..CodePage::default()
    };
    // header, two hits, header (folded), header, the lit hit.
    assert_eq!(lit_child(&found(true), false), Some(Lit::Hit(5)));
    assert_eq!(lit_child(&found(false), false), None, "stale results are not drawn");
}
