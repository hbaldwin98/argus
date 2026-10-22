//! A grammar in the kit's registry, coloured by the token space: the page's classes land on the
//! same words (EDITOR-COLOUR-1), through the kit's own highlighter and a native grammar standing
//! in for an installed one.

use std::ops::Range;
use std::sync::Arc;

use gpui_kit::component::highlighter::SyntaxHighlighter;
use gpui_kit::component::input::Rope;
use gpui_kit::{Hsla, Rgba};
use tree_sitter::Parser;

use super::*;
use crate::theme::kit::highlight;

const CODE: &str = "// note\nfn main<'a>() {\n    let s = \"a\\n\";\n    let n = 42;\n}\nmod m {}\n";

fn tokens() -> crate::theme::Tokens {
    crate::theme::choice::tokens(argus_desktop_core::settings::remembered::Theme::default(), true, 100)
}

fn rust(name: &str) -> SyntaxHighlighter {
    rust_with(name, tree_sitter_rust::HIGHLIGHTS_QUERY)
}

fn rust_with(name: &str, query: &str) -> SyntaxHighlighter {
    let language: Language = tree_sitter_rust::LANGUAGE.into();
    let made = language.clone();
    super::language(
        name,
        language,
        query,
        Arc::new(move || {
            let mut parser = Parser::new();
            parser.set_language(&made)?;
            Ok((parser, made.clone()))
        }),
    );
    let mut highlighter = SyntaxHighlighter::new(name);
    highlighter.update(None, &Rope::from(CODE), None);
    highlighter
}

fn same(a: Hsla, b: Hsla) -> bool {
    let (a, b) = (Rgba::from(a), Rgba::from(b));
    (a.r - b.r).abs() < 0.01 && (a.g - b.g).abs() < 0.01 && (a.b - b.b).abs() < 0.01
}

/// The colour the word at `at` is drawn in, if it has one.
fn colour_at(styles: &[(Range<usize>, gpui_kit::HighlightStyle)], at: usize) -> Option<Hsla> {
    styles.iter().rev().find(|(range, _)| range.contains(&at)).and_then(|(_, style)| style.color)
}

#[test]
fn the_pages_classes_land_on_the_same_words() {
    let t = tokens();
    let theme = highlight(&t);
    let styles = rust("argus-test-rust-classes").styles(&(0..CODE.len()), &theme);
    let at = |word: &str, nth: usize| CODE.match_indices(word).nth(nth).unwrap_or_else(|| panic!("{word}")).0;
    for (what, at, want) in [
        ("a comment is faint", at("// note", 0), Some(t.fg_faint)),
        ("fn is a keyword", at("fn", 0), Some(t.mauve)),
        ("main is a function", at("main", 0), Some(t.blue)),
        ("a string is green", at("\"a", 0) + 1, Some(t.green)),
        ("a number is peach", at("42", 0), Some(t.peach)),
        ("a lifetime is a label, which is peach", at("'a", 0) + 1, Some(t.peach)),
        ("an escape is a string", at("\\n", 0), Some(t.green)),
        ("a plain variable is the text's own", at("let s", 0) + 4, None),
    ] {
        match (colour_at(&styles, at), want) {
            (Some(got), Some(want)) => assert!(same(got, want), "{what}"),
            (None, None) => {}
            (got, want) => panic!("{what}: got {got:?}, wanted {want:?}"),
        }
    }
}

#[test]
fn a_comment_is_italic_and_a_keyword_is_not() {
    let theme = highlight(&tokens());
    let styles = rust("argus-test-rust-italic").styles(&(0..CODE.len()), &theme);
    let style = |at: usize| styles.iter().rev().find(|(range, _)| range.contains(&at)).map(|(_, style)| *style).unwrap();
    assert!(style(0).font_style.is_some(), "a comment is italic");
    assert!(style(CODE.find("fn").unwrap()).font_style.is_none());
}

#[test]
fn a_theme_change_recolours_the_same_grammar() {
    let (mocha, nord) = (tokens(), crate::theme::choice::tokens(argus_desktop_core::settings::remembered::Theme::Nord, true, 100));
    let mut highlighter = rust("argus-test-rust-themes");
    let at = CODE.find("fn").unwrap();
    let first = colour_at(&highlighter.styles(&(0..CODE.len()), &highlight(&mocha)), at).unwrap();
    let second = colour_at(&highlighter.styles(&(0..CODE.len()), &highlight(&nord)), at).unwrap();
    assert!(same(first, mocha.mauve) && same(second, nord.mauve) && !same(first, second));
    let _ = &mut highlighter;
}

#[test]
fn the_captures_the_kit_has_no_name_for_are_coloured_as_the_pages_classes() {
    // `@namespace` and `@module` are types to the page and `@escape` a string; the kit resolves
    // neither, so the registry renames them (`theme/syntax`).
    let t = tokens();
    let theme = highlight(&t);
    let mut highlighter = rust_with("argus-test-rust-renamed", "(mod_item name: (identifier) @namespace) (escape_sequence) @escape (function_item name: (identifier) @module.builtin)");
    highlighter.update(None, &Rope::from(CODE), None);
    let styles = highlighter.styles(&(0..CODE.len()), &theme);
    assert!(same(colour_at(&styles, CODE.find("m {}").unwrap()).unwrap(), t.yellow), "a namespace is a type");
    assert!(same(colour_at(&styles, CODE.find("\\n").unwrap()).unwrap(), t.green), "an escape is a string");
    assert!(same(colour_at(&styles, CODE.find("main").unwrap()).unwrap(), t.yellow), "a dotted module is a type");
}
