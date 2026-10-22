use crate::code::vim::marks::{ANCHOR, HEAD};
use crate::code::vim::tests::Ed;

#[test]
fn leaving_a_selection_keeps_its_ends_for_the_angle_marks_and_gv() {
    let mut ed = Ed::new("|abc def");
    ed.keys("wve<Esc>");
    let marks = &ed.vim.buf.marks;
    assert_eq!((marks.get('<'), marks.get('>')), (Some(4), Some(6)));
    assert_eq!((marks.get(ANCHOR), marks.get(HEAD)), (Some(4), Some(6)));
}

#[test]
fn a_selection_made_backward_has_its_marks_the_right_way_round() {
    let mut ed = Ed::new("abc de|f");
    ed.keys("vb<Esc>");
    let marks = &ed.vim.buf.marks;
    assert_eq!((marks.get('<'), marks.get('>')), (Some(4), Some(6)));
    assert_eq!((marks.get(ANCHOR), marks.get(HEAD)), (Some(6), Some(4)), "gv keeps which end the cursor was at");
}

#[test]
fn a_jump_to_where_the_cursor_already_is_is_not_recorded() {
    let mut ed = Ed::new("|a\nb");
    ed.keys("gg");
    ed.keys("<C-o>");
    assert_eq!(ed.shows(), "|a\nb");
    assert_eq!(ed.vim.buf.marks.get('\''), Some(0), "nothing recorded: the start");
}

#[test]
fn a_walk_that_goes_nowhere_leaves_the_cursor() {
    let mut ed = Ed::new("a|bc");
    ed.keys("<C-o><C-i>");
    assert_eq!(ed.shows(), "a|bc");
}

#[test]
fn a_mark_set_in_a_selection_is_the_cursor_end() {
    let mut ed = Ed::new("|abc");
    ed.keys("vlmx<Esc>");
    assert_eq!(ed.vim.buf.marks.get('x'), Some(1));
}
