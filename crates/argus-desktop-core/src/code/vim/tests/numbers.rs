//! `Ctrl-a` and `Ctrl-x` on the numbers the library reads (EDITOR-VIM-21):
//! decimal, `0x` hex, `0b` binary, octal with a leading zero, and a sign.

use super::*;

#[test]
fn hex_keeps_its_prefix_and_the_zeros_that_fill_it() {
    table(&[
        ("|0x0f", "<C-a>", "0x1|0"),
        ("|0xff", "<C-a>", "0x10|0"),
        ("|0x00ff", "<C-a>", "0x010|0"),
        ("|0xFF", "<C-x>", "0xf|e"),
        ("|0x10", "<C-x>", "0x0|f"),
        ("|0x1", "255<C-a>", "0x10|0"),
    ]);
}

#[test]
fn binary_and_octal_are_read_by_their_prefix() {
    table(&[
        ("|0b101", "<C-a>", "0b11|0"),
        ("|0b1", "<C-x>", "0b|0"),
        ("|007", "<C-a>", "01|0"),
        ("|010", "<C-x>", "00|7"),
    ]);
}

#[test]
fn digits_that_are_none_of_the_base_are_ignored_as_javascript_reads_a_number() {
    // `parseInt("09", 8)` is 0: the 9 is no octal digit and ends the number.
    table(&[("|009", "<C-a>", "00|1"), ("|0b102", "<C-a>", "0b01|1"), ("|0b2", "<C-a>", "|0b2")]);
}

#[test]
fn a_minus_before_the_digits_is_a_sign() {
    table(&[
        ("|-5", "<C-a>", "-|4"),
        ("|-1", "2<C-a>", "|1"),
        ("|1", "3<C-x>", "-|2"),
        ("a|-3", "<C-a>", "a-|2"),
        ("|-0x1f", "<C-a>", "-0x1|e"),
    ]);
}

#[test]
fn decimal_numbers_are_read_from_the_cursor_on() {
    table(&[
        ("|x 7 y", "<C-a>", "x |8 y"),
        ("x |7 y 9", "<C-a>", "x |8 y 9"),
        ("x 7| y 9", "<C-a>", "x 7 y 1|0"),
        ("x 7 y 9|", "<C-a>", "x 7 y 1|0"),
        ("a 1 9|", "<C-a>", "a 1 1|0"),
        ("|abc", "<C-a>", "|abc"),
    ]);
}
