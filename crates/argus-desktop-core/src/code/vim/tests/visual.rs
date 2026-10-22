//! Visual and visual-line mode (EDITOR-VIM-5, -9).

use super::*;

#[test]
fn a_selection_is_operated_on() {
    table(&[
        ("|abc def", "ved", "| def"),
        ("|abc def", "vwd", "|ef"),
        ("a|bc", "vld", "|a"),
        ("|abc\ndef", "Vd", "|def"),
        ("|abc\ndef\nghi", "Vjd", "|ghi"),
        ("|abcdef", "v3ld", "|ef"),
        ("|abc", "v$d", "|"),
        ("|abc", "vl~", "|ABc"),
        ("|ABC", "vlu", "|abC"),
        ("|abc", "vlU", "|ABc"),
        ("|abc", "vl>", "    |abc"),
        ("|abc", "vlrx", "|xxc"),
        ("|a\nb\nc", "VjjJ", "a b| c"),
        ("|abc", "vlod", "|c"),
        ("|abc", "vlx", "|c"),
    ]);
}

#[test]
fn changing_a_selection_types_in_its_place() {
    table(&[("|abc", "vlcX<Esc>", "|Xc"), ("|abc\ndef", "VcX<Esc>", "|X\ndef"), ("|abc\ndef", "vCX<Esc>", "|X\ndef")]);
}

#[test]
fn yanking_a_selection_leaves_the_cursor_at_its_start() {
    table(&[
        ("|abc", "vly", "|abc"),
        ("ab|c", "vhy", "a|bc"),
        ("|abc", "vlyP", "a|babc"),
        ("|a\nb", "VjyGp", "a\nb\n|a\nb"),
    ]);
}

#[test]
fn objects_select() {
    table(&[
        ("a|bc def", "viwd", "| def"),
        ("|abc def", "vawd", "|def"),
        ("(a |b)", "vi(d", "(|)"),
        ("(a |b)", "va(d", "|"),
        ("a\nb|\n\nc", "vipd", "|\nc"),
    ]);
}

#[test]
fn escape_and_the_mode_keys_leave_and_switch() {
    table(&[
        ("|abc", "vl<Esc>x", "a|c"),
        ("|abc", "vvl", "a|bc"),
        ("|abc\nd", "vVd", "|d"),
        ("|abc\nd", "Vvd", "|bc\nd"),
        ("|abc def", "vl<Esc>$gvd", "|c def"),
    ]);
}

#[test]
fn the_selection_is_for_the_host_to_draw() {
    let mut ed = Ed::new("|ab\ncd");
    assert_eq!(ed.vim.selection(), None);
    ed.keys("vl");
    assert_eq!((ed.vim.selection(), ed.mode()), (Some((0, 2)), Mode::Visual));
    ed.keys("<Esc>V");
    assert_eq!((ed.vim.selection(), ed.mode()), (Some((0, 3)), Mode::VisualLine));
    ed.keys("<Esc>");
    assert_eq!(ed.vim.selection(), None);
}

#[test]
fn p_over_a_selection_swaps_it_with_the_register() {
    table(&[("|abc def", "yiwwviwp", "abc |abc")]);
    let mut ed = Ed::new("|abc def");
    ed.keys("yiwwviwp");
    assert_eq!(ed.sh.regs.get(None).unwrap().text, "def", "p takes what it replaced");
    let mut ed = Ed::new("|abc def");
    ed.keys("yiwwviwP");
    assert_eq!(ed.sh.regs.get(None).unwrap().text, "abc", "P keeps the register");
}

#[test]
fn a_colon_from_a_selection_opens_the_line() {
    let mut ed = Ed::new("|abc");
    ed.keys("v:");
    assert_eq!((ed.mode(), ed.vim.command_line()), (Mode::Command, Some("'<,'>")));
    ed.keys("w<CR>");
    assert_eq!(ed.effects, vec![Effect::Write]);
    assert_eq!(ed.mode(), Mode::Normal);
}
