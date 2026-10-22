use proptest::prelude::*;

use super::*;
use crate::code::vim::tests::Ed;

#[test]
fn a_number_is_written_in_its_base_in_lower_case_with_a_sign() {
    assert_eq!(written(255, 16), "ff");
    assert_eq!(written(-255, 16), "-ff");
    assert_eq!(written(5, 2), "101");
    assert_eq!(written(0, 10), "0");
    assert_eq!(written(8, 8), "10");
    assert_eq!(written(-1, 10), "-1");
}

#[test]
fn the_count_is_how_much_it_changes_by() {
    let mut ed = Ed::new("|x 5");
    ed.keys("10<C-a>");
    assert_eq!(ed.text(), "x 15");
    ed.keys("0100<C-x>");
    assert_eq!(ed.text(), "x -85");
}

#[test]
fn a_number_ending_before_the_cursor_is_passed_over() {
    let mut ed = Ed::new("1 2 |x 3");
    ed.keys("<C-a>");
    assert_eq!(ed.text(), "1 2 x 4");
}

#[test]
fn only_the_cursors_line_is_looked_at() {
    let mut ed = Ed::new("|abc\n5");
    ed.keys("<C-a>");
    assert_eq!(ed.text(), "abc\n5");
}

#[test]
fn a_dot_repeats_it_with_the_same_count() {
    let mut ed = Ed::new("|1");
    ed.keys("5<C-a>.");
    assert_eq!(ed.text(), "11");
}

#[test]
fn a_number_too_big_for_the_machine_is_left() {
    let mut ed = Ed::new("|99999999999999999999999999999999999999999999");
    ed.keys("<C-a>");
    assert_eq!(ed.text(), "99999999999999999999999999999999999999999999");
}

proptest! {
    // Up by n and down by n is where it began, for a decimal number that has no sign to lose.
    #[test]
    fn up_and_down_by_the_same_count_is_no_change(n in 1u32..5000, by in 1usize..500) {
        let mut ed = Ed::new(&format!("|{n}"));
        ed.keys(&format!("{by}<C-a>{by}<C-x>"));
        prop_assert_eq!(ed.text(), n.to_string());
    }

    // A hex number keeps its prefix and at least its digits' width.
    #[test]
    fn hex_keeps_its_prefix_and_width(n in 0u32..0xffff, width in 1usize..8) {
        let digits = format!("{n:0width$x}");
        let mut ed = Ed::new(&format!("|0x{digits}"));
        ed.keys("<C-a>");
        let text = ed.text();
        prop_assert!(text.starts_with("0x"));
        prop_assert!(text.len() >= 2 + digits.len());
        prop_assert_eq!(u32::from_str_radix(&text[2..], 16).unwrap(), n + 1);
    }
}
