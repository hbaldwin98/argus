//! The ex commands the library has beyond the first few (EDITOR-VIM-13):
//! `:s`, `:g`, `:v`, `:normal`, `:sort`, `:registers`, `:set` and the
//! mappings, and the range and the line they are typed on.

use super::*;

#[test]
fn substitute_replaces_the_first_match_on_the_line_or_all_with_g() {
    table(&[
        ("|aaa", ":s/a/b/<CR>", "|baa"),
        ("|aaa", ":s/a/b/g<CR>", "bb|b"),
        ("|aa\naa", ":s/a/b/<CR>", "|ba\naa"),
        ("|a b", ":s/a/[&]/<CR>", "|[a] b"),
        ("|ab", ":s/(a)(b)/$2$1/<CR>", "|ba"),
        ("|abc", ":s/b//<CR>", "a|c"),
        ("|a", ":s#a#b#<CR>", "|b"),
        ("|a", ":s/a/b<CR>", "|b"),
        ("|a", ":s/a/x\\ry/<CR>", "|x\ny"),
        ("|a", ":s/a/\\&/<CR>", "|&"),
        ("|a", ":s/a/$$/<CR>", "|$"),
    ]);
}

#[test]
fn substitute_goes_by_the_lines_named() {
    table(&[
        ("|a\na\na", ":%s/a/b/<CR>", "b\nb\n|b"),
        ("|a\na\na", ":2,3s/a/b/<CR>", "a\nb\n|b"),
        ("|a\na\na", ":2s/a/b/<CR>", "a\n|b\na"),
        ("|a\na\na\na", ":s/a/b/ 2<CR>", "b\n|b\na\na"),
        ("|a\na\na", "Vj:s/a/b/<CR>", "b\n|b\na"),
    ]);
}

#[test]
fn a_replacement_has_groups_and_a_backslash_one_is_text_unless_the_pattern_is_vims() {
    table(&[
        ("|ab", ":s/(a)/\\1/<CR>", "|\\1b"),
        ("|ab", ":set nopcre<CR>:s/\\(a\\)/\\1\\1/<CR>", "|aab"),
        ("|abc", ":s/(b)(c)/$2$1$3/<CR>", "a|cb$3"),
    ]);
}

#[test]
fn substitute_goes_by_capitals_and_a_slash_i_flag() {
    table(&[
        ("|aA", ":s/A/x/<CR>", "a|x"),
        ("|aA", ":s/a/x/<CR>", "|xA"),
        ("|Aa", ":s/a/x/<CR>", "|xa"),
        ("|aA", ":s/A/x/i<CR>", "|xA"),
    ]);
}

#[test]
fn substitute_without_a_pattern_or_replacement_uses_the_last() {
    table(&[("|a\na", ":s/a/b/<CR>j:s<CR>", "b\n|b"), ("|a b\nc a", ":s/a/x/<CR>j:s//y/<CR>", "x b\nc |y")]);
}

#[test]
fn substitute_says_what_it_did() {
    let mut ed = Ed::new("|aa");
    ed.keys(":s/a/b/g<CR>");
    assert_eq!(ed.vim.message(), Some("Found 2 matches for pattern: /a/im (set nopcre to use Vim regexps)"));
    ed.keys(":s/zz/b/<CR>");
    assert_eq!(ed.vim.message(), Some("No matches for /zz/im (set nopcre to use vim regexps)"));
    ed.keys(":s/(/b/<CR>");
    assert_eq!(ed.vim.message(), Some("Invalid regex: ("));
}

#[test]
fn substitute_with_no_earlier_one_has_nothing_to_repeat() {
    let mut ed = Ed::new("|aa");
    ed.keys(":s<CR>");
    assert_eq!(ed.vim.message(), Some("No previous substitute regular expression"));
}

#[test]
fn a_confirming_substitute_asks_about_each_match() {
    let mut ed = Ed::new("|aaa");
    ed.keys(":s/a/x/gc<CR>");
    assert_eq!(ed.mode(), Mode::Confirm);
    assert_eq!(ed.vim.confirming(), Some((0, 1)));
    ed.keys("y");
    assert_eq!(ed.text(), "xaa");
    assert_eq!(ed.vim.confirming(), Some((1, 2)));
    ed.keys("n");
    assert_eq!(ed.vim.confirming(), Some((2, 3)));
    ed.keys("y");
    assert_eq!(ed.mode(), Mode::Normal);
    assert_eq!(ed.shows(), "xa|x");
    assert_eq!(ed.vim.message(), Some("Found 3 matches for pattern: /a/im (set nopcre to use Vim regexps)"));
}

#[test]
fn a_confirming_substitute_may_replace_all_the_rest_or_last_or_quit() {
    table(&[
        ("|aaa", ":s/a/x/gc<CR>na", "ax|x"),
        ("|aaa", ":s/a/x/gc<CR>l", "|xaa"),
        ("|aaa", ":s/a/x/gc<CR>q", "|aaa"),
        ("|aaa", ":s/a/x/gc<CR><Esc>", "|aaa"),
        ("|aaa", ":s/a/x/gc<CR>zy", "x|aa"),
    ]);
}

#[test]
fn a_confirming_substitute_that_replaces_with_longer_text_keeps_its_place() {
    table(&[("|aaa", ":s/a/xyz/gc<CR>yyy", "xyzxyz|xyz")]);
}

#[test]
fn a_substitute_may_join_lines_with_a_pattern_that_has_a_line_break() {
    table(&[("|a\nb", ":%s/\\n/-/<CR>", "a|-b")]);
}

#[test]
fn global_runs_a_command_on_each_line_with_a_match() {
    table(&[
        ("|a\nb\na", ":g/a/d<CR>", "|b"),
        ("|a\na\nb\na", ":g/a/d<CR>", "|b"),
        ("|a\nb\nc", ":g/b/d<CR>", "a\n|c"),
        ("|a1\nb\na2", ":g/a/s/\\d/x/<CR>", "ax\nb\na|x"),
        ("|a\nb\na", ":g/a/normal Ax<CR>", "ax\nb\na|x"),
        ("|a\nb\na", ":g/a/+1d<CR>", "|a"),
    ]);
}

#[test]
fn vglobal_and_bang_run_it_on_the_lines_without_a_match() {
    table(&[("|a\nb\na", ":v/a/d<CR>", "a\n|a"), ("|a\nb\na", ":g!/a/d<CR>", "a\n|a"), ("|a\nb\nc", ":v/b/d<CR>", "|b")]);
}

#[test]
fn global_goes_by_the_range_and_shows_the_lines_with_no_command() {
    table(&[("|a\na\na", ":2,3g/a/d<CR>", "|a"), ("|a\na\na", ":2g/a/d<CR>", "a\n|a")]);
    let mut ed = Ed::new("|ab\ncd\nab");
    ed.keys(":g/a<CR>");
    assert_eq!(ed.vim.message(), Some("ab\nab"));
    ed.keys(":g<CR>");
    assert_eq!(ed.vim.message(), Some("Regular Expression missing from global"));
}

#[test]
fn global_goes_on_past_a_line_its_command_has_nothing_to_do_on() {
    // The library's `:s` ends the whole run at the first line it finds no match on.
    table(&[("|a1\na\na2", ":g/a/s/\\d/x/<CR>", "ax\na\na|x")]);
}

#[test]
fn normal_types_keys_on_each_line() {
    table(&[
        ("|a\nb", ":%norm Ax<CR>", "ax\nb|x"),
        ("|a\nb\nc", ":2,3norm Ix<CR>", "a\nxb\nx|c"),
        ("|a b", ":norm wD<CR>", "a| "),
        ("|a", ":norm! Ax<CR>", "a|x"),
        ("|a\nb", ":norm ix<CR>", "x|a\nb"),
    ]);
    let mut ed = Ed::new("|a");
    ed.keys(":norm<CR>");
    assert_eq!(ed.vim.message(), Some("Argument is required."));
}

#[test]
fn normal_with_a_bang_leaves_mappings_out() {
    let mut ed = Ed::new("|abc");
    ed.keys(":nmap x dd<CR>");
    ed.keys(":norm! x<CR>");
    assert_eq!(ed.text(), "bc");
    ed.keys(":norm x<CR>");
    assert_eq!(ed.text(), "");
}

#[test]
fn sort_puts_the_lines_in_order() {
    table(&[
        ("|c\na\nb", ":sor<CR>", "|a\nb\nc"),
        ("|c\na\nb", ":sor!<CR>", "|c\nb\na"),
        ("|10\n9\n100", ":sor n<CR>", "|9\n10\n100"),
        ("|b\na\nb", ":sor u<CR>", "|a\nb"),
        ("|b\nA\nc", ":sor i<CR>", "|A\nb\nc"),
        ("|c\nb\na\nd", ":2,3sor<CR>", "|c\na\nb\nd"),
        ("|b\n2\na\n1", ":sor n<CR>", "|b\na\n1\n2"),
        ("|0xff\n0x2\n0x10", ":sor x<CR>", "|0x2\n0x10\n0xff"),
        ("|a3\nb1\nc2", ":sor r /\\d/<CR>", "|b1\nc2\na3"),
    ]);
    let mut ed = Ed::new("|b\na");
    ed.keys(":sor x y<CR>");
    assert_eq!(ed.vim.message(), Some("Invalid arguments:  x y"));
    assert_eq!(ed.text(), "b\na");
}

#[test]
fn registers_lists_what_they_hold() {
    let mut ed = Ed::new("|abc\nd");
    ed.keys("yyjdd\"ayl:reg<CR>");
    let say = ed.vim.message().unwrap().to_owned();
    assert!(say.starts_with("----------Registers----------\n\n"));
    assert!(say.contains("\"0    abc\n"), "{say}");
    assert!(say.contains("\"1    d\n"), "{say}");
    assert!(say.contains("\"a    a"), "{say}");
    ed.keys(":reg a<CR>");
    let say = ed.vim.message().unwrap();
    assert!(say.contains("\"a    a") && !say.contains("\"0"), "{say}");
}

#[test]
fn a_listing_leaves_the_colon_register_as_it_was() {
    let mut ed = Ed::new("|a");
    ed.keys(":s/a/b/<CR>:reg<CR>");
    assert_eq!(ed.sh.regs.get(Some(':')).unwrap().text, "s/a/b/");
}

#[test]
fn set_reads_and_writes_the_options_there_are() {
    let mut ed = Ed::new("|a");
    ed.keys(":set tw?<CR>");
    assert_eq!(ed.vim.message(), Some("  tw=80"));
    ed.keys(":set tw=20<CR>:set textwidth?<CR>");
    assert_eq!(ed.vim.message(), Some("  textwidth=20"));
    ed.keys(":set pcre?<CR>");
    assert_eq!(ed.vim.message(), Some(" pcre"));
    ed.keys(":set nopcre<CR>:set pcre?<CR>");
    assert_eq!(ed.vim.message(), Some(" nopcre"));
    ed.keys(":set pcre!<CR>:set pcre?<CR>");
    assert_eq!(ed.vim.message(), Some(" pcre"));
    ed.keys(":set tw<CR>");
    assert_eq!(ed.vim.message(), Some("  tw=20"));
    ed.keys(":set tw=abc<CR>");
    assert_eq!(ed.vim.message(), Some("Invalid argument: tw=abc"));
    ed.keys(":set pcre=x<CR>");
    assert_eq!(ed.vim.message(), Some("Invalid argument: pcre=x"));
    ed.keys(":set<CR>");
    assert_eq!(ed.vim.message(), Some("Invalid mapping: set"));
    ed.keys(":set insertModeEscKeysTimeout=500<CR>:set insertModeEscKeysTimeout?<CR>");
    assert_eq!(ed.vim.message(), Some("  insertModeEscKeysTimeout=500"));
    ed.keys(":set ft=rust<CR>:set ft?<CR>");
    assert_eq!(ed.vim.message(), Some("  ft=rust"));
    ed.keys(":set lmap=ab<CR>:set lmap?<CR>");
    assert_eq!(ed.vim.message(), Some("  lmap=ab"));
}

#[test]
fn the_text_width_and_the_file_type_are_one_editors_but_pcre_is_everyones() {
    let mut one = Ed::new("|a");
    one.keys(":set tw=20<CR>:set nopcre<CR>");
    let mut two = Ed::with_shared("|b", std::mem::take(&mut one.sh));
    two.keys(":set tw?<CR>");
    assert_eq!(two.vim.message(), Some("  tw=80"));
    two.keys(":set pcre?<CR>");
    assert_eq!(two.vim.message(), Some(" nopcre"));
}

#[test]
fn setlocal_and_setglobal_choose_the_copy() {
    let mut one = Ed::new("|a");
    one.keys(":setlocal nopcre<CR>");
    let mut two = Ed::with_shared("|b", std::mem::take(&mut one.sh));
    two.keys(":set pcre?<CR>");
    assert_eq!(two.vim.message(), Some(" pcre"), "the other editor's copy is untouched");
    one.sh = std::mem::take(&mut two.sh);
    one.keys(":set pcre?<CR>");
    assert_eq!(one.vim.message(), Some(" nopcre"));
    one.keys(":setglobal nopcre<CR>:setglobal pcre?<CR>");
    assert_eq!(one.vim.message(), Some(" nopcre"));
}

#[test]
fn a_count_before_the_colon_is_a_range_and_a_selection_is_the_marks() {
    let mut ed = Ed::new("|a\nb\nc");
    ed.keys("3:");
    assert_eq!(ed.vim.command_line(), Some(".,.+2"));
    ed.keys("d<CR>");
    assert_eq!(ed.text(), "");
    let mut ed = Ed::new("|a\nb\nc");
    ed.keys("Vj:");
    assert_eq!(ed.vim.command_line(), Some("'<,'>"));
}

#[test]
fn the_range_takes_marks_patterns_and_offsets() {
    table(&[
        ("|a\nb\nc\nd", "jmajj:'a,.d<CR>", "|a"),
        ("|a\nb\nc\nd", ":/c/d<CR>", "a\nb\n|d"),
        ("|a\nb\nc\nd", ":/b/,/c/d<CR>", "a\n|d"),
        ("a\nb\nc\n|d", ":?b?d<CR>", "a\n|c\nd"),
        ("|a\nb\nc\nd", ":.+1,$-1d<CR>", "a\n|d"),
        ("|a\nb\nc\nd", ":2,3d<CR>", "a\n|d"),
        ("|a\nb\nc\nd", "Vj<Esc>G:*d<CR>", "|c\nd"),
    ]);
}

#[test]
fn a_mark_or_pattern_that_is_not_there_is_an_error() {
    let mut ed = Ed::new("|a\nb");
    ed.keys(":'z,.d<CR>");
    assert_eq!(ed.vim.message(), Some("Mark not set"));
    ed.keys(":/zzz/d<CR>");
    assert_eq!(ed.vim.message(), Some("Pattern not found/zzz/"));
    assert_eq!(ed.text(), "a\nb");
}

#[test]
fn an_empty_line_is_an_error_and_a_few_commands_say_little() {
    let mut ed = Ed::new("|a");
    ed.keys(":<CR>");
    assert_eq!(ed.vim.message(), Some("Not an editor command \":\""));
    ed.keys(":version<CR>");
    assert_eq!(ed.vim.message(), Some("Codemirror-vim version: <DEV>"));
    ed.keys(":colorscheme x<CR>");
    assert_eq!(ed.vim.message(), None);
}

#[test]
fn a_command_name_may_be_shortened_as_far_as_the_library_allows() {
    for (typed, ok) in [
        ("wri", true),
        ("writ", true),
        ("sub", true),
        ("subst", true),
        ("de", true),
        ("dele", true),
        ("u", true),
        ("un", true),
        ("wq", false),
        ("quit", false),
        ("sor", true),
        ("so", false),
    ] {
        let mut ed = Ed::new("|a\nb");
        ed.keys(&format!(":{typed}<CR>"));
        let unknown = ed.vim.message().is_some_and(|m| m.starts_with("Not an editor command"));
        assert_eq!(!unknown, ok, "{typed}");
    }
}

#[test]
fn map_makes_keys_stand_for_other_keys() {
    table(&[
        ("|a\nb", ":nmap x dd<CR>x", "|b"),
        ("|a\nb\nc", ":nmap Q dd<CR>2Q", "|c"),
        ("|abc", ":map x l<CR>x", "a|bc"),
        ("|abc", ":nnoremap x l<CR>x", "a|bc"),
        ("|abc", ":nmap a b<CR>:nmap b x<CR>$a", "a|b"),
    ]);
}

#[test]
fn noremap_does_not_map_what_it_types() {
    table(&[
        ("a|bc", ":nmap l x<CR>:nnoremap h l<CR>h", "ab|c"),
        ("a|bc", ":nmap l x<CR>:nmap h l<CR>h", "a|c"),
    ]);
}

#[test]
fn a_mapping_to_itself_ends() {
    table(&[("|abc", ":nmap x x<CR>x", "|abc"), ("|abc", ":nmap x y<CR>:nmap y x<CR>x", "|abc")]);
}

#[test]
fn a_mapping_has_a_mode() {
    table(&[
        ("|abc", ":vmap x d<CR>x", "|bc"),
        ("|abc", ":vmap l d<CR>lvl", "a|c"),
        ("|ab", ":imap jk <lt>Esc><CR>ijk", "|ab"),
    ]);
}

#[test]
fn an_insert_mapping_takes_back_the_keys_it_typed() {
    table(&[
        ("|a", ":imap jk <lt>Esc><CR>ixjk", "|xa"),
        ("|a", ":imap jk <lt>Esc><CR>ixjx<Esc>", "xj|xa"),
        ("|a", ":inoremap jk <lt>Esc><CR>ijk", "|a"),
    ]);
}

#[test]
fn an_insert_mapping_half_typed_stands_after_the_timeout() {
    let mut ed = Ed::new("|a");
    ed.keys(":imap jk <lt>Esc><CR>ij");
    ed.vim.timeout();
    ed.keys("k<Esc>");
    assert_eq!(ed.text(), "jka");
}

#[test]
fn a_mapping_with_no_mode_is_not_for_insert_mode() {
    table(&[("|a", ":map j x<CR>ijj<Esc>", "j|ja")]);
}

#[test]
fn unmap_removes_a_mapping_with_no_mode_only() {
    table(&[("|abc", ":map x l<CR>:unmap x<CR>x", "|bc"), ("|abc", ":nmap x l<CR>:unmap x<CR>x", "a|bc")]);
    let mut ed = Ed::new("|a");
    ed.keys(":unmap z<CR>");
    assert_eq!(ed.vim.message(), Some("No such mapping: unmap z"));
}

#[test]
fn mapclear_removes_them_all_or_one_modes() {
    table(&[
        ("|abc", ":nmap x l<CR>:mapclear<CR>x", "|bc"),
        ("|abc", ":nmap x l<CR>:nmapclear<CR>x", "|bc"),
        ("|abc", ":vmap x d<CR>:nmap x l<CR>:vmapclear<CR>x", "a|bc"),
    ]);
}

#[test]
fn a_map_needs_two_keys() {
    let mut ed = Ed::new("|a");
    ed.keys(":map x<CR>");
    assert_eq!(ed.vim.message(), Some("Invalid mapping: map x"));
}

#[test]
fn a_command_may_be_mapped_to_another_command_or_to_keys() {
    table(&[("|a\nb", ":map :foo :d<CR>:foo<CR>", "|b"), ("|abc", ":map :go l<CR>:go<CR>", "a|bc")]);
}
