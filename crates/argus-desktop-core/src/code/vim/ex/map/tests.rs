use crate::code::vim::maps::{Hit, Mode};
use crate::code::vim::tests::Ed;

fn mapped(commands: &str) -> Ed {
    let mut ed = Ed::new("|a");
    ed.keys(commands);
    ed
}

fn has(ed: &Ed, lhs: &str, mode: Mode) -> Option<(bool, Vec<crate::code::vim::key::Key>)> {
    match ed.sh.maps.lookup(&crate::code::vim::key::parse(lhs), mode) {
        Hit::Full(m) => Some((m.noremap, m.rhs.clone())),
        _ => None,
    }
}

#[test]
fn each_command_makes_a_mapping_for_its_mode_and_remap_or_not() {
    let ed = mapped(":map a b<CR>:nmap c d<CR>:vmap e f<CR>:omap g h<CR>:imap i j<CR>:noremap k l<CR>:nnoremap m n<CR>:vnoremap o p<CR>:onoremap q r<CR>:inoremap s t<CR>");
    for (lhs, mode, noremap) in [
        ("a", Mode::Normal, false),
        ("a", Mode::Visual, false),
        ("a", Mode::OperatorPending, false),
        ("c", Mode::Normal, false),
        ("e", Mode::Visual, false),
        ("g", Mode::OperatorPending, false),
        ("i", Mode::Insert, false),
        ("k", Mode::Normal, true),
        ("m", Mode::Normal, true),
        ("o", Mode::Visual, true),
        ("q", Mode::OperatorPending, true),
        ("s", Mode::Insert, true),
    ] {
        let found = has(&ed, lhs, mode);
        assert_eq!(found.map(|f| f.0), Some(noremap), "{lhs} in {mode:?}");
    }
    for (lhs, mode) in [("a", Mode::Insert), ("c", Mode::Visual), ("e", Mode::Normal), ("i", Mode::Normal), ("s", Mode::Normal)] {
        assert!(has(&ed, lhs, mode).is_none(), "{lhs} in {mode:?}");
    }
}

#[test]
fn the_right_hand_side_is_the_keys_in_notation() {
    let ed = mapped(":nmap a <lt>C-d>x<CR>");
    let (_, rhs) = has(&ed, "a", Mode::Normal).unwrap();
    assert_eq!(rhs, crate::code::vim::key::parse("<C-d>x"));
}

#[test]
fn only_two_words_are_taken_so_a_right_hand_side_has_no_blank() {
    let ed = mapped(":nmap a b c<CR>");
    assert_eq!(has(&ed, "a", Mode::Normal).unwrap().1, crate::code::vim::key::parse("b"));
}

#[test]
fn a_mapping_with_a_missing_side_is_invalid() {
    assert_eq!(mapped(":map<CR>").vim.message(), Some("Invalid mapping: map"));
    assert_eq!(mapped(":nmap a<CR>").vim.message(), Some("Invalid mapping: nmap a"));
}

#[test]
fn a_command_mapped_in_a_mode_is_an_error() {
    // Only the map with no mode takes a command name; `:nmap :a :b` gives the library's own message.
    assert_eq!(mapped(":nmap :a :b<CR>").vim.message(), Some("Mode not supported for ex mappings"));
}

#[test]
fn unmap_names_the_line_it_could_not_find() {
    assert_eq!(mapped(":unmap q<CR>").vim.message(), Some("No such mapping: unmap q"));
    assert_eq!(mapped(":unmap<CR>").vim.message(), Some("No such mapping: unmap"));
}

#[test]
fn each_clear_command_clears_its_own_mode() {
    let mut ed = mapped(":nmap a b<CR>:vmap c d<CR>:imap e f<CR>:omap g h<CR>");
    ed.keys(":imapclear<CR>");
    assert!(has(&ed, "e", Mode::Insert).is_none() && has(&ed, "a", Mode::Normal).is_some());
    ed.keys(":omapclear<CR>");
    assert!(has(&ed, "g", Mode::OperatorPending).is_none() && has(&ed, "c", Mode::Visual).is_some());
    ed.keys(":vmapclear<CR>");
    assert!(has(&ed, "c", Mode::Visual).is_none());
    ed.keys(":nmapclear<CR>");
    assert!(has(&ed, "a", Mode::Normal).is_none());
}
