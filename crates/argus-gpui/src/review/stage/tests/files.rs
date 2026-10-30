//! The Changes pane's list column has no Files tab: the title bar's tabs say which side of Git is
//! up, and the checkout's files are Code's (`project/Git.dc.html`).

use super::changes::changes;
use super::*;

#[gpui_kit::test]
fn the_list_column_offers_no_files_tab(cx: &mut TestAppContext) {
    let opened = on_main(cx);
    hear(cx, &opened, vec![reviewing(changes())]);
    assert!(!present(cx, &opened, "review-list-files"));
    assert!(!present(cx, &opened, "review-list-changes"));
}
