//! A file's grammar in the whole window: offered, declined, installed, and the first paint of a file
//! whose grammar is already loaded (EDITOR-GRAM, EDITOR-STATE-2, EDITOR-SET-2). A native grammar
//! stands in for an installed one, which loads the same way once it is in the loader.

use std::collections::HashMap;

use argus_desktop_core::code::grammar::load::Loaded;
use tree_sitter::Query;

use super::vim::settings;
use super::*;
use crate::checkouts::stage::tests::click_id;

/// Answers by url, and says 404 to the rest.
struct Table(HashMap<String, Vec<u8>>);

impl Http for Table {
    fn get(&self, url: &str, _: u64) -> Result<Vec<u8>, String> {
        self.0.get(url).cloned().ok_or_else(|| format!("{url} answered 404"))
    }
}

fn kotlin() -> Table {
    let mut table = HashMap::new();
    let mut put = |url: &str, body: &str| table.insert(url.to_owned(), body.as_bytes().to_vec());
    put("https://registry.npmjs.org/-/v1/search?text=tree-sitter-kotlin&size=10", r#"{"objects":[{"package":{"name":"tree-sitter-kotlin","version":"1.0.0","publisher":{"username":"someone"}}}]}"#);
    put("https://data.jsdelivr.com/v1/packages/npm/tree-sitter-kotlin@1.0.0?structure=flat", r#"{"files":[{"name":"/tree-sitter-kotlin.wasm","size":2048},{"name":"/queries/highlights.scm","size":1024}]}"#);
    put("https://cdn.jsdelivr.net/npm/tree-sitter-kotlin@1.0.0/tree-sitter-kotlin.wasm", "this is not a WebAssembly module");
    put("https://cdn.jsdelivr.net/npm/tree-sitter-kotlin@1.0.0/queries/highlights.scm", "(identifier) @variable");
    Table(table)
}

fn rust() -> Loaded {
    let language: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
    Loaded {
        name: "rust".into(),
        label: "Rust".into(),
        query: Query::new(&language, tree_sitter_rust::HIGHLIGHTS_QUERY).unwrap(),
        source: tree_sitter_rust::HIGHLIGHTS_QUERY.into(),
        language,
    }
}

fn loader() -> (tempfile::TempDir, Arc<Loader>) {
    let dir = tempfile::tempdir().unwrap();
    let loader = Arc::new(Loader::new(Store::new(dir.path().join("grammars"))));
    (dir, loader)
}

/// A window on `path` edited, its text `text`, grammars from `loader` and the network `net`.
fn open_with(cx: &mut TestAppContext, loader: &Arc<Loader>, net: Net, path: &str, text: &str, installs: bool) -> Opened {
    with_grammars(loader.clone(), net);
    let opened = with(cx, CodePage { path: Some(path.into()), ..editing(3, 1) });
    hear(cx, &opened, vec![settings(false, installs), opened_with(path, text)]);
    settle(cx, &opened);
    sent(&opened);
    opened
}

fn said(cx: &mut TestAppContext, opened: &Opened) -> Option<String> {
    label(cx, opened, "editor-grammar")
}

fn language(cx: &mut TestAppContext) -> String {
    let editing = state(cx);
    cx.update(|cx| editing.read(cx).language_name().to_string())
}

/// EDITOR-GRAM-6, -7: a file with no grammar installed is offered Argus's pinned one, with its
/// source and size, and Install and Not now beside it.
#[gpui_kit::test]
fn a_pinned_grammar_is_offered_with_its_source_and_size(cx: &mut TestAppContext) {
    let (_dir, loader) = loader();
    let opened = open_with(cx, &loader, Arc::new(Offline), "src/lib.rs", "fn main() {}\n", false);
    let said = said(cx, &opened).expect("an offer");
    assert!(said.starts_with("No colours for Rust yet. Install its tree-sitter grammar? tree-sitter-rust@0.24.0 from npm, "), "{said}");
    assert!(said.ends_with(" KB") || said.ends_with(" MB"), "{said}");
    assert!(present(cx, &opened, "editor-install") && present(cx, &opened, "editor-not-now"));
    assert_eq!(sent(&opened), Vec::<String>::new(), "nothing is downloaded until the person says yes");
}

/// EDITOR-GRAM-7: Not now puts the offer away, and the grammar is not offered again this window.
#[gpui_kit::test]
fn not_now_puts_the_offer_away_for_the_window(cx: &mut TestAppContext) {
    let (_dir, loader) = loader();
    let opened = open_with(cx, &loader, Arc::new(Offline), "src/lib.rs", "fn main() {}\n", false);
    click_id(cx, &opened, "editor-not-now");
    assert_eq!(said(cx, &opened), None);
    assert!(!present(cx, &opened, "editor-install"));
    says(cx, &opened, CodePage { path: Some("src/other.rs".into()), ..editing(3, 1) });
    hear(cx, &opened, vec![opened_with("src/other.rs", "fn other() {}\n")]);
    settle(cx, &opened);
    assert_eq!(said(cx, &opened), None, "another Rust file is not offered it again");
}

/// EDITOR-GRAM-7: a grammar found by searching says it is not checked by Argus and who published it.
#[gpui_kit::test]
fn a_searched_grammar_says_it_is_unchecked_and_by_whom(cx: &mut TestAppContext) {
    let (_dir, loader) = loader();
    let opened = open_with(cx, &loader, Arc::new(kotlin()), "src/App.kt", "fun main() {}\n", false);
    let said = said(cx, &opened).expect("an offer");
    assert_eq!(said, "No colours for kotlin yet. Install its tree-sitter grammar? tree-sitter-kotlin@1.0.0 from npm by someone, 3 KB; found by searching, not checked by Argus");
}

/// EDITOR-GRAM-8: an install that fails says what it was and why, and keeps nothing; the editor
/// stays plain.
#[gpui_kit::test]
fn an_install_that_fails_says_why_and_the_text_stays_plain(cx: &mut TestAppContext) {
    let (dir, loader) = loader();
    let opened = open_with(cx, &loader, Arc::new(kotlin()), "src/App.kt", "fun main() {}\n", false);
    click_id(cx, &opened, "editor-install");
    settle(cx, &opened);
    let said = said(cx, &opened).expect("a sentence");
    assert!(said.starts_with("Could not install tree-sitter-kotlin@1.0.0: "), "{said}");
    assert!(!dir.path().join("grammars").join("kotlin.wasm").exists(), "a grammar that will not load is never kept");
    assert!(!present(cx, &opened, "editor-install"), "the offer is spent");
}

/// EDITOR-SET-2: with Settings' install switch on, a pinned grammar is installed when its file
/// opens, without the offer; here the network has none, so what is said is the failure.
#[gpui_kit::test]
fn a_pinned_grammar_is_installed_unasked_when_settings_says_so(cx: &mut TestAppContext) {
    let (_dir, loader) = loader();
    let opened = open_with(cx, &loader, Arc::new(Offline), "src/lib.rs", "fn main() {}\n", true);
    let said = said(cx, &opened).expect("a sentence");
    assert!(said.starts_with("Could not install tree-sitter-rust@0.24.0: "), "it tried without being asked: {said}");
    assert!(!present(cx, &opened, "editor-install"), "and did not ask");
}

/// EDITOR-SET-2: a searched grammar is asked about whatever the switch says.
#[gpui_kit::test]
fn a_searched_grammar_is_asked_about_even_when_settings_installs_unasked(cx: &mut TestAppContext) {
    let (_dir, loader) = loader();
    let opened = open_with(cx, &loader, Arc::new(kotlin()), "src/App.kt", "fun main() {}\n", true);
    assert!(present(cx, &opened, "editor-install"));
}

/// EDITOR-GRAM-9, EDITOR-SWITCH-7: a grammar already loaded colours the first paint: the editor is
/// made in its language and nothing is offered.
#[gpui_kit::test]
fn a_grammar_already_loaded_colours_the_first_paint(cx: &mut TestAppContext) {
    let (_dir, loader) = loader();
    loader.adopt(rust());
    let opened = open_with(cx, &loader, Arc::new(Offline), "src/lib.rs", "fn main() {}\n", false);
    assert_eq!(language(cx), "rust");
    assert_eq!(said(cx, &opened), None, "nothing to offer");
    // Another file of the kind opens coloured too; a file of another kind does not.
    says(cx, &opened, CodePage { path: Some("src/other.rs".into()), ..editing(3, 1) });
    hear(cx, &opened, vec![opened_with("src/other.rs", "fn other() {}\n")]);
    settle(cx, &opened);
    assert_eq!(language(cx), "rust");
    says(cx, &opened, CodePage { path: Some("tool.py".into()), ..editing(3, 1) });
    hear(cx, &opened, vec![opened_with("tool.py", "print(1)\n")]);
    settle(cx, &opened);
    assert_ne!(language(cx), "rust", "a Python file is not Rust");
    assert!(said(cx, &opened).is_some_and(|said| said.starts_with("No colours for Python")), "it is offered its own");
}

/// EDITOR-GRAM-6: a grammar that loads colours every open editor of its kind, and no editor of
/// another. The grammar arrives after three files are open, as an install's result does.
#[gpui_kit::test]
fn a_grammar_that_loads_colours_every_open_editor_of_its_kind(cx: &mut TestAppContext) {
    let (_dir, loader) = loader();
    let opened = open_with(cx, &loader, Arc::new(Offline), "src/lib.rs", "fn main() {}
", false);
    // An edit not yet written keeps a file's editor open when another is shown.
    let dirty = |cx: &mut TestAppContext, opened: &Opened| {
        let typing = state(cx);
        cx.update_window(opened.handle, |_, window, cx| typing.update(cx, |state, cx| state.insert("X".to_owned(), window, cx))).unwrap();
    };
    dirty(cx, &opened);
    for (path, text) in [("src/other.rs", "fn other() {}
"), ("tool.py", "print(1)
")] {
        says(cx, &opened, CodePage { path: Some(path.into()), ..editing(3, 1) });
        hear(cx, &opened, vec![opened_with(path, text)]);
        settle(cx, &opened);
        dirty(cx, &opened);
    }
    let languages = |cx: &mut TestAppContext| {
        let editor = editor();
        cx.update(|cx| {
            let mut all: Vec<(String, Option<String>)> = editor.read(cx).sessions.iter().map(|(key, session)| (key.1.clone(), session.language.clone())).collect();
            all.sort();
            all
        })
    };
    assert!(languages(cx).iter().all(|(_, language)| language.is_none()), "nothing is coloured yet");
    loader.adopt(rust());
    let editor = editor();
    cx.update(|cx| {
        let key = editor.read(cx).sessions.keys().find(|key| key.1 == "src/lib.rs").cloned().unwrap();
        editor.update(cx, |editor, cx| editor.find_grammar(&key, cx));
    });
    settle(cx, &opened);
    assert_eq!(
        languages(cx),
        vec![("src/lib.rs".to_owned(), Some("rust".to_owned())), ("src/other.rs".to_owned(), Some("rust".to_owned())), ("tool.py".to_owned(), None)]
    );
}

/// EDITOR-STATE-2: while a clash is held the bar shows no grammar offer.
#[gpui_kit::test]
fn a_held_clash_has_the_bar_and_the_offer_comes_back_after(cx: &mut TestAppContext) {
    use argus_protocol::TextChange;
    let (_dir, loader) = loader();
    let opened = open_with(cx, &loader, Arc::new(Offline), "src/lib.rs", "let a = 1;\n", false);
    let typing = state(cx);
    cx.update_window(opened.handle, |_, window, cx| {
        typing.update(cx, |state, cx| {
            state.set_selected_range(1..1, cx);
            state.insert("X".to_owned(), window, cx);
        })
    })
    .unwrap();
    cx.run_until_parked();
    heard_edit(cx, &opened, EditEvent::Changed { checkout: 100, path: "src/lib.rs".into(), from: 0, version: 1, changes: vec![TextChange { from: 0, to: 3, insert: "var".into() }] });
    settle(cx, &opened);
    assert_eq!(bar(cx, &opened).as_deref(), Some("An agent changed what you were editing"));
    assert_eq!(said(cx, &opened), None, "no offer while the clash waits");
    click_id(cx, &opened, "editor-keep-mine");
    assert!(said(cx, &opened).is_some_and(|said| said.starts_with("No colours for Rust")), "the offer is back once it is settled");
}

/// EDITOR-GRAM-8, a real one: the pinned Rust grammar is downloaded, checked against its hash,
/// loaded and kept, and the open editor is coloured with it. Needs the network, so it is run by
/// hand: `cargo test -p argus-gpui -- --ignored an_installed_grammar`.
#[gpui_kit::test]
#[ignore = "downloads the pinned Rust grammar from npm"]
fn an_installed_grammar_colours_the_open_editor(cx: &mut TestAppContext) {
    let (dir, loader) = loader();
    let opened = open_with(cx, &loader, Arc::new(argus_desktop_core::code::grammar::net::Net::new()), "src/lib.rs", "fn main() {}\n", false);
    click_id(cx, &opened, "editor-install");
    for _ in 0..200 {
        cx.executor().advance_clock(std::time::Duration::from_millis(100));
        std::thread::sleep(std::time::Duration::from_millis(50));
        settle(cx, &opened);
        if language(cx) == "rust" {
            break;
        }
    }
    assert_eq!(language(cx), "rust");
    assert_eq!(said(cx, &opened), None);
    assert!(dir.path().join("grammars").join("rust.wasm").exists(), "kept for the next window");
    // The installed parser runs in the kit's highlighter and the words come out the page's colours.
    let code = "fn main() { let n = 42; }\n";
    let mut highlighter = gpui_kit::component::highlighter::SyntaxHighlighter::new("rust");
    highlighter.update(None, &gpui_kit::component::input::Rope::from(code), None);
    let tokens = cx.update(|cx| *crate::theme::Tokens::global(cx));
    let styles = highlighter.styles(&(0..code.len()), &crate::theme::kit::highlight(&tokens));
    let at = |word: &str| code.find(word).unwrap();
    let colour = |at: usize| styles.iter().rev().find(|(range, _)| range.contains(&at)).and_then(|(_, style)| style.color);
    assert_eq!(colour(at("fn")), Some(tokens.mauve), "fn is a keyword");
    assert_eq!(colour(at("main")), Some(tokens.blue), "main is a function");
    assert_eq!(colour(at("42")), Some(tokens.peach), "42 is a number");
}
