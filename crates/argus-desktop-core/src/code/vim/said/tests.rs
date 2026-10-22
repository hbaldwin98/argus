use super::*;
use crate::code::vim::key::parse;

fn typed(vim: &mut Vim, sh: &mut Shared, keys: &str) {
    for key in parse(keys) {
        vim.key(sh, key);
    }
}

#[test]
fn the_mode_is_named_as_vim_does() {
    let (mut vim, mut sh) = (Vim::new("abc\n"), Shared::default());
    assert_eq!(said(&vim, &sh).mode, "NORMAL");
    typed(&mut vim, &mut sh, "i");
    assert_eq!(said(&vim, &sh).mode, "INSERT");
    typed(&mut vim, &mut sh, "<Esc>v");
    assert_eq!(said(&vim, &sh).mode, "VISUAL");
    typed(&mut vim, &mut sh, "<Esc>V");
    assert_eq!(said(&vim, &sh).mode, "VISUAL LINE");
    typed(&mut vim, &mut sh, "<Esc><C-v>");
    assert_eq!(said(&vim, &sh).mode, "VISUAL BLOCK");
    typed(&mut vim, &mut sh, "<Esc>R");
    assert_eq!(said(&vim, &sh).mode, "REPLACE");
}

#[test]
fn the_keys_of_a_command_not_finished_are_shown_until_it_is() {
    let (mut vim, mut sh) = (Vim::new("a b c d\n"), Shared::default());
    typed(&mut vim, &mut sh, "2d");
    assert_eq!(said(&vim, &sh).pending, "2d");
    typed(&mut vim, &mut sh, "w");
    assert_eq!(said(&vim, &sh).pending, "", "a command run is not pending");
    typed(&mut vim, &mut sh, "\"a");
    assert_eq!(said(&vim, &sh).pending, "\"a");
}

#[test]
fn the_line_being_typed_is_shown_with_its_colon_or_slash() {
    let (mut vim, mut sh) = (Vim::new("abc\n"), Shared::default());
    typed(&mut vim, &mut sh, ":s/a");
    assert_eq!(said(&vim, &sh).line, ":s/a");
    assert_eq!(said(&vim, &sh).mode, "COMMAND");
    typed(&mut vim, &mut sh, "<Esc>/b");
    assert_eq!(said(&vim, &sh).line, "/b");
    assert_eq!(said(&vim, &sh).mode, "SEARCH");
}

#[test]
fn what_the_last_key_left_is_shown_and_a_recording_says_so() {
    let (mut vim, mut sh) = (Vim::new("abc\n"), Shared::default());
    typed(&mut vim, &mut sh, ":nosuchcommand<CR>");
    assert_eq!(said(&vim, &sh).line, "Not an editor command \":nosuchcommand\"");
    typed(&mut vim, &mut sh, "qq");
    assert_eq!(said(&vim, &sh).line, "recording @q");
    typed(&mut vim, &mut sh, "q");
    assert_eq!(said(&vim, &sh).line, "");
}
