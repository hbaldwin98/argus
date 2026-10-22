use proptest::prelude::*;

use super::*;
use crate::code::vim::tests::Ed;

#[test]
fn the_flags_are_read_in_any_order_and_a_pattern_after_them() {
    let a = parse_args(" iu").unwrap();
    assert!(a.ignore_case && a.unique && !a.reverse && a.radix.is_none());
    let a = parse_args("! n").unwrap();
    assert!(a.reverse && a.radix == Some(10));
    let a = parse_args(" x").unwrap();
    assert_eq!(a.radix, Some(16));
    let a = parse_args(" o").unwrap();
    assert_eq!(a.radix, Some(8));
    let a = parse_args(" r /\\d/").unwrap();
    assert!(a.include_match && a.pattern.is_some());
    assert!(parse_args("").is_ok());
}

#[test]
fn flags_that_cannot_go_together_or_no_flag_at_all_are_invalid() {
    for arg in [" nx", " xo", "n", " q", " n //", " n /", " /"] {
        assert!(parse_args(arg).is_err(), "{arg:?}");
    }
}

#[test]
fn a_number_is_the_first_run_of_digits_of_the_base_with_a_sign() {
    assert_eq!(number_of("a12b", 10), Some(12));
    assert_eq!(number_of("a-12b", 10), Some(-12));
    assert_eq!(number_of("none", 10), None);
    assert_eq!(number_of("0xff", 16), Some(255));
    assert_eq!(number_of("ff", 16), Some(255));
    assert_eq!(number_of("-0x1", 16), Some(-1));
    assert_eq!(number_of("0755", 8), Some(493));
    assert_eq!(number_of("789", 8), Some(7));
}

#[test]
fn strings_go_by_utf_16_units() {
    // A character past the BMP is two units in the high-surrogate range, below a character just above it.
    assert_eq!(utf16_cmp("\u{1F600}", "\u{FF5E}"), Ordering::Less);
    assert_eq!(utf16_cmp("a", "b"), Ordering::Less);
}

fn sorted(text: &str, command: &str) -> String {
    let mut ed = Ed::new(text);
    ed.keys(&format!(":{command}<CR>"));
    ed.text()
}

#[test]
fn a_range_of_one_line_or_none_given_is_the_whole_text_or_nothing() {
    assert_eq!(sorted("b\na", "sor"), "a\nb");
    assert_eq!(sorted("c\nb\na", "2sor"), "c\nb\na", "one line is nothing to sort");
    assert_eq!(sorted("c\nb\na", "1,2sor"), "b\nc\na");
}

#[test]
fn text_lines_go_before_the_numbered_ones_and_after_when_reversed() {
    assert_eq!(sorted("2\na\n1\nb", "sor n"), "a\nb\n1\n2");
    assert_eq!(sorted("2\na\n1\nb", "sor! n"), "2\n1\na\nb");
}

#[test]
fn a_sort_by_a_pattern_orders_by_what_follows_or_what_it_matches() {
    assert_eq!(sorted("a3\nb1\nc2", "sor r /\\d/"), "b1\nc2\na3");
    assert_eq!(sorted("a-z\nb-y\nc-x", "sor /-/"), "c-x\nb-y\na-z");
}

proptest! {
    // A plain sort puts the same lines in order; and sorting again changes nothing.
    #[test]
    fn a_sort_is_a_permutation_in_order_and_idempotent(lines in proptest::collection::vec("[a-cA-C0-9 ]{0,4}", 2..8)) {
        let text = lines.join("\n");
        let once = sorted(&text, "sor");
        let mut got: Vec<&str> = once.split('\n').collect();
        let mut want: Vec<&str> = lines.iter().map(String::as_str).collect();
        want.sort_unstable_by(|a, b| utf16_cmp(a, b));
        prop_assert_eq!(&got, &want);
        got.sort_unstable();
        want.sort_unstable();
        prop_assert_eq!(got, want);
        prop_assert_eq!(sorted(&once, "sor"), once);
    }

    // `u` leaves each line once, and a numeric sort orders by number.
    #[test]
    fn a_unique_sort_has_no_equal_neighbours_and_a_numeric_one_is_in_order(nums in proptest::collection::vec(0u32..50, 2..8)) {
        let text = nums.iter().map(u32::to_string).collect::<Vec<_>>().join("\n");
        let unique = sorted(&text, "sor u");
        let u: Vec<&str> = unique.split('\n').collect();
        prop_assert!(u.windows(2).all(|w| w[0] != w[1]));
        let numeric = sorted(&text, "sor n");
        let n: Vec<u32> = numeric.split('\n').map(|l| l.parse().unwrap()).collect();
        prop_assert!(n.windows(2).all(|w| w[0] <= w[1]));
    }
}
