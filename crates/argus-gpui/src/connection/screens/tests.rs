use super::*;
use argus_desktop_core::connection::page::FrameCursor;

fn frame(pane: u64, side: Side) -> Frame {
    Frame {
        pane,
        side,
        rows: 2,
        cols: 8,
        full: true,
        cursor: FrameCursor { row: 0, col: 0, visible: true, shape: "block" },
        alternate_screen: false,
        mouse: false,
        lines: Vec::new(),
        parked: None,
    }
}

fn marked(side: Side) -> Marked {
    Marked { side, selection: Selection { anchor: (0, 0), head: (1, 3) } }
}

#[test]
fn the_workspace_and_the_consoles_two_sides_have_screens_from_the_start() {
    let mut screens = Screens::new();
    for side in [Side::Workspace, Side::First, Side::Second] {
        assert_eq!(screens.apply(frame(1, side)), Some(side));
        assert_eq!(screens.of(side).pane(), Some(1));
    }
}

#[test]
fn a_frame_for_a_side_with_no_screen_is_dropped_until_the_side_is_bound() {
    let mut screens = Screens::new();
    for side in [Side::Beside(0), Side::View(2)] {
        assert_eq!(screens.apply(frame(4, side)), None);
        assert_eq!(screens.of(side).pane(), None);
    }
    screens.bind(Side::Beside(0));
    assert_eq!(screens.apply(frame(4, Side::Beside(0))), Some(Side::Beside(0)));
    assert_eq!(screens.of(Side::Beside(0)).pane(), Some(4));
    assert_eq!(screens.of(Side::Beside(1)).pane(), None, "each side is its own");
}

#[test]
fn binding_a_side_again_keeps_what_it_holds() {
    let mut screens = Screens::new();
    screens.bind(Side::Beside(0));
    screens.apply(frame(4, Side::Beside(0)));
    screens.bind(Side::Beside(0));
    assert_eq!(screens.of(Side::Beside(0)).pane(), Some(4));
}

#[test]
fn an_unbound_side_forgets_its_rows_and_the_three_always_there_stay() {
    let mut screens = Screens::new();
    screens.bind(Side::Beside(0));
    screens.apply(frame(4, Side::Beside(0)));
    screens.unbind(Side::Beside(0));
    assert_eq!(screens.of(Side::Beside(0)).pane(), None);
    assert_eq!(screens.apply(frame(4, Side::Beside(0))), None);
    screens.apply(frame(9, Side::Workspace));
    screens.unbind(Side::Workspace);
    assert_eq!(screens.of(Side::Workspace).pane(), Some(9));
}

#[test]
fn there_is_one_selection_and_a_new_one_names_the_side_it_left_and_the_side_it_came_to() {
    let mut screens = Screens::new();
    assert_eq!(screens.mark(Some(marked(Side::Workspace))), [Side::Workspace]);
    assert_eq!(screens.mark(Some(marked(Side::Workspace))), Vec::<Side>::new(), "nothing changed");
    assert_eq!(screens.mark(Some(marked(Side::First))), [Side::Workspace, Side::First]);
    assert_eq!(screens.selection_of(Side::Workspace), None);
    assert!(screens.selection_of(Side::First).is_some());
    assert_eq!(screens.mark(None), [Side::First]);
    assert_eq!(screens.marked(), None);
}

#[test]
fn another_panes_frame_clears_the_selection_on_its_side_and_the_same_panes_does_not() {
    let mut screens = Screens::new();
    screens.apply(frame(1, Side::Workspace));
    screens.mark(Some(marked(Side::Workspace)));
    screens.apply(frame(1, Side::Workspace));
    assert!(screens.selection_of(Side::Workspace).is_some(), "the same pane draws on");
    screens.apply(frame(2, Side::First));
    assert!(screens.selection_of(Side::Workspace).is_some(), "another side's frame is not this side's");
    screens.apply(frame(2, Side::Workspace));
    assert_eq!(screens.marked(), None, "pane 2's rows are not pane 1's");
}

#[test]
fn unbinding_the_side_a_selection_is_on_clears_it() {
    let mut screens = Screens::new();
    screens.bind(Side::Beside(1));
    screens.mark(Some(marked(Side::Beside(1))));
    assert!(screens.unbind(Side::Beside(1)));
    assert_eq!(screens.marked(), None);
}
