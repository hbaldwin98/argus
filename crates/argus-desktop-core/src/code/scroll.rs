//! Where the file scrolls to when the cursor moves, so the person is not left looking at lines that
//! are not the ones they went to (`code/stage.ts`, `drawCode`, CODE-41).
//!
//! Only a move of the cursor brings it into view, and only when it is off screen: above the top, or
//! within two rows of the bottom. It is then placed a third of the way down, with the lines before
//! it for context, not pinned to the edge it was scrolled to. A redraw that moved nothing leaves
//! the scroll alone, so the file does not slide from under a click.

/// How far to scroll the file so the cursor row, `row_top` from the top of the content and `row_h`
/// tall, is in view in a viewport `viewport` tall scrolled `scrolled`; `None` when it is.
pub fn into_view(row_top: f32, row_h: f32, scrolled: f32, viewport: f32) -> Option<f32> {
    let above = row_top - scrolled;
    (above < 0.0 || above > viewport - row_h * 2.0).then(|| (row_top - viewport / 3.0).max(0.0))
}

/// What a page's cursor is, to tell a move from a redraw: the file and the line.
pub fn cursor_key(path: Option<&str>, at: u32) -> (Option<String>, u32) {
    (path.map(str::to_owned), at)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CODE-41: a cursor in view stays put, one off screen is placed a third of the way down.
    #[test]
    fn a_cursor_in_view_is_left_and_one_off_screen_is_put_a_third_of_the_way_down() {
        assert_eq!(into_view(500.0, 20.0, 400.0, 600.0), None);
        assert_eq!(into_view(100.0, 20.0, 400.0, 600.0), Some(0.0), "above the top, near the file's start");
        assert_eq!(into_view(2000.0, 20.0, 400.0, 600.0), Some(1800.0));
    }

    #[test]
    fn within_two_rows_of_the_bottom_counts_as_off_screen() {
        assert_eq!(into_view(959.0, 20.0, 400.0, 600.0), None, "559 of 600, with 41 to spare");
        assert_eq!(into_view(961.0, 20.0, 400.0, 600.0), Some(761.0));
    }

    #[test]
    fn the_cursor_is_the_file_and_the_line() {
        assert_ne!(cursor_key(Some("a"), 1), cursor_key(Some("b"), 1));
        assert_ne!(cursor_key(Some("a"), 1), cursor_key(Some("a"), 2));
        assert_eq!(cursor_key(None, 0), cursor_key(None, 0));
    }
}
