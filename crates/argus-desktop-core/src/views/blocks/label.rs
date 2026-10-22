//! How long an agent's label is let to be in a drawing, and how wide it is taken to be: a box in a
//! graph or a sequence diagram is sized from its text before anything is measured, so the layout is
//! numbers and the same in both windows.

/// How wide one character is taken to be, in the drawing's units. An estimate, as the page's: a
/// wide glyph overflows its box, and the whole text is under the drawing's source or its tooltip.
pub const CHAR_W: f32 = 7.0;

/// `text` held to `max` characters, the last of them an ellipsis when it was cut.
pub fn clip(text: &str, max: usize) -> String {
    let mut chars = text.chars();
    let head: String = chars.by_ref().take(max).collect();
    if chars.next().is_none() {
        return head;
    }
    let mut cut: String = head.chars().take(max.saturating_sub(1)).collect();
    cut.push('\u{2026}');
    cut
}

/// How wide `text` is once held to `max` characters.
pub fn width(text: &str, max: usize) -> f32 {
    clip(text, max).chars().count() as f32 * CHAR_W
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_within_the_limit_is_left_alone() {
        assert_eq!(clip("abc", 3), "abc");
        assert_eq!(clip("", 3), "");
    }

    #[test]
    fn a_longer_label_is_cut_to_the_limit_with_an_ellipsis_in_the_last_place() {
        assert_eq!(clip("abcd", 3), "ab\u{2026}");
        assert_eq!(clip("abcd", 3).chars().count(), 3);
    }

    #[test]
    fn characters_are_counted_not_bytes() {
        assert_eq!(clip("\u{e9}\u{e9}\u{e9}", 3), "\u{e9}\u{e9}\u{e9}");
        assert_eq!(width("\u{e9}\u{e9}", 28), 14.0);
    }

    #[test]
    fn the_width_is_that_of_the_clipped_text() {
        assert_eq!(width(&"x".repeat(100), 28), 28.0 * CHAR_W);
        assert_eq!(width("ab", 28), 2.0 * CHAR_W);
    }
}
