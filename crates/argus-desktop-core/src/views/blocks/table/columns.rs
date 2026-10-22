//! Which columns of a table give way as the box it is drawn in narrows, before it scrolls sideways.
//!
//! A column its author marked optional goes under 560 px. An extension's table also gives up its
//! last four columns one at a time, the last first, so the name and what it is doing stay whole in
//! a narrow dock (`.vtable-drop-N`, `.vtable-optional`).

/// How many of an extension's trailing columns give way.
pub const DROPPABLE: usize = 4;

/// The box width, in the page's pixels, at and under which a column of rank 1 to 4 is dropped, as
/// the page's container queries say.
const DROP_AT: [f32; DROPPABLE] = [700.0, 620.0, 540.0, 460.0];

/// The width at and under which an optional column is dropped.
pub const OPTIONAL_AT: f32 = 560.0;

/// The rank of column `c` of `count` among those that may be dropped, 1 for the last; 0 for one that
/// stays. The first two columns always stay.
pub fn drop_rank(c: usize, count: usize) -> usize {
    let rank = count - c;
    if c >= 2 && rank <= DROPPABLE {
        rank
    } else {
        0
    }
}

/// Whether a column is drawn in a box `width` wide. A width of zero is a box not yet measured, in
/// which nothing is dropped.
pub fn shown(optional: bool, rank: usize, width: f32) -> bool {
    if width <= 0.0 {
        return true;
    }
    if optional && width <= OPTIONAL_AT {
        return false;
    }
    rank == 0 || width > DROP_AT[rank - 1]
}

#[cfg(test)]
mod tests;
