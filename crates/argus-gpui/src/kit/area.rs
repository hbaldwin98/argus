//! Records the area an element was laid out in, for whoever needs to turn a pointer's place into a
//! place in it.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::{canvas, prelude::*, Bounds, Pixels};

/// Records the area its parent was laid out in.
pub fn measure(into: Rc<Cell<Bounds<Pixels>>>) -> impl IntoElement {
    canvas(move |bounds, _, _| into.set(bounds), |_, _, _, _| {}).absolute().size_full()
}
