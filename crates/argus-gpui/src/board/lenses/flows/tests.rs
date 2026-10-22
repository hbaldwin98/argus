use super::*;
use crate::board::snapshots;

// BOARD-WORK-21: the lit step's row is a child of the list, after the label, what the list says when it
// holds nothing, every flow, and the label over the steps.
#[test]
fn the_lit_steps_row_is_counted_past_the_label_the_flows_and_the_steps_label() {
    let view = snapshots::open("feature-flows");
    let flows = view.flows.as_ref().unwrap();
    let at = flows.flow.as_ref().unwrap().rows.iter().position(|row| row.on).unwrap();
    assert_eq!(lit_child(&view), Some(1 + flows.flows.len() + 1 + at));
    let mut empty = view.clone();
    empty.flows.as_mut().unwrap().flows.clear();
    assert_eq!(lit_child(&empty), Some(1 + 1 + 1 + at), "a line that says there are none takes a child");
}

#[test]
fn no_lit_step_no_row_to_bring() {
    let mut view = snapshots::open("feature-flows");
    view.flows.as_mut().unwrap().flow = None;
    assert_eq!(lit_child(&view), None);
    assert_eq!(lit_child(&snapshots::open("feature-work")), None, "another lens has no flows");
}

// BOARD-FLOWS-3: the page's `.steprow` track is `minmax(80px, 150px)` and grows to 150 with room, so the
// longest route of the fixture (`argusd ⇢ hook`) is shown whole, not cut to `argusd ⇢ ho…`.
#[test]
fn the_route_column_is_wide_enough_for_the_longest_route_of_the_fixture() {
    let tokens = crate::theme::scale::Scale::root();
    let view = snapshots::open("feature-flows");
    let flow = view.flows.as_ref().unwrap().flow.as_ref().unwrap();
    let widest = flow.rows.iter().map(|row| row.route.chars().count()).max().unwrap() as f32;
    assert!(widest * tokens.fs_sm * 0.6 <= steps::route_width(&tokens));
    assert_eq!(steps::route_width(&tokens), tokens.steprow_route_max);
}
