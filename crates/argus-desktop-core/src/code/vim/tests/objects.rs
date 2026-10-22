//! Text objects after an operator (EDITOR-VIM-11).

use super::*;

#[test]
fn words() {
    table(&[
        ("foo |bar baz", "diw", "foo | baz"),
        ("foo |bar baz", "daw", "foo |baz"),
        ("foo b|ar", "daw", "fo|o"),
        ("|a b c d", "d2aw", "|c d"),
        ("|a b c d", "d2iw", "|b c d"),
        ("fo|o bar", "ciwX<Esc>", "|X bar"),
        ("foo|.bar", "diw", "foo|bar"),
        ("a  |  b", "diw", "a|b"),
        ("foo.b|ar baz", "diW", "| baz"),
    ]);
}

#[test]
fn quotes() {
    table(&[
        ("say \"he|llo\" now", "di\"", "say \"|\" now"),
        ("say \"he|llo\" now", "da\"", "say |now"),
        ("|say \"hi\"", "di\"", "say \"|\""),
        ("say 'a|b'", "di'", "say '|'"),
        ("say `a|b`", "da`", "sa|y"),
        ("\"a\" \"b|\"", "di\"", "\"a\" \"|\""),
        ("\"a\\\"|b\"", "di\"", "\"|\""),
    ]);
}

#[test]
fn brackets() {
    table(&[
        ("(a |b)", "di(", "(|)"),
        ("(a |b)", "da(", "|"),
        ("(a |b)", "dib", "(|)"),
        ("f(a|b)", "ci(X<Esc>", "f(|X)"),
        ("((a|))", "di(", "((|))"),
        ("((a|))", "d2i(", "(|)"),
        ("(a (b|) c)", "da(", "(a | c)"),
        ("[a|b]", "di[", "[|]"),
        ("<a|b>", "di<", "<|>"),
        ("{a|b}", "diB", "{|}"),
        ("|x (a)", "di(", "x (|)"),
        ("(|a)", "di)", "(|)"),
        ("{\n  a|\n}", "di{", "{\n|}"),
        ("{\n  a|\n}", "da{", "|"),
    ]);
}

#[test]
fn with_no_pair_ahead_nothing_is_done() {
    table(&[("a|bc", "di(", "a|bc")]);
}

#[test]
fn paragraphs() {
    table(&[
        ("a\nb|\n\nc", "dip", "|\nc"),
        ("a\nb|\n\nc", "dap", "|c"),
        ("a\n\nb|", "dap", "|a"),
        ("a\n|\nb", "dip", "a\n|b"),
        ("a\n|\nb", "dap", "|a"),
        ("|a\n\nb\n\nc", "d2ap", "|c"),
    ]);
}

#[test]
fn yanking_an_object_does_not_leave_the_text() {
    let mut ed = Ed::new("a (b|c) d");
    ed.keys("yi(");
    assert_eq!(ed.regs.get(None).unwrap().text, "bc");
    assert_eq!(ed.shows(), "a (|bc) d");
}
