use super::*;
use crate::board::snapshots;

// BOARD-HEAD-5: Work counts its tasks, Record its decisions made and open, Flows its flows once they have come.
#[test]
fn each_lens_carries_what_it_has_to_show() {
    let mut view = snapshots::open("feature-flows");
    assert_eq!(lens_count(&view, Lens::Work), Some(view.tasks_total));
    assert_eq!(lens_count(&view, Lens::Record), Some(view.decided + view.open));
    assert_eq!(lens_count(&view, Lens::Flows), Some(view.flows.as_ref().unwrap().flows.len()));
    view.flows.as_mut().unwrap().loaded = false;
    assert_eq!(lens_count(&view, Lens::Flows), None, "no count until the flows have arrived");
    view.flows = None;
    assert_eq!(lens_count(&view, Lens::Flows), None, "nor while the lens is not the one read");
}

#[test]
fn the_lenses_are_in_the_order_and_with_the_keys_of_the_digits() {
    let keys: Vec<_> = LENSES.iter().map(|(lens, name, key)| (*lens, *name, *key)).collect();
    assert_eq!(keys, [(Lens::Work, "Work", "1"), (Lens::Record, "Record", "2"), (Lens::Flows, "Flows", "3")]);
}
