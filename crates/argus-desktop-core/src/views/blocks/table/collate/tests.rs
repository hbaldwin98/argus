use std::cmp::Ordering::{Equal, Greater, Less};

use super::*;

#[test]
fn letters_order_by_base_letter_before_case_and_lowercase_comes_first() {
    // What `"a".localeCompare("B")` and `"a".localeCompare("A")` answer in a browser.
    assert_eq!(locale_compare("a", "B"), Less);
    assert_eq!(locale_compare("B", "a"), Greater);
    assert_eq!(locale_compare("a", "A"), Less);
    assert_eq!(locale_compare("Zebra", "apple"), Greater);
    assert_eq!(locale_compare("same", "same"), Equal);
}

#[test]
fn an_accented_letter_sorts_beside_its_base_letter_not_after_z() {
    assert_eq!(locale_compare("ä", "z"), Less);
    assert_eq!(locale_compare("résumé", "resumf"), Less, "the base letters decide before the accents");
    assert_eq!(locale_compare("resume", "résumé"), Less, "and the accents decide when they are all that differs");
    assert_eq!(locale_compare("é", "f"), Less);
}

#[test]
fn punctuation_comes_before_digits_which_come_before_letters() {
    assert_eq!(locale_compare("_x", "x"), Less);
    assert_eq!(locale_compare("-1", "1"), Less);
    assert_eq!(locale_compare("1", "a"), Less);
    assert_eq!(locale_compare("a b", "ab"), Less, "a space is not ignored");
}

#[test]
fn digits_compare_one_at_a_time_not_as_a_number() {
    assert_eq!(locale_compare("10", "9"), Less);
    assert_eq!(locale_compare("item 10", "item 2"), Less);
}

#[test]
fn a_figure_reads_as_javascript_writes_it() {
    assert_eq!(js_string(1.0), "1");
    assert_eq!(js_string(-12.5), "-12.5");
    assert_eq!(js_string(0.1 + 0.2), "0.30000000000000004");
    assert_eq!(js_string(-0.0), "0");
    assert_eq!(js_string(123456789012345680000.0), "123456789012345680000");
    assert_eq!(js_string(1e21), "1e+21");
    assert_eq!(js_string(1.5e300), "1.5e+300");
    assert_eq!(js_string(0.000001), "0.000001");
    assert_eq!(js_string(1e-7), "1e-7");
    assert_eq!(js_string(-2.5e-9), "-2.5e-9");
    assert_eq!(js_string(f64::NAN), "NaN");
    assert_eq!(js_string(f64::INFINITY), "Infinity");
    assert_eq!(js_string(f64::NEG_INFINITY), "-Infinity");
}

mod properties {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        // A sort needs a total order, or `sort_by` may panic or scramble: reversing the
        // arguments reverses the answer, and an order is transitive.
        #[test]
        fn the_order_is_antisymmetric(a in "\\PC{0,8}", b in "\\PC{0,8}") {
            prop_assert_eq!(locale_compare(&a, &b), locale_compare(&b, &a).reverse());
        }

        #[test]
        fn the_order_is_transitive(a in "[a-zA-Zé1_ -]{0,5}", b in "[a-zA-Zé1_ -]{0,5}", c in "[a-zA-Zé1_ -]{0,5}") {
            if locale_compare(&a, &b) != Greater && locale_compare(&b, &c) != Greater {
                prop_assert!(locale_compare(&a, &c) != Greater);
            }
        }

        // A figure written for a comparison reads back as itself, as `String(n)` does.
        #[test]
        fn a_written_figure_reads_back_as_the_same_figure(n in any::<f64>().prop_filter("finite", |n| n.is_finite())) {
            prop_assert_eq!(js_string(n).parse::<f64>().unwrap(), n + 0.0);
        }
    }
}
