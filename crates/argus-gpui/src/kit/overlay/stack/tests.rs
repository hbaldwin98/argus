use super::*;
use Overlay::*;

fn up(kind: Overlay) -> Up {
    Up { kind, dismissable: true }
}

#[test]
fn opening_the_palette_closes_the_settings_and_the_sheet() {
    // SHELL-KEYS-13.
    for (first, second) in [(Settings, Palette), (Sheet, Palette), (Palette, Settings), (Palette, Sheet), (Settings, Sheet)] {
        let mut stack = Stack::default();
        stack.open(up(first));
        assert_eq!(stack.open(up(second)), Opening::Opened { closed: vec![first] });
        assert_eq!(stack.kinds(), [second], "{first:?} then {second:?}");
    }
}

#[test]
fn a_dialog_is_not_closed_by_the_palette_and_stays_under_it() {
    // CROSS-12: settings can open over a dialog, and the dialog is still there under it.
    let mut stack = Stack::default();
    stack.open(up(AddProject));
    assert_eq!(stack.open(up(Settings)), Opening::Opened { closed: vec![] });
    assert_eq!(stack.kinds(), [AddProject, Settings]);
    stack.open(up(Palette));
    assert_eq!(stack.kinds(), [AddProject, Palette], "only the exclusive ones were closed");
}

#[test]
fn opening_what_is_up_changes_nothing() {
    let mut stack = Stack::default();
    stack.open(up(Settings));
    stack.open(up(Rename));
    assert_eq!(stack.open(up(Settings)), Opening::Already);
    assert_eq!(stack.kinds(), [Settings, Rename], "it did not move to the top either");
}

#[test]
fn a_close_says_where_the_overlay_was() {
    let mut stack = Stack::default();
    stack.open(up(AddProject));
    stack.open(up(Rename));
    assert_eq!(stack.close(AddProject), Some(0));
    assert!(stack.is_top(Rename));
    assert_eq!(stack.close(AddProject), None);
}

#[test]
fn escape_answers_the_topmost_overlay_that_can_be_dismissed() {
    let mut stack = Stack::default();
    stack.open(up(AddProject));
    stack.open(Up { kind: Stopped, dismissable: false });
    assert_eq!(stack.escape(), Some(AddProject), "the stopped card holds no Escape; what is under it hears it");
    stack.open(up(Rename));
    assert_eq!(stack.escape(), Some(Rename));
    stack.close(Rename);
    stack.close(AddProject);
    assert_eq!(stack.escape(), None, "nothing left that Escape could close");
}

#[test]
fn the_pane_regains_the_keys_only_when_it_had_them_none_is_taken_and_a_pane_is_shown() {
    // SHELL-SEAM-10.
    assert!(pane_regains_the_keys(true, false, true));
    assert!(!pane_regains_the_keys(false, false, true), "it never had them");
    assert!(!pane_regains_the_keys(true, true, true), "another dialog still has them");
    assert!(!pane_regains_the_keys(true, false, false), "a key sent to a pane nobody sees");
}

#[test]
fn only_a_dialog_that_takes_the_keys_from_a_pane_counts() {
    let mut stack = Stack::default();
    stack.open(up(Rename));
    assert!(!stack.takes_from_pane());
    stack.open(up(AddProject));
    assert!(stack.takes_from_pane());
    stack.close(AddProject);
    assert!(!stack.takes_from_pane());
}
