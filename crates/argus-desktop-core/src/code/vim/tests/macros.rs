//! Macros and `@` (EDITOR-VIM-9, -17): recording keys into a register, playing
//! it back, and what is and is not remembered of a macro.

use super::*;

#[test]
fn a_recording_is_played_back_with_at() {
    table(&[
        ("|a\nb\nc", "qaIx<Esc>jq@a", "xa\nxb\n|c"),
        ("|a\nb\nc", "qaIx<Esc>jq@a@@", "xa\nxb\n|xc"),
        ("|a\nb\nc\nd", "qaIx<Esc>jq2@a", "xa\nxb\nxc\n|d"),
        ("|a\nb\nc\nd", "qaIx<Esc>jq3@a", "xa\nxb\nxc\n|xd"),
    ]);
}

#[test]
fn a_register_holds_the_keys_typed_as_text() {
    let mut ed = Ed::new("|abc");
    ed.keys("qaihello<Esc>q");
    assert_eq!(ed.sh.regs.get(Some('a')).unwrap().text, "ihello<Esc>");
    ed.keys("qbd2l<C-w>q");
    assert_eq!(ed.sh.regs.get(Some('b')).unwrap().text, "d2l<C-w>");
}

#[test]
fn a_register_that_is_written_is_a_macro() {
    let mut ed = Ed::new("|abc");
    ed.sh.regs.set_macro('q', "x");
    ed.keys("@q@q");
    assert_eq!(ed.text(), "c");
}

#[test]
fn a_macro_is_pasted_as_its_keys_and_yanked_text_can_be_played() {
    let mut ed = Ed::new("|ddx\nline\nline");
    ed.keys("\"ayyjj@a");
    // `ddx` played: dd deletes a line and x a character; the line break is no key.
    assert_eq!(ed.text(), "ddx\nine");
}

#[test]
fn at_at_plays_the_last_register_and_at_colon_the_last_ex_command() {
    table(&[
        ("|aaa\nbbb\naaa", ":s/a/x/<CR>j@:", "xaa\n|bbb\naaa"),
        ("|aaa\naaa\naaa", ":s/a/x/<CR>j@:j@@", "xaa\nxaa\n|xaa"),
    ]);
}

#[test]
fn counts_play_it_that_many_times() {
    table(&[("|abcdef", "qaxq3@a", "|ef"), ("|abcdef", "qaxq@a@@@@", "|ef")]);
}

#[test]
fn playing_does_not_change_what_a_dot_repeats() {
    table(&[("|abcdef", "qaiZ<Esc>qx@a.", "|abcdef")]);
}

#[test]
fn a_macro_may_search_and_run_ex_commands() {
    table(&[
        ("|a x b x c", "qa/x<CR>rYq@a", "a Y b |Y c"),
        ("|aa\nbb\naa", "qa:s/a/c/<CR>jq@a", "ca\nbb\n|aa"),
    ]);
}

#[test]
fn a_macro_that_plays_itself_ends() {
    let mut ed = Ed::new("|abc");
    ed.keys("qa@aq");
    ed.keys("@a");
    assert_eq!(ed.text(), "abc");
    assert_eq!(ed.sh.macros.depth, 0, "every level of it came back out");
}

#[test]
fn a_recording_is_ended_by_q_only_between_commands() {
    let mut ed = Ed::new("|abc def");
    ed.keys("qafqq");
    assert_eq!(ed.sh.regs.get(Some('a')).unwrap().text, "fq", "the q after f is the character looked for");
    assert!(ed.sh.macros.recording.is_none(), "and the last q ended the recording");
}

#[test]
fn one_set_of_registers_serves_every_editor() {
    let mut one = Ed::new("|a");
    one.keys("qaxq");
    let mut two = Ed::with_shared("|bcd", std::mem::take(&mut one.sh));
    two.keys("@a");
    assert_eq!(two.text(), "cd");
}

#[test]
fn recording_a_capital_register_starts_it_afresh() {
    // The library empties the register whatever the case, so `qA` does not append.
    let mut ed = Ed::new("|abcdef");
    ed.keys("qaxqqAlq");
    assert_eq!(ed.sh.regs.get(Some('a')).unwrap().text, "l");
}

#[test]
fn a_macro_played_while_it_is_recorded_plays_itself_down_to_the_depth_limit() {
    // As in the library the register grows as it is typed, so `@a` inside `qa` plays
    // what holds `@a`: it ends at MOST_DEPTH, not at the end of the world.
    let mut ed = Ed::new("ab
b");
    ed.keys("qa:g/b/normal Ax<CR>@a");
    assert_eq!(ed.sh.macros.depth, 0);
    assert_eq!(ed.sh.regs.get(Some('a')).unwrap().text, ":g/b/normal Ax<CR>@a");
    // Two lines get an `x` for each run of the `:g`: the typed one and one per level.
    let runs = ed.text().matches('x').count() / 2;
    let most = crate::code::vim::macros::MOST_DEPTH;
    assert!((most..=most + 1).contains(&runs), "{runs} runs");
}
