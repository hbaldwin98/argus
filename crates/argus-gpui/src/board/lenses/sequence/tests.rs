use argus_desktop_core::board::lenses::picture::layout;

use super::*;
use crate::board::lenses::flows::picture::{lit_offset, shown};
use crate::board::snapshots;

fn flow() -> argus_desktop_core::connection::page::FlowDetail {
    snapshots::open("feature-flows").flows.and_then(|flows| flows.flow).expect("a flow")
}

// BOARD-SEQ-14: a drag names a lifeline as the source calls it, and the diagram it was drawn from.
#[test]
fn what_the_diagram_shows_names_the_flow_and_each_lifeline_as_the_source_calls_it() {
    let flow = flow();
    let drawn = shown(&flow, 0.7);
    assert_eq!((drawn.diagram, drawn.scale), (flow.id, 0.7));
    assert_eq!(drawn.names, flow.diagram.lifelines.iter().map(|l| l.name.clone()).collect::<Vec<_>>());
    assert_eq!(drawn.picture.heads.len(), flow.diagram.lifelines.len());
}

// BOARD-FLOWS-12: the lit step is scrolled into view only if it is outside the box, 24px past the edge.
#[test]
fn the_inline_box_scrolls_sideways_only_when_the_lit_step_is_out_of_it() {
    let picture = layout(&flow().diagram);
    let lit = picture.arrows.iter().find(|arrow| arrow.on).expect("a step is lit");
    let reach = crate::board::lenses::flows::picture::reach(lit).1;
    assert_eq!(lit_offset(0.0, 10_000.0, &picture, 1.0, 24.0), 0.0, "a box that shows everything stays");
    let narrow = reach - 40.0;
    let moved = lit_offset(0.0, narrow, &picture, 1.0, 24.0);
    assert_eq!(moved, reach + 24.0 - narrow, "the end of the arrow brought 24 inside the edge");
    assert_eq!(lit_offset(moved, narrow, &picture, 1.0, 24.0), moved, "and left there");
}

#[test]
fn a_diagram_with_no_lit_step_does_not_move() {
    let mut flow = flow();
    flow.step = None;
    let mut flow_picture = layout(&flow.diagram);
    for arrow in &mut flow_picture.arrows {
        arrow.on = false;
    }
    assert_eq!(lit_offset(37.0, 100.0, &flow_picture, 1.0, 24.0), 37.0);
}

#[test]
fn a_press_is_a_step_or_a_lifeline() {
    assert_ne!(Press::Step(1), Press::Lifeline(1));
}
