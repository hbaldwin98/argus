//! Git beside the work: the Git stage's view drawn in a dock, the pane left on the stage under it,
//! the way to the stage and back, and the files and the diff taking turns in a dock too narrow for
//! both.

use super::*;
use argus_desktop_core::layout::docks::{DockPage, DockTab, Panel, PanelKind, PresentPanel};

/// The docks with Git in the right one at `size` CSS pixels, shown.
fn docked(size: u32) -> Event {
    let mut docks = crate::layout::work::tests::docks();
    docks.right = DockPage { tabs: vec![DockTab { panel: Panel::Changes, title: "Git".into() }], shown: Some(Panel::Changes), size };
    docks.present.push(PresentPanel { panel: Panel::Changes, kind: PanelKind::Changes, floats: true, stage: true, closes: true, cell: false });
    Event::Docks(Box::new(docks))
}

fn asked(opened: &Opened) -> Vec<String> {
    let said = taken(opened);
    forget(opened);
    said
}

#[gpui_kit::test]
fn a_docked_git_draws_the_review_in_its_dock_and_leaves_the_pane_on_the_stage(cx: &mut TestAppContext) {
    let opened = on_main(cx);
    hear(cx, &opened, vec![reviewing(review()), docked(900)]);
    assert!(!cx.update(|cx| opened.heard.read(cx).git_up()), "the stage is the pane's");
    assert!(present(cx, &opened, "dock-right-panel"));
    assert!(present(cx, &opened, "review"), "the review is drawn, in the dock");
    assert!(!present(cx, &opened, "git-collapse"), "beside the work already");
    assert!(!present(cx, &opened, "git-tab-changes"), "the dock's head is its only head: no Git stage strip under it");
    click(cx, &opened, "dock-expand-button-right");
    assert_eq!(asked(&opened), ["Docks { gesture: Stage { panel: Changes } }"]);
}

#[gpui_kit::test]
fn a_docked_pane_pressed_hears_the_reviews_keys(cx: &mut TestAppContext) {
    let opened = on_main(cx);
    hear(cx, &opened, vec![reviewing(review()), docked(900)]);
    click(cx, &opened, ("review-file", 0usize));
    forget(&opened);
    press(cx, &opened, "j");
    assert_eq!(asked(&opened), ["ReviewAction { action: Next }"], "not typed into the pane on the stage");
}

#[gpui_kit::test]
fn git_on_the_stage_goes_back_to_the_side_pane(cx: &mut TestAppContext) {
    let opened = reviewed(cx);
    click(cx, &opened, "git-collapse");
    assert_eq!(asked(&opened), ["Docks { gesture: Collapse { panel: Changes } }"]);
}

#[gpui_kit::test]
fn going_elsewhere_keeps_a_docked_git_and_git_pressed_shows_it(cx: &mut TestAppContext) {
    let opened = on_main(cx);
    hear(cx, &opened, vec![reviewing(review()), docked(900)]);
    click(cx, &opened, "nav-git");
    assert_eq!(asked(&opened), ["Docks { gesture: Show { panel: Changes } }"], "its dock opens on it, and nothing closes");
}

#[gpui_kit::test]
fn in_a_narrow_dock_the_files_and_the_diff_take_turns(cx: &mut TestAppContext) {
    let opened = on_main(cx);
    hear(cx, &opened, vec![reviewing(review()), docked(480)]);
    // The first frame is laid out wide; the one after knows the tab's width.
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    assert!(!present(cx, &opened, "review-column"), "no files beside the diff");
    assert!(present(cx, &opened, ("review-file", 0usize)), "the files first");
    assert!(!present(cx, &opened, "review-back"), "and no diff");
    click(cx, &opened, ("review-file", 0usize));
    assert_eq!(asked(&opened), ["ReviewClick { target: File { file: 0 } }"], "the file the review opened on, pressed");
    assert!(present(cx, &opened, "review-back"), "its diff, with the way back");
    assert!(!present(cx, &opened, ("review-file", 0usize)));
    click(cx, &opened, "review-back");
    assert!(present(cx, &opened, ("review-file", 0usize)), "the files again");
    assert!(asked(&opened).is_empty(), "the turn is the window's own");
}
