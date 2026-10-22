use crate::code::vim::tests::Ed;

fn said(commands: &str) -> Option<String> {
    let mut ed = Ed::new("|a");
    ed.keys(commands);
    ed.vim.message().map(str::to_owned)
}

#[test]
fn a_name_with_a_question_mark_or_no_value_shows_the_option() {
    assert_eq!(said(":set pcre?<CR>").as_deref(), Some(" pcre"));
    assert_eq!(said(":set tw?<CR>").as_deref(), Some("  tw=80"));
    assert_eq!(said(":set tw<CR>").as_deref(), Some("  tw=80"));
    assert_eq!(said(":set ft<CR>").as_deref(), Some("  ft="));
}

#[test]
fn a_question_mark_with_a_value_is_an_error() {
    assert_eq!(said(":set tw?=5<CR>").as_deref(), Some("Trailing characters:  tw?=5"));
}

#[test]
fn a_value_sets_it_and_a_boolean_is_toggled_with_a_bang_or_turned_off_with_no() {
    assert_eq!(said(":set tw=30<CR>:set tw?<CR>").as_deref(), Some("  tw=30"));
    assert_eq!(said(":set pcre!<CR>:set pcre?<CR>").as_deref(), Some(" nopcre"));
    assert_eq!(said(":set pcre!<CR>:set pcre!<CR>:set pcre?<CR>").as_deref(), Some(" pcre"));
    assert_eq!(said(":set nopcre<CR>:set pcre?<CR>").as_deref(), Some(" nopcre"));
}

#[test]
fn an_unknown_option_is_an_error_whether_shown_set_or_turned_off() {
    assert_eq!(said(":set number<CR>").as_deref(), Some("Unknown option: number"));
    assert_eq!(said(":set nonumber<CR>").as_deref(), Some("Unknown option: number"));
    assert_eq!(said(":set wrap=1<CR>").as_deref(), Some("Unknown option: wrap"));
    assert_eq!(said(":set number?<CR>").as_deref(), Some("Unknown option: number"));
}

#[test]
fn only_the_first_word_is_the_option() {
    assert_eq!(said(":set tw=30 pcre<CR>:set tw?<CR>").as_deref(), Some("  tw=30"));
}

#[test]
fn a_value_may_hold_an_equals_sign() {
    assert_eq!(said(":set ft=a=b<CR>:set ft?<CR>").as_deref(), Some("  ft=a=b"));
}

#[test]
fn no_argument_is_the_librarys_complaint_about_a_mapping() {
    assert_eq!(said(":set<CR>").as_deref(), Some("Invalid mapping: set"));
    assert_eq!(said(":setlocal<CR>").as_deref(), Some("Invalid mapping: setlocal"));
}

#[test]
fn a_text_width_of_none_given_by_no_is_ignored() {
    assert_eq!(said(":set tw=30<CR>:set notw<CR>:set tw?<CR>").as_deref(), Some("  tw=30"));
}
