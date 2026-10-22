use super::*;
use crate::code::vim::key::parse;

fn read(notation: &str) -> Option<Motion> {
    match motion(&parse(notation)) {
        Moved::Done(m) => Some(m),
        _ => None,
    }
}

#[test]
fn a_motion_is_named_by_its_keys() {
    assert_eq!(read("h"), Some(Motion::Left));
    assert_eq!(read("<Left>"), Some(Motion::Left));
    assert_eq!(read("<BS>"), Some(Motion::Left));
    assert_eq!(read(" "), Some(Motion::Right));
    assert_eq!(read("gg"), Some(Motion::FileStart));
    assert_eq!(read("ge"), Some(Motion::WordEndBack { big: false }));
    assert_eq!(read("fx"), Some(Motion::Find(Find::To, 'x')));
    assert_eq!(read("Tx"), Some(Motion::Find(Find::TillBack, 'x')));
    assert_eq!(read("<CR>"), Some(Motion::Plus));
    assert_eq!(read("<End>"), Some(Motion::LineEnd));
}

#[test]
fn half_a_motion_is_awaited_and_extra_keys_are_none() {
    assert!(matches!(motion(&parse("g")), Moved::Need));
    assert!(matches!(motion(&parse("f")), Moved::Need));
    assert!(matches!(motion(&parse("")), Moved::Need));
    assert!(matches!(motion(&parse("hh")), Moved::Bad));
    assert!(matches!(motion(&parse("gq")), Moved::Bad));
    assert!(matches!(motion(&parse("<C-h>")), Moved::Bad));
    assert!(matches!(motion(&parse("<Tab>")), Moved::Bad));
}
