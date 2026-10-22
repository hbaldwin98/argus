//! The places a carried pane can be dropped, published by the views that draw them.
//!
//! The drag that carries a pane belongs to the work area, while the cells it can land on are the
//! split grid's and the checkouts it can move to are the rail's: two views the drag host does not
//! own and cannot ask. So each view says where its targets were last drawn (the page asked the
//! DOM with `elementFromPoint`; here a view paints a [`publish`] element), and the drag asks this
//! registry which one a point is over.
//!
//! A view forgets its own kind of spot when it renders, so one that is gone is not a target, and
//! publishes again when it paints. What is lit while a pane is carried is kept here too, since the
//! rail has to draw a checkout as a target without being the drag's host.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::{canvas, prelude::*, App, Bounds, Global, Pixels, Point, Window};

/// A place something can be dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spot {
    /// A cell of the Workspace's grid, by its number.
    Cell(u8),
    /// The stage body the Workspace's grid fills.
    Stage,
    /// A checkout's row in the rail.
    Checkout(u64),
}

#[derive(Default)]
struct State {
    spots: Vec<(Spot, Bounds<Pixels>)>,
    /// The checkout a carried pane would move to.
    lit: Option<u64>,
    /// A pane is being carried: the grid keeps the layout it drew when the carrying began.
    carrying: bool,
}

/// The registry. A handle: clones share one list.
#[derive(Clone, Default)]
pub struct Spots(Rc<RefCell<State>>);

impl Global for Spots {}

impl Spots {
    pub fn of(cx: &mut App) -> Self {
        cx.default_global::<Spots>().clone()
    }

    /// Reads the registry from a context that may not make it.
    pub fn peek(cx: &App) -> Self {
        cx.try_global::<Spots>().cloned().unwrap_or_default()
    }

    /// Says where `spot` was drawn.
    pub fn set(&self, spot: Spot, bounds: Bounds<Pixels>) {
        let mut state = self.0.borrow_mut();
        match state.spots.iter_mut().find(|(held, _)| *held == spot) {
            Some(entry) => entry.1 = bounds,
            None => state.spots.push((spot, bounds)),
        }
    }

    pub fn get(&self, spot: Spot) -> Option<Bounds<Pixels>> {
        self.0.borrow().spots.iter().find(|(held, _)| *held == spot).map(|(_, bounds)| *bounds)
    }

    /// Forgets every spot `gone` names.
    pub fn forget(&self, gone: impl Fn(Spot) -> bool) {
        self.0.borrow_mut().spots.retain(|(spot, _)| !gone(*spot));
    }

    /// The grid's cells and where they were drawn, by number.
    pub fn cells(&self) -> Vec<(u8, Bounds<Pixels>)> {
        let mut cells: Vec<_> = self
            .0
            .borrow()
            .spots
            .iter()
            .filter_map(|(spot, bounds)| if let Spot::Cell(n) = spot { Some((*n, *bounds)) } else { None })
            .collect();
        cells.sort_by_key(|(n, _)| *n);
        cells
    }

    /// The checkout row a point is over.
    pub fn checkout_at(&self, at: Point<Pixels>) -> Option<u64> {
        self.0.borrow().spots.iter().find_map(|(spot, bounds)| match spot {
            Spot::Checkout(id) if bounds.contains(&at) => Some(*id),
            _ => None,
        })
    }

    /// Lights a checkout as the place a carried pane would move to, or none. Whether it changed.
    pub fn light(&self, checkout: Option<u64>) -> bool {
        let mut state = self.0.borrow_mut();
        let changed = state.lit != checkout;
        state.lit = checkout;
        changed
    }

    /// Says a pane is carried, or no longer. Whether it changed.
    pub fn carry(&self, carrying: bool) -> bool {
        let mut state = self.0.borrow_mut();
        let changed = state.carrying != carrying;
        state.carrying = carrying;
        changed
    }

    pub fn carrying(&self) -> bool {
        self.0.borrow().carrying
    }

    pub fn lit(&self) -> Option<u64> {
        self.0.borrow().lit
    }
}

/// An element that says its parent's area is where `spot` is. It is put among the parent's
/// children and paints nothing.
pub fn publish(spot: Spot) -> impl IntoElement {
    canvas(
        move |bounds, _: &mut Window, cx: &mut App| Spots::of(cx).set(spot, bounds),
        |_, _, _, _| {},
    )
    // From the corner of the padding box: an absolute child with no inset sits inside the padding.
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{point, px, size};

    fn at(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
    }

    #[test]
    fn a_spot_is_where_it_was_last_said_to_be() {
        let spots = Spots::default();
        spots.set(Spot::Cell(0), at(0.0, 0.0, 100.0, 100.0));
        spots.set(Spot::Cell(0), at(10.0, 0.0, 100.0, 100.0));
        assert_eq!(spots.get(Spot::Cell(0)), Some(at(10.0, 0.0, 100.0, 100.0)));
        assert_eq!(spots.get(Spot::Cell(1)), None);
    }

    #[test]
    fn the_cells_come_back_in_their_numbers_and_the_other_kinds_are_not_among_them() {
        let spots = Spots::default();
        spots.set(Spot::Cell(2), at(0.0, 0.0, 1.0, 1.0));
        spots.set(Spot::Stage, at(0.0, 0.0, 9.0, 9.0));
        spots.set(Spot::Cell(0), at(0.0, 0.0, 1.0, 1.0));
        spots.set(Spot::Checkout(7), at(0.0, 0.0, 1.0, 1.0));
        assert_eq!(spots.cells().iter().map(|(n, _)| *n).collect::<Vec<_>>(), [0, 2]);
    }

    #[test]
    fn a_view_forgets_its_own_kind_and_no_other() {
        let spots = Spots::default();
        spots.set(Spot::Cell(0), at(0.0, 0.0, 1.0, 1.0));
        spots.set(Spot::Checkout(7), at(0.0, 0.0, 1.0, 1.0));
        spots.forget(|spot| matches!(spot, Spot::Checkout(_)));
        assert!(spots.get(Spot::Checkout(7)).is_none());
        assert!(spots.get(Spot::Cell(0)).is_some());
    }

    #[test]
    fn the_checkout_row_under_a_point_is_found_and_nothing_elsewhere() {
        let spots = Spots::default();
        spots.set(Spot::Checkout(7), at(0.0, 100.0, 200.0, 24.0));
        spots.set(Spot::Cell(0), at(0.0, 0.0, 500.0, 500.0));
        assert_eq!(spots.checkout_at(point(px(50.0), px(110.0))), Some(7));
        assert_eq!(spots.checkout_at(point(px(50.0), px(130.0))), None);
    }

    #[test]
    fn lighting_says_whether_it_changed() {
        let spots = Spots::default();
        assert!(spots.light(Some(3)));
        assert!(!spots.light(Some(3)));
        assert_eq!(spots.lit(), Some(3));
        assert!(spots.light(None));
    }
}
