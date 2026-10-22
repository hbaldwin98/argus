//! Registers across commands and across editors (EDITOR-VIM-12, -17, -23).

use super::*;

#[test]
fn a_yank_in_one_editor_is_pasted_in_another() {
    let mut one = Ed::new("|alpha beta");
    one.keys("yw");
    let mut two = Ed::with_regs("|x", std::mem::take(&mut one.regs));
    two.keys("p");
    assert_eq!(two.text(), "xalpha ");
}

#[test]
fn a_named_register_holds_until_it_is_written_again() {
    table(&[
        ("|a\nb", "\"ayyj\"ap", "a\nb\n|a"),
        ("|a\nb", "\"ayyj\"byy\"ap\"bp", "a\nb\na\n|b"),
        ("|a\nb", "\"ayyj\"Ayy\"ap", "a\nb\n|a\nb"),
    ]);
}

#[test]
fn the_black_hole_keeps_nothing() {
    table(&[("|a\nb", "yyj\"_ddp", "a\n|a")]);
}

#[test]
fn a_yank_survives_a_delete_in_register_zero() {
    table(&[("|a\nb", "yyjdd\"0p", "a\n|a")]);
}

#[test]
fn deleted_lines_shift_through_the_numbered_registers() {
    table(&[("|a\nb\nc\nd", "dddd\"2p", "c\n|a\nd")]);
    let mut e = Ed::new("|a\nb\nc");
    e.keys("dddd");
    assert_eq!(e.regs.get(Some('1')).unwrap().text, "b\n");
    assert_eq!(e.regs.get(Some('2')).unwrap().text, "a\n");
}

#[test]
fn a_small_delete_is_the_dash_register_and_not_the_numbered_ones() {
    let mut e = Ed::new("|abc");
    e.keys("x");
    assert_eq!(e.regs.get(Some('-')).unwrap().text, "a");
    assert_eq!(e.regs.get(Some('1')), None);
}

#[test]
fn the_clipboard_is_the_hosts_and_the_unnamed_register_never_reaches_it() {
    let mut e = Ed::new("|abc");
    e.keys("yl");
    assert_eq!(e.regs.take_clipboard_write(), None, "a plain yank leaves it alone");
    e.keys("\"+yy");
    assert_eq!(e.regs.take_clipboard_write(), Some("abc\n".into()));
    e.regs.offer_clipboard(Some("host".into()));
    e.keys("\"+p");
    assert_eq!(e.text(), "ahostbc");
}

#[test]
fn a_change_fills_the_registers_like_a_delete() {
    let mut e = Ed::new("|abc def");
    e.keys("cwX<Esc>");
    assert_eq!(e.regs.get(None).unwrap().text, "abc");
    assert_eq!(e.regs.get(Some('-')).unwrap().text, "abc");
}

#[test]
fn a_delete_across_lines_shifts_even_when_it_is_not_whole_lines() {
    let mut e = Ed::new("|ab\ncd");
    e.keys("vjd");
    assert_eq!(e.regs.get(Some('1')).unwrap().text, "ab\nc");
    assert_eq!(e.regs.get(Some('-')), None);
}
