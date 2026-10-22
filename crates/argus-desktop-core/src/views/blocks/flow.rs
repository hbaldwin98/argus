//! How many equal columns fit a width, as the page's `repeat(auto-fit | auto-fill, minmax(min, 1fr))`
//! grids count them: the columns block, the grid block and the stats block.
//!
//! A webview lays these out by itself; GPUI has no such track, so the window measures the box and
//! asks here. A box not measured yet is asked with a width of zero and gets one column, the
//! stacked form, which is never wrong, only narrower than it will be a frame later.

/// What the extra tracks of a grid do when there are fewer items than tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fill {
    /// `auto-fit`: empty tracks collapse, so the items stretch across the whole width.
    Fit,
    /// `auto-fill`: empty tracks stay, so an item is never wider than a full row's would be.
    Fill,
}

/// The columns of a grid `width` wide whose columns are at least `min` and whose gaps are `gap`
/// (all in the same unit) holding `items` items; never fewer than one.
pub fn columns(width: f32, min: f32, gap: f32, items: usize, fill: Fill) -> usize {
    let fits = if width.is_finite() && width > 0.0 { ((width + gap) / (min + gap)).floor().max(1.0) as usize } else { 1 };
    match fill {
        Fill::Fit => fits.min(items.max(1)),
        Fill::Fill => fits,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_many_columns_fit_as_their_minimums_and_gaps_allow() {
        // 3 * 240 + 2 * 16 = 752 is the narrowest three columns fit.
        assert_eq!(columns(752.0, 240.0, 16.0, 9, Fill::Fill), 3);
        assert_eq!(columns(751.0, 240.0, 16.0, 9, Fill::Fill), 2);
        assert_eq!(columns(240.0, 240.0, 16.0, 9, Fill::Fill), 1);
    }

    #[test]
    fn a_box_narrower_than_one_column_or_not_measured_is_one_column() {
        assert_eq!(columns(100.0, 240.0, 16.0, 4, Fill::Fit), 1);
        assert_eq!(columns(0.0, 240.0, 16.0, 4, Fill::Fit), 1);
        assert_eq!(columns(f32::NAN, 240.0, 16.0, 4, Fill::Fill), 1);
    }

    #[test]
    fn auto_fit_collapses_the_tracks_there_are_no_items_for_and_auto_fill_keeps_them() {
        assert_eq!(columns(1200.0, 240.0, 16.0, 2, Fill::Fit), 2);
        assert_eq!(columns(1200.0, 240.0, 16.0, 2, Fill::Fill), 4);
        assert_eq!(columns(1200.0, 240.0, 16.0, 0, Fill::Fit), 1, "no items is still a box");
    }
}
