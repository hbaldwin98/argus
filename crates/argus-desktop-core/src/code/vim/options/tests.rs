use super::*;

fn set(o: &mut Options, l: &mut Locals, name: &str, given: Given, scope: Scope) -> Result<(), String> {
    o.set(name, given, scope, l)
}

fn text(s: &str) -> Given {
    Given::Text(s.into())
}

#[test]
fn only_five_options_are_known_by_their_names_and_short_names() {
    let (o, l) = (Options::default(), Locals::default());
    for name in ["filetype", "ft", "textwidth", "tw", "langmap", "lmap", "pcre", "insertModeEscKeysTimeout"] {
        assert!(o.get(name, Scope::Both, &l).is_ok(), "{name}");
    }
    for name in ["ignorecase", "number", "tabstop", "hlsearch", ""] {
        assert_eq!(o.get(name, Scope::Both, &l), Err(format!("Unknown option: {name}")), "{name}");
    }
}

#[test]
fn the_defaults_are_the_librarys() {
    let (o, l) = (Options::default(), Locals::default());
    assert_eq!(o.get("pcre", Scope::Both, &l), Ok(Some(Value::Bool(true))));
    assert_eq!(o.get("textwidth", Scope::Both, &l), Ok(Some(Value::Number(80.0))));
    assert_eq!(o.get("insertModeEscKeysTimeout", Scope::Both, &l), Ok(Some(Value::Number(200.0))));
    assert_eq!(o.get("langmap", Scope::Both, &l), Ok(Some(Value::Text(String::new()))));
    assert_eq!(Options::text_width(&l), 80);
}

#[test]
fn a_boolean_is_set_on_by_its_name_off_by_no_and_not_by_a_value() {
    let (mut o, mut l) = (Options::default(), Locals::default());
    set(&mut o, &mut l, "pcre", Given::Off, Scope::Both).unwrap();
    assert_eq!(o.get("pcre", Scope::Both, &l), Ok(Some(Value::Bool(false))));
    set(&mut o, &mut l, "pcre", Given::Unset, Scope::Both).unwrap();
    assert_eq!(o.get("pcre", Scope::Both, &l), Ok(Some(Value::Bool(true))));
    assert_eq!(set(&mut o, &mut l, "pcre", text("maybe"), Scope::Both), Err("Invalid argument: pcre=maybe".into()));
}

#[test]
fn a_number_takes_a_number_and_a_text_width_under_two_is_ignored() {
    let (mut o, mut l) = (Options::default(), Locals::default());
    set(&mut o, &mut l, "tw", text("30"), Scope::Both).unwrap();
    assert_eq!(Options::text_width(&l), 30);
    set(&mut o, &mut l, "tw", text("1"), Scope::Both).unwrap();
    assert_eq!(Options::text_width(&l), 30, "a width of one or less is no width");
    set(&mut o, &mut l, "tw", text("40.6"), Scope::Both).unwrap();
    assert_eq!(Options::text_width(&l), 41, "rounded");
    assert_eq!(set(&mut o, &mut l, "tw", text("wide"), Scope::Both), Err("Invalid argument: tw=wide".into()));
    assert!(set(&mut o, &mut l, "insertModeEscKeysTimeout", text("x"), Scope::Both).is_err());
}

#[test]
fn the_text_width_and_the_file_type_are_an_editors_own() {
    let (mut o, mut l) = (Options::default(), Locals::default());
    set(&mut o, &mut l, "tw", text("30"), Scope::Global).unwrap();
    assert_eq!(l.textwidth, None, "there is no global one to set");
    assert_eq!(o.get("tw", Scope::Global, &l), Ok(None));
    set(&mut o, &mut l, "ft", text("rust"), Scope::Local).unwrap();
    assert_eq!(o.get("ft", Scope::Local, &l), Ok(Some(Value::Text("rust".into()))));
}

#[test]
fn pcre_and_the_timeout_have_a_shared_and_a_local_copy() {
    let (mut o, mut l) = (Options::default(), Locals::default());
    set(&mut o, &mut l, "pcre", Given::Off, Scope::Local).unwrap();
    assert!(o.pcre, "the shared one is left");
    assert_eq!(o.get("pcre", Scope::Both, &l), Ok(Some(Value::Bool(false))), "the editor's own wins");
    assert_eq!(o.get("pcre", Scope::Global, &l), Ok(Some(Value::Bool(true))));
    set(&mut o, &mut l, "pcre", Given::Off, Scope::Global).unwrap();
    assert!(!o.pcre);
    assert_eq!(o.get("pcre", Scope::Local, &l), Ok(Some(Value::Bool(false))));
}

#[test]
fn langmap_is_read_as_pairs_and_as_from_to_lists() {
    let m = parse_langmap("aA,bB");
    assert_eq!((m.get(&'a'), m.get(&'b'), m.len()), (Some(&'A'), Some(&'B'), 2));
    let m = parse_langmap("abc;ABC");
    assert_eq!((m.get(&'a'), m.get(&'b'), m.get(&'c')), (Some(&'A'), Some(&'B'), Some(&'C')));
    let m = parse_langmap("aA,xy;zw");
    assert_eq!((m.get(&'a'), m.get(&'x'), m.get(&'y')), (Some(&'A'), Some(&'z'), Some(&'w')));
}

#[test]
fn a_malformed_part_of_a_langmap_is_skipped() {
    assert!(parse_langmap("abc").is_empty(), "an odd list of pairs");
    assert!(parse_langmap("ab;c").is_empty(), "lists of two lengths");
    let m = parse_langmap("abc,xX");
    assert_eq!((m.len(), m.get(&'x')), (1, Some(&'X')), "the rest stands");
}

#[test]
fn a_backslash_in_a_langmap_takes_the_character_after_it_as_itself() {
    let m = parse_langmap("\\,x");
    assert_eq!(m.get(&','), Some(&'x'), "an escaped comma is no separator");
    let m = parse_langmap("\\;a;bc");
    assert_eq!((m.get(&';'), m.get(&'a')), (Some(&'b'), Some(&'c')));
}

#[test]
fn setting_the_langmap_reads_it() {
    let (mut o, mut l) = (Options::default(), Locals::default());
    set(&mut o, &mut l, "lmap", text("jk"), Scope::Both).unwrap();
    assert_eq!(o.keymap.get(&'j'), Some(&'k'));
    set(&mut o, &mut l, "lmap", text(""), Scope::Both).unwrap();
    assert!(o.keymap.is_empty());
}
