//! `gc` and `=` once a grammar is loaded: the file's language's comment token and its brackets'
//! indent, and nothing while there is none (EDITOR-VIM-24).

use super::grammar::{loader, rust};
use super::vim::settings;
use super::*;
use crate::keys::root::tests::type_text;

/// A file open with Vim on; `rust` has its grammar loaded before it opens, as an installed one is.
fn open_with(cx: &mut TestAppContext, text: &str, loaded: bool) -> (tempfile::TempDir, Opened) {
    let (dir, loader) = loader();
    if loaded {
        loader.adopt(rust());
    }
    with_grammars(loader, Arc::new(super::Offline));
    let opened = with(cx, editing(3, 1));
    hear(cx, &opened, vec![settings(true, false), opened_with("src/lib.rs", text)]);
    settle(cx, &opened);
    sent(&opened);
    (dir, opened)
}

/// EDITOR-VIM-24: with Rust's grammar loaded, `gcc` comments the line with Rust's token, and again
/// uncomments it.
#[gpui_kit::test]
fn gc_uses_the_loaded_languages_comment_token(cx: &mut TestAppContext) {
    let (_dir, opened) = open_with(cx, "fn a() {}\nfn b() {}\n", true);
    type_text(cx, &opened, "gcc");
    assert_eq!(text(cx).as_deref(), Some("// fn a() {}\nfn b() {}\n"));
    type_text(cx, &opened, "gcc");
    assert_eq!(text(cx).as_deref(), Some("fn a() {}\nfn b() {}\n"));
}

/// EDITOR-VIM-24: `=` indents the lines by the brackets open above them, a unit of four spaces
/// (the editor's own).
#[gpui_kit::test]
fn equals_indents_by_the_brackets_with_the_editors_four_spaces(cx: &mut TestAppContext) {
    let (_dir, opened) = open_with(cx, "fn a() {\nlet x = 1;\nif x {\ny();\n}\n}\n", true);
    type_text(cx, &opened, "=G");
    assert_eq!(text(cx).as_deref(), Some("fn a() {\n    let x = 1;\n    if x {\n        y();\n    }\n}\n"));
}

/// EDITOR-VIM-24: with no grammar loaded the file has no token and no indent rule, so both keys do
/// nothing and say nothing, like an unbound key.
#[gpui_kit::test]
fn gc_and_equals_do_nothing_while_no_grammar_is_loaded(cx: &mut TestAppContext) {
    let (_dir, opened) = open_with(cx, "fn a() {\nlet x = 1;\n}\n", false);
    type_text(cx, &opened, "gcc");
    type_text(cx, &opened, "=G");
    assert_eq!(text(cx).as_deref(), Some("fn a() {\nlet x = 1;\n}\n"));
    assert_eq!(sent(&opened), Vec::<String>::new());
}

/// EDITOR-VIM-24: a language written by layout has a comment token but no bracket indent, so `gcc`
/// comments with its own token and `=` changes nothing.
#[gpui_kit::test]
fn equals_leaves_a_language_that_is_not_written_in_braces_alone(cx: &mut TestAppContext) {
    let (_dir, opened) = open_with(cx, "x = f(\n1,\n)\n", true);
    let editor = editor();
    cx.update(|cx| {
        editor.update(cx, |editor, _| {
            let key = editor.shown.clone().unwrap();
            editor.sessions.get_mut(&key).unwrap().language = Some("python".into());
        })
    });
    type_text(cx, &opened, "=G");
    assert_eq!(text(cx).as_deref(), Some("x = f(\n1,\n)\n"));
    type_text(cx, &opened, "gcc");
    assert_eq!(text(cx).as_deref(), Some("# x = f(\n1,\n)\n"), "its own comment token is still used");
}
