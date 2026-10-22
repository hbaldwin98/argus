//! The line numbers beside the note (BOARD-CAPTURE-6): one for each line of the text, as tall as the
//! rows the line takes once it is wrapped, scrolled in step with the text, and a line the grammar
//! could not place drawn in the danger colour and strong.
//!
//! The page measured the wrapped lines from a hidden copy of its text field. The kit's textarea knows
//! where it broke each line (`DisplayMap`), so the numbers ask it; the rows are known once the field has
//! been laid out at its width, and until then every line is one row.

use gpui_kit::component::input::TextareaState;
use gpui_kit::component::v_flex;
use gpui_kit::{div, prelude::*, px, App, Entity, Pixels, SharedString, TestSupportExt as _};

use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// How many numbers there are for `text`: its lines, and one for an empty note.
pub fn count(text: &str) -> usize {
    text.split('\n').count().max(1)
}

/// Whether the one-based `line` is one the grammar found no place for.
pub fn unread(unread: &[usize], line: usize) -> bool {
    unread.contains(&line)
}

/// How many rows each of `lines` lines takes in `state`, as the field last laid them out; one each
/// before it has been laid out.
pub fn rows(state: &Entity<TextareaState>, lines: usize, cx: &App) -> Vec<usize> {
    let state = state.read(cx);
    let map = state.display_map();
    let known = map.wrap_row_count() > 0;
    (0..lines).map(|line| if known { map.visible_wrap_row_count_for_buffer_line(line).max(1) } else { 1 }).collect()
}

/// `.note-gutter`: the numbers, right-aligned in a column 36 wide, each `rows * line_height` tall, the
/// column moved up by how far the field is scrolled.
pub fn gutter(rows: &[usize], unread_lines: &[usize], line_height: Pixels, scrolled: Pixels, tokens: &Tokens) -> impl IntoElement {
    let numbers = rows.iter().enumerate().map(|(at, rows)| {
        let line = at + 1;
        let lit = unread(unread_lines, line);
        div()
            .id(("capture-line", line))
            .test_support()
            .aria_label(SharedString::from(format!("{line}")))
            .h(line_height * *rows as f32)
            .text_color(if lit { tokens.danger } else { tokens.fg_faint })
            .when(lit, |number| number.weighted(tokens.weight_semibold))
            .child(SharedString::from(line.to_string()))
    });
    div()
        .id("capture-gutter")
        .test_support()
        .flex_shrink_0()
        .w(rem(tokens.note_gutter_w))
        .h_full()
        .overflow_hidden()
        .pt(rem(tokens.sp_8))
        .pr(rem(tokens.sp_12))
        .font_family(SharedString::from(tokens.font_mono))
        .typed(tokens.type_mono)
        .text_right()
        .child(v_flex().mt(scrolled.min(px(0.0))).children(numbers))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_has_a_number_for_each_line_and_an_empty_one_has_one() {
        assert_eq!(count(""), 1);
        assert_eq!(count("a"), 1);
        assert_eq!(count("a\nb\n"), 3, "a trailing newline starts a line the cursor can be on");
    }

    #[test]
    fn only_a_line_the_grammar_could_not_place_is_marked() {
        assert!(unread(&[3, 5], 5) && !unread(&[3, 5], 4));
    }
}
