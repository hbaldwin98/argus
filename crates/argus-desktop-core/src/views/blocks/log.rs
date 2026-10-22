//! Where a log is read from: a log is a tail, so one nobody scrolled up in follows its newest line.

/// How near the end, in pixels, still counts as at it (`log.ts` reads `< 4`).
pub const AT_END_WITHIN: f32 = 4.0;

/// Whether a log scrolled to `top` is at its end, given the height of its lines and of its box.
pub fn at_end(top: f32, content: f32, shown: f32) -> bool {
    content - top - shown < AT_END_WITHIN
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_log_within_four_pixels_of_its_end_is_at_it() {
        assert!(at_end(680.0, 1000.0, 320.0));
        assert!(at_end(677.0, 1000.0, 320.0));
        assert!(!at_end(676.0, 1000.0, 320.0));
    }

    #[test]
    fn a_log_that_fits_its_box_is_at_its_end() {
        assert!(at_end(0.0, 100.0, 320.0));
    }
}
