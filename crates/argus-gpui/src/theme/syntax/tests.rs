use gpui_kit::{Hsla, Rgba};

use super::*;

fn tokens() -> Tokens {
    crate::theme::choice::tokens(argus_desktop_core::settings::remembered::Theme::default(), true, 100)
}

fn colour_of(colours: &SyntaxColors, name: &str) -> Option<Hsla> {
    colours.style(name).and_then(|style| style.color)
}

fn same(a: Hsla, b: Hsla) -> bool {
    let (a, b) = (Rgba::from(a), Rgba::from(b));
    (a.r - b.r).abs() < 0.01 && (a.g - b.g).abs() < 0.01 && (a.b - b.b).abs() < 0.01
}

#[test]
fn each_class_has_the_pages_colour() {
    // EDITOR-COLOUR-1: keyword mauve, string green, comment faint italic, number peach, type
    // yellow, function blue, property teal, operator muted.
    let t = tokens();
    let colours = colours(&t);
    for (name, want) in [
        ("keyword", t.mauve),
        ("string", t.green),
        ("comment", t.fg_faint),
        ("number", t.peach),
        ("boolean", t.peach),
        ("constant", t.peach),
        ("label", t.peach),
        ("type", t.yellow),
        ("constructor", t.yellow),
        ("tag", t.yellow),
        ("function", t.blue),
        ("property", t.teal),
        ("attribute", t.teal),
        ("operator", t.fg_muted),
        ("punctuation", t.fg_muted),
    ] {
        let got = colour_of(&colours, name).unwrap_or_else(|| panic!("{name} is not coloured"));
        assert!(same(got, want), "{name}");
    }
    assert!(colours.style("comment").unwrap().font_style.is_some(), "a comment is italic");
    assert!(colours.style("keyword").unwrap().font_style.is_none());
}

#[test]
fn a_dotted_capture_takes_its_first_words_colour_and_a_plain_variable_has_none() {
    let t = tokens();
    let colours = colours(&t);
    assert!(same(colour_of(&colours, "keyword.control.return").unwrap(), t.mauve));
    assert!(same(colour_of(&colours, "function.method").unwrap(), t.blue));
    assert!(same(colour_of(&colours, "punctuation.bracket").unwrap(), t.fg_muted));
    for name in ["variable", "variable.builtin", "variable.parameter", "title", "emphasis", "link_uri", "embedded", "markup.heading"] {
        assert_eq!(colour_of(&colours, name), None, "{name} is the text's colour");
    }
}

#[test]
fn the_themes_own_colours_are_used_so_a_theme_change_recolours() {
    let mocha = tokens();
    let other = crate::theme::choice::tokens(argus_desktop_core::settings::remembered::Theme::Nord, true, 100);
    assert_ne!(colour_of(&colours(&mocha), "keyword"), colour_of(&colours(&other), "keyword"));
}

#[test]
fn a_query_renames_the_captures_the_kit_would_leave_uncoloured() {
    let q = "(a) @namespace (b) @module.builtin (c) @escape (d) @module (e) @string.special";
    assert_eq!(query(q), "(a) @type (b) @type.builtin (c) @string.escape (d) @type (e) @string.special");
}

#[test]
fn only_a_whole_first_word_is_renamed() {
    let q = "(a) @modules (b) @namespaces (c) @escapes (d) @_module (e) @keyword";
    assert_eq!(query(q), q);
    assert_eq!(query(""), "");
    assert_eq!(query("no captures here"), "no captures here");
    assert_eq!(query("(x) @escape\n(y) @namespace"), "(x) @string.escape\n(y) @type", "a capture at the end of a line, and of the text");
    assert_eq!(query("(x) @escape"), "(x) @string.escape");
}

#[test]
fn every_word_of_the_table_is_coloured_once_its_query_is_renamed() {
    // The table is `argus-protocol`'s, shared with the daemon: a word added there that the kit
    // cannot resolve fails here, and is renamed in `kit_name`.
    let t = tokens();
    let colours = colours(&t);
    for (name, kind) in CAPTURES {
        let renamed = query(&format!("(x) @{name}"));
        let capture = renamed.rsplit('@').next().unwrap();
        let got = colour_of(&colours, capture).unwrap_or_else(|| panic!("@{name} is not coloured as @{capture}"));
        assert!(same(got, colour(*kind, &t).0), "@{name} is {kind:?}'s colour");
    }
}
