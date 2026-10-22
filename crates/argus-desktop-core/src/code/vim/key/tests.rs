use super::*;

#[test]
fn plain_characters_are_keys_and_named_ones_are_in_angle_brackets() {
    assert_eq!(parse("dw"), vec![Key::ch('d'), Key::ch('w')]);
    assert_eq!(
        parse("i<Esc>:w<CR>"),
        vec![Key::ch('i'), Key::of(Code::Esc), Key::ch(':'), Key::ch('w'), Key::of(Code::Enter)]
    );
    assert_eq!(parse("<C-r>"), vec![Key::ctrl('r')]);
    assert_eq!(parse("<C-R>"), vec![Key::ctrl('r')]);
}

#[test]
fn a_less_than_sign_that_names_nothing_is_the_character() {
    assert_eq!(parse("<<"), vec![Key::ch('<'), Key::ch('<')]);
    assert_eq!(parse("a<b"), vec![Key::ch('a'), Key::ch('<'), Key::ch('b')]);
    assert_eq!(parse("<lt><lt>"), vec![Key::ch('<'), Key::ch('<')]);
}

#[test]
fn escape_is_three_keys() {
    assert!(Key::of(Code::Esc).is_esc());
    assert!(Key::ctrl('[').is_esc());
    assert!(Key::ctrl('c').is_esc());
    assert!(!Key::ch('c').is_esc());
}

#[test]
fn only_an_unmodified_character_types() {
    assert_eq!(Key::ch('é').typed(), Some('é'));
    assert_eq!(Key::ctrl('a').typed(), None);
    assert_eq!(Key::of(Code::Enter).typed(), None);
}
