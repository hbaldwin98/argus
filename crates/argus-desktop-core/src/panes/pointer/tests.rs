use super::*;

fn down(pointer: &mut Pointer, at: Cell) -> Option<Gesture> {
    pointer.press(0, false, false, at)
}

#[test]
fn the_cell_under_a_point_is_whole_cells_from_the_corner() {
    assert_eq!(cell_at(0.0, 0.0, 8.0, 20.0, 24, 80), (0, 0));
    assert_eq!(cell_at(15.9, 39.9, 8.0, 20.0, 24, 80), (1, 1));
    assert_eq!(cell_at(16.0, 40.0, 8.0, 20.0, 24, 80), (2, 2));
}

#[test]
fn a_point_past_an_edge_is_the_cell_at_the_edge() {
    assert_eq!(cell_at(-30.0, -5.0, 8.0, 20.0, 24, 80), (0, 0));
    assert_eq!(cell_at(5000.0, 5000.0, 8.0, 20.0, 24, 80), (23, 79));
}

#[test]
fn a_grid_with_no_cells_has_one_at_the_corner() {
    assert_eq!(cell_at(40.0, 40.0, 8.0, 20.0, 0, 0), (0, 0));
    assert_eq!(cell_at(40.0, 40.0, 0.0, 0.0, 5, 5), (0, 0));
}

#[test]
fn a_press_the_child_asked_for_is_reported_with_its_button() {
    let mut pointer = Pointer::default();
    assert_eq!(
        pointer.press(2, false, true, (3, 4)),
        Some(Gesture::Mouse { gesture: PageMouse::Down, button: 2, at: (3, 4) })
    );
    assert!(pointer.active());
    assert_eq!(pointer.selection((9, 9)), None, "the child's press selects nothing");
}

#[test]
fn shift_keeps_a_press_for_a_selection_even_when_the_child_asked_for_the_mouse() {
    let mut pointer = Pointer::default();
    assert_eq!(pointer.press(0, true, true, (1, 1)), None);
    pointer.drag((1, 5));
    assert_eq!(pointer.selection((1, 5)), Some(((1, 1), (1, 5))));
}

#[test]
fn only_a_left_press_starts_a_selection() {
    let mut pointer = Pointer::default();
    assert_eq!(pointer.press(2, false, false, (1, 1)), None);
    assert!(!pointer.active(), "a right press anchors nothing");
    assert_eq!(pointer.release(2, (1, 1)), None);
    assert_eq!(pointer.press(1, false, false, (1, 1)), None);
    assert!(!pointer.active());
}

#[test]
fn a_selection_shows_only_once_the_pointer_has_left_the_anchor_cell() {
    let mut pointer = Pointer::default();
    down(&mut pointer, (2, 3));
    assert_eq!(pointer.selection((2, 3)), None);
    pointer.drag((2, 3));
    assert_eq!(pointer.selection((2, 3)), None, "moving inside the cell is no drag");
    pointer.drag((2, 4));
    assert_eq!(pointer.selection((2, 4)), Some(((2, 3), (2, 4))));
    // Back on the anchor after leaving it: it was a drag, and stays one.
    pointer.drag((2, 3));
    assert_eq!(pointer.selection((2, 3)), Some(((2, 3), (2, 3))));
}

#[test]
fn letting_go_after_a_drag_copies_between_the_two_cells_and_a_click_copies_nothing() {
    let mut pointer = Pointer::default();
    down(&mut pointer, (2, 3));
    pointer.drag((4, 0));
    assert_eq!(pointer.release(0, (4, 1)), Some(Gesture::Copy { from: (2, 3), to: (4, 1) }));
    assert!(!pointer.active());

    down(&mut pointer, (2, 3));
    assert_eq!(pointer.release(0, (2, 3)), None, "a press and release is a click");
    // The next press starts afresh: the old drag does not leak into it.
    down(&mut pointer, (5, 5));
    assert_eq!(pointer.release(0, (5, 5)), None);
}

#[test]
fn a_backward_drag_copies_from_where_it_began() {
    let mut pointer = Pointer::default();
    down(&mut pointer, (5, 9));
    pointer.drag((1, 2));
    assert_eq!(pointer.release(0, (1, 2)), Some(Gesture::Copy { from: (5, 9), to: (1, 2) }));
}

#[test]
fn a_childs_drag_is_reported_only_when_the_cell_changes() {
    let mut pointer = Pointer::default();
    pointer.press(0, false, true, (1, 1));
    assert_eq!(pointer.drag((1, 1)), None, "still the cell the press reported");
    assert_eq!(pointer.drag((1, 2)), Some(Gesture::Mouse { gesture: PageMouse::Drag, button: 0, at: (1, 2) }));
    assert_eq!(pointer.drag((1, 2)), None);
}

#[test]
fn the_release_of_another_button_leaves_the_childs_press_held() {
    let mut pointer = Pointer::default();
    pointer.press(0, false, true, (1, 1));
    assert_eq!(pointer.release(2, (1, 1)), None);
    assert!(pointer.active());
    assert_eq!(
        pointer.release(0, (3, 3)),
        Some(Gesture::Mouse { gesture: PageMouse::Up, button: 0, at: (3, 3) })
    );
    assert!(!pointer.active());
}

#[test]
fn a_move_with_no_button_is_reported_only_to_the_screen_with_the_keys_and_only_on_a_new_cell() {
    let mut pointer = Pointer::default();
    assert_eq!(pointer.hover((1, 1), true, false), None, "a screen without the keys reports no move");
    assert_eq!(pointer.hover((1, 1), false, true), None, "the child did not ask for the mouse");
    assert_eq!(
        pointer.hover((1, 1), true, true),
        Some(Gesture::Mouse { gesture: PageMouse::Move, button: 0, at: (1, 1) })
    );
    assert_eq!(pointer.hover((1, 1), true, true), None, "the same cell again");
    assert!(pointer.hover((1, 2), true, true).is_some());
}

#[test]
fn a_move_is_not_reported_while_a_press_is_being_followed() {
    let mut pointer = Pointer::default();
    down(&mut pointer, (0, 0));
    assert_eq!(pointer.hover((4, 4), true, true), None);
    pointer.cancel();
    assert!(pointer.hover((4, 4), true, true).is_some());
}

#[test]
fn a_release_the_window_never_saw_lets_the_childs_button_go_and_copies_nothing() {
    let mut pointer = Pointer::default();
    pointer.press(2, false, true, (1, 1));
    assert_eq!(
        pointer.lost((4, 4)),
        Some(Gesture::Mouse { gesture: PageMouse::Up, button: 2, at: (4, 4) })
    );
    assert!(!pointer.active());
    down(&mut pointer, (0, 0));
    pointer.drag((0, 9));
    assert_eq!(pointer.lost((0, 9)), None, "a selection lost is not copied");
    assert!(!pointer.active());
}

#[test]
fn a_press_the_pointer_lost_ends_with_no_copy_and_no_report() {
    let mut pointer = Pointer::default();
    down(&mut pointer, (0, 0));
    pointer.drag((0, 9));
    pointer.cancel();
    assert_eq!(pointer.release(0, (0, 9)), None);
    assert_eq!(pointer.selection((0, 9)), None);
}

#[test]
fn a_notch_is_a_hundred_pixels_or_three_lines() {
    let mut wheel = Wheel::default();
    assert_eq!(wheel.add(100.0, Unit::Pixels), 1);
    assert_eq!(wheel.add(-300.0, Unit::Pixels), -3);
    assert_eq!(wheel.add(3.0, Unit::Lines), 1);
    assert_eq!(wheel.add(-6.0, Unit::Lines), -2);
}

#[test]
fn a_touchpads_small_deltas_add_up_and_the_fraction_is_carried() {
    let mut wheel = Wheel::default();
    let sent: Vec<i32> = (0..5).map(|_| wheel.add(50.0, Unit::Pixels)).collect();
    assert_eq!(sent, [0, 1, 0, 1, 0], "50, 100 (one), 50, 100 (one), 50");
    let mut wheel = Wheel::default();
    assert_eq!(wheel.add(1.5, Unit::Lines), 0);
    assert_eq!(wheel.add(1.5, Unit::Lines), 1);
    assert_eq!(wheel.add(1.5, Unit::Lines), 0);
}

#[test]
fn reversing_the_wheel_cancels_what_was_carried() {
    let mut wheel = Wheel::default();
    assert_eq!(wheel.add(60.0, Unit::Pixels), 0);
    assert_eq!(wheel.add(-60.0, Unit::Pixels), 0);
    assert_eq!(wheel.add(100.0, Unit::Pixels), 1);
}

mod properties {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        // SHELL-INPUT-7: a drag past an edge selects to the edge, never outside the grid.
        #[test]
        fn the_cell_under_any_point_is_inside_the_grid(
            x in -5000.0f32..5000.0, y in -5000.0f32..5000.0,
            cell_w in 1.0f32..30.0, cell_h in 1.0f32..40.0, rows in 1u16..200, cols in 1u16..400,
        ) {
            let (row, col) = cell_at(x, y, cell_w, cell_h, rows, cols);
            prop_assert!(row < rows && col < cols);
        }

        // SHELL-INPUT-10: the whole notches sent and the fraction carried are everything heard.
        #[test]
        fn no_wheel_movement_is_lost_or_invented(deltas in prop::collection::vec(-250.0f32..250.0, 0..40)) {
            let mut wheel = Wheel::default();
            let sent: i32 = deltas.iter().map(|d| wheel.add(*d, Unit::Pixels)).sum();
            let heard: f32 = deltas.iter().sum::<f32>() / PIXELS_PER_NOTCH;
            prop_assert!((heard - sent as f32).abs() < 1.0 + 1e-3, "heard {heard}, sent {sent}");
        }
    }
}
