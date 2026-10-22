use super::*;
use crate::code::vim::tests::Ed;

#[test]
fn only_typed_keys_are_recorded_and_the_q_that_ends_it_is_not() {
    let mut ed = Ed::new("|abc");
    ed.keys("qaxlq");
    assert_eq!(ed.sh.regs.get(Some('a')).unwrap().text, "xl");
    assert!(ed.sh.macros.recording.is_none());
    ed.keys("x");
    assert_eq!(ed.sh.regs.get(Some('a')).unwrap().text, "xl", "nothing is recorded after");
}

#[test]
fn keys_are_recorded_as_notation_so_they_play_back_the_same() {
    let mut ed = Ed::new("|a b");
    ed.keys("qq:s/a/x/<CR>A<lt>b<Esc><C-a>q");
    assert_eq!(ed.sh.regs.get(Some('q')).unwrap().text, ":s/a/x/<CR>A<lt>b<Esc><C-a>");
    let text = ed.text();
    assert_eq!(text, "x b<b");
}

#[test]
fn what_a_mapping_types_and_what_a_macro_plays_are_not_recorded() {
    let mut ed = Ed::new("|abcdef");
    ed.keys(":nmap x dl<CR>");
    ed.sh.regs.set_macro('m', "l");
    ed.keys("qaxxq");
    assert_eq!(ed.sh.regs.get(Some('a')).unwrap().text, "xx", "the keys typed, not `dl`");
    ed.keys("qb@mq");
    assert_eq!(ed.sh.regs.get(Some('b')).unwrap().text, "@m");
}

#[test]
fn a_macro_runs_a_bounded_number_of_keys() {
    let mut ed = Ed::new("|a");
    // A register of a hundred thousand keys and more is stopped, whatever it does.
    ed.sh.regs.set_macro('m', &"l".repeat(crate::code::vim::macros::MOST_KEYS + 10));
    ed.keys("@m");
    assert_eq!(ed.text(), "a");
    assert_eq!(ed.sh.macros.depth, 0);
}

#[test]
fn a_mapping_in_the_midst_of_a_macro_is_still_applied() {
    let mut ed = Ed::new("|abc");
    ed.keys(":nmap Q x<CR>");
    ed.sh.regs.set_macro('m', "Q");
    ed.keys("@m");
    assert_eq!(ed.text(), "bc");
}

#[test]
fn a_mapping_met_again_while_it_is_expanded_is_dropped() {
    // The library ends the recursion by not running the mapping that is running, and
    // the key that would have is gone with it.
    let mut ed = Ed::new("|abc");
    ed.keys(":nmap x xx<CR>");
    ed.keys("x");
    assert_eq!(ed.text(), "abc");
    ed.keys(":nmap y dly<CR>y");
    assert_eq!(ed.text(), "bc", "what comes before it in the mapping is done");
}

#[test]
fn noremap_keys_are_not_mapped_even_when_a_mapping_for_them_exists() {
    let mut ed = Ed::new("|abc");
    ed.keys(":nmap y x<CR>:nnoremap z y<CR>");
    ed.keys("z");
    assert_eq!(ed.text(), "abc", "z types the built-in y, which yanks");
    ed.keys("y");
    assert_eq!(ed.text(), "abc", "y types x by the mapping: waiting on an operator target");
}

#[test]
fn an_insert_mapping_that_is_half_typed_and_then_left_with_escape_stands() {
    let mut ed = Ed::new("|a");
    ed.keys(":imap jk <lt>Esc><CR>ij<Esc>");
    assert_eq!(ed.text(), "ja");
    assert_eq!(ed.mode(), Mode::Normal);
}

#[test]
fn replace_mode_gives_back_what_a_mapping_overwrote() {
    let mut ed = Ed::new("|abcd");
    ed.keys(":imap jk <lt>Esc><CR>");
    ed.keys("Rjk");
    assert_eq!(ed.text(), "abcd");
    assert_eq!(ed.mode(), Mode::Normal);
}
