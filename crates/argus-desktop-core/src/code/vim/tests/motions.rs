//! The motions the library has beyond the first few (EDITOR-VIM-7): `(` and
//! `)` by sentence, `]` and `[` with a symbol, and the text objects `is`,
//! `as`, `it` and `at`.

use super::*;

const TEXT: &str = "Hello world. This is two. And three.";

#[test]
fn a_sentence_motion_goes_to_the_start_of_the_next_or_last_sentence() {
    table(&[
        ("|Hello world. This is two. And three.", ")", "Hello world. |This is two. And three."),
        ("|Hello world. This is two. And three.", "))", "Hello world. This is two. |And three."),
        ("|Hello world. This is two. And three.", "2)", "Hello world. This is two. |And three."),
        ("Hello world. This is two. |And three.", "(", "Hello world. |This is two. And three."),
        ("Hello world. This is two. |And three.", "((", "|Hello world. This is two. And three."),
        ("Hello world. This is |two. And three.", "(", "Hello world. |This is two. And three."),
        ("Hello world. This is |two. And three.", ")", "Hello world. This is two. |And three."),
    ]);
}

#[test]
fn the_last_sentence_ends_on_its_last_character() {
    table(&[("|Hello world. This is two. And three.", ")))", "Hello world. This is two. And three|.")]);
}

#[test]
fn an_empty_line_is_a_sentence_boundary() {
    table(&[
        ("|Hello world.\n\nNext para here. Yes.", ")", "Hello world.\n|\nNext para here. Yes."),
        ("|Hello world.\n\nNext para here. Yes.", "))", "Hello world.\n\n|Next para here. Yes."),
    ]);
}

#[test]
fn an_operator_takes_the_text_up_to_a_sentence_motion() {
    table(&[
        ("Hello world. This is |two. And three.", "d)", "Hello world. This is |And three."),
        ("Hello world. This is |two. And three.", "d(", "Hello world. |two. And three."),
    ]);
}

#[test]
fn is_and_as_take_a_sentence_without_and_with_the_blank_after_it() {
    table(&[
        ("Hello world. This is |two. And three.", "dis", "Hello world. | And three."),
        ("Hello world. This is |two. And three.", "das", "Hello world. |And three."),
        ("Hello world. This is |two. And three.", "yisP", "Hello world. This is two|.This is two. And three."),
        ("Hello |world. This is two.", "cisX<Esc>", "|X This is two."),
        ("|One. Two. Three.", "d2as", "|Three."),
    ]);
}

#[test]
fn as_on_the_last_sentence_takes_the_blank_before_it() {
    table(&[("One. Two. |Three.", "das", "One. Two|."), ("One. Two. |Three.", "dis", "One. Two.| ")]);
}

#[test]
fn a_sentence_object_in_a_selection_selects_it() {
    let mut ed = Ed::new("|One. Two. Three.");
    ed.keys("vis");
    assert_eq!(ed.vim.selection(), Some((0, 4)));
    let _ = TEXT;
}

#[test]
fn a_bracket_symbol_finds_the_next_or_last_unmatched_bracket() {
    table(&[
        ("{ a |{ b } c }", "]}", "{ a { b |} c }"),
        ("{ a { b |} c }", "]}", "{ a { b } c |}"),
        ("{ a |{ b } c }", "[{", "|{ a { b } c }"),
        ("( a |( b ) c )", "])", "( a ( b |) c )"),
        ("{ a { b |} c }", "d]}", "{ a { b |}"),
    ]);
}

#[test]
fn double_brackets_go_to_the_sections_that_begin_in_the_first_column() {
    table(&[
        ("|a\n{\nb\n}\n{\nc\n}", "]]", "a\n|{\nb\n}\n{\nc\n}"),
        ("|a\n{\nb\n}\n{\nc\n}", "]]]]", "a\n{\nb\n}\n|{\nc\n}"),
        ("a\n{\nb\n}\n{\nc\n|}", "[[", "a\n{\nb\n}\n|{\nc\n}"),
        ("a\n{\nb\n}\n|{\nc\n}", "][", "a\n{\nb\n}\n{\nc\n|}"),
        ("a\n{\nb\n}\n{\nc\n}|", "[]", "a\n{\nb\n|}\n{\nc\n}"),
        // With none ahead, the last line; with none behind, the first.
        ("fn a() {\n    x\n}\nfn b() {\n  |y\n}", "]]", "fn a() {\n    x\n}\nfn b() {\n  y\n|}"),
        ("fn a() {\n    x\n}\nfn b() {\n  |y\n}", "[[", "|fn a() {\n    x\n}\nfn b() {\n  y\n}"),
    ]);
}

#[test]
fn a_comment_end_and_a_comment_start() {
    table(&[("/* a |b */ c", "]/", "/* a b *|/ c"), ("a /* |b */ c", "[*", "a |/* b */ c")]);
}

#[test]
fn m_and_capital_m_go_to_method_braces() {
    table(&[
        ("a {\n  b\n  |c\n}\nd {\n}", "]m", "a {\n  b\n  c\n}\nd |{\n}"),
        ("a {\n  b\n  |c\n}\nd {\n}", "]M", "a {\n  b\n  c\n|}\nd {\n}"),
        ("a {\n  b\n}\nd {\n|}", "[m", "a {\n  b\n}\nd |{\n}"),
    ]);
}

#[test]
fn a_preprocessor_line_is_a_symbol_too() {
    // The walk starts after the first character, so the `#if` it begins on is not counted.
    table(&[("|#if x\nb\n#endif\nc", "]#", "#if x\nb\n|#endif\nc"), ("a\n|b\n#endif", "]#", "a\nb\n|#endif")]);
}

#[test]
fn a_symbol_that_is_none_of_these_goes_nowhere_and_is_no_jump() {
    table(&[("a|bc", "]x", "a|bc"), ("a|bc", "]x<C-o>", "a|bc")]);
}

#[test]
fn tag_objects_take_the_element_around_the_cursor() {
    table(&[
        ("<a><b>x|</b></a>", "dit", "<a><b>|</b></a>"),
        ("<a><b>x|</b></a>", "dat", "<a>|</a>"),
        ("<a><b>x|</b></a>", "d2it", "<a>|</a>"),
        ("<a><b>x|</b></a>", "d2at", "|"),
        ("<a><b>x|</b></a>", "cit-<Esc>", "<a><b>|-</b></a>"),
        ("<div class=\"x\">te|xt</div>", "dit", "<div class=\"x\">|</div>"),
        ("<div class=\"x\">te|xt</div>", "dat", "|"),
        ("<p>a<br>b|</p>", "dit", "<p>|</p>"),
        ("<p>a</p><p>|b</p>", "dat", "<p>a</p|>"),
    ]);
}

#[test]
fn a_tag_object_works_from_either_tag() {
    table(&[("|<p>a</p>", "dit", "<p>|</p>"), ("<p>a</p|>", "dit", "<p>|</p>"), ("<p|>a</p>", "dat", "|")]);
}

#[test]
fn a_tag_object_with_no_element_around_the_cursor_does_nothing() {
    table(&[("no tags |here", "dit", "no tags |here"), ("<a></a> |x", "dit", "<a></a> |x")]);
}

#[test]
fn comments_self_closing_and_void_tags_are_not_elements() {
    table(&[
        ("<a><!-- <b> -->|x</a>", "dit", "<a>|</a>"),
        ("<a><img src=\"x>\" />|x</a>", "dit", "<a>|</a>"),
        ("<ul><li>a<li>|b</ul>", "dit", "<ul>|</ul>"),
    ]);
}
