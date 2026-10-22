use super::*;
use crate::code::grammar::net::{Net, Table};
use crate::code::grammar::store::{Installed, Store};

fn loader() -> (tempfile::TempDir, Loader) {
    let dir = tempfile::tempdir().unwrap();
    let loader = Loader::new(Store::new(dir.path().join("grammars")));
    (dir, loader)
}

fn none() -> HashSet<String> {
    HashSet::new()
}

fn kotlin_table(wasm: &[u8]) -> Table {
    Table::default()
        .with("https://registry.npmjs.org/-/v1/search?text=tree-sitter-kotlin&size=10", r#"{"objects":[{"package":{"name":"tree-sitter-kotlin","version":"1.0.0"}}]}"#)
        .with(
            "https://data.jsdelivr.com/v1/packages/npm/tree-sitter-kotlin@1.0.0?structure=flat",
            r#"{"files":[{"name":"/tree-sitter-kotlin.wasm","size":8},{"name":"/queries/highlights.scm","size":10}]}"#,
        )
        .with("https://cdn.jsdelivr.net/npm/tree-sitter-kotlin@1.0.0/tree-sitter-kotlin.wasm", wasm)
        .with("https://cdn.jsdelivr.net/npm/tree-sitter-kotlin@1.0.0/queries/highlights.scm", "(identifier) @variable")
}

#[test]
fn a_pinned_grammar_is_offered_unless_settings_says_to_install_it_unasked() {
    // EDITOR-GRAM-6, EDITOR-SET-2.
    let (_dir, loader) = loader();
    let http = Table::default();
    assert!(matches!(find("a.rs", &loader, &http, false, &none()), Found::Offer(o) if o.pinned && o.name == "rust"));
    assert!(matches!(find("a.rs", &loader, &http, true, &none()), Found::Install(o) if o.name == "rust"));
}

#[test]
fn a_searched_grammar_is_always_asked_about() {
    let (_dir, loader) = loader();
    let http = kotlin_table(b"wasm");
    assert!(matches!(find("App.kt", &loader, &http, true, &none()), Found::Offer(o) if !o.pinned));
    assert!(matches!(find("App.kt", &loader, &http, false, &none()), Found::Offer(_)));
}

#[test]
fn a_grammar_the_person_said_not_now_to_is_not_offered_again() {
    // EDITOR-GRAM-7: remembered by name for the window.
    let (_dir, loader) = loader();
    let declined: HashSet<String> = ["rust".to_owned(), "kotlin".to_owned()].into();
    assert!(matches!(find("a.rs", &loader, &Table::default(), true, &declined), Found::Nothing));
    assert!(matches!(find("App.kt", &loader, &kotlin_table(b"wasm"), false, &declined), Found::Nothing));
    let other: HashSet<String> = ["python".to_owned()].into();
    assert!(matches!(find("a.rs", &loader, &Table::default(), false, &other), Found::Offer(_)));
}

#[test]
fn a_file_with_nothing_to_offer_finds_nothing() {
    let (_dir, loader) = loader();
    assert!(matches!(find("README", &loader, &Table::default(), true, &none()), Found::Nothing));
    assert!(matches!(find("App.kt", &loader, &Table::default(), true, &none()), Found::Nothing), "offline");
}

#[test]
fn an_installed_grammar_that_will_not_load_is_a_sentence_for_the_bar_not_an_offer() {
    let (_dir, loader) = loader();
    loader
        .store()
        .keep(&Installed {
            name: "rust".into(),
            label: "Rust".into(),
            source: "tree-sitter-rust@0.24.0".into(),
            symbol: "rust".into(),
            wasm: b"not wasm".to_vec(),
            query: "(identifier) @variable".into(),
        })
        .unwrap();
    match find("a.rs", &loader, &Table::default(), true, &none()) {
        Found::Failed(said) => assert!(said.starts_with("Could not load the grammar: "), "{said}"),
        _ => panic!("expected the failure"),
    }
}

#[test]
fn a_grammar_that_will_not_load_is_never_kept() {
    // EDITOR-GRAM-8: the parser is compiled before it is stored.
    let (dir, loader) = loader();
    let offer = offer_for("App.kt", &kotlin_table(b"not wasm")).unwrap();
    assert!(install(&offer, &loader, &kotlin_table(b"not wasm")).is_err());
    assert!(loader.store().installed("kotlin").is_none());
    assert!(!dir.path().join("grammars").exists(), "nothing was written");
    assert!(loader.loaded_for("App.kt").is_none());
}

#[test]
fn a_pinned_file_that_is_not_the_one_checked_installs_nothing() {
    let (dir, loader) = loader();
    let offer = offer_for("a.rs", &Table::default()).unwrap();
    let http = Table::default()
        .with(&offer.wasm.url, b"someone else's parser".to_vec())
        .with(&offer.queries[0].url, "(identifier) @variable");
    let said = install(&offer, &loader, &http).err().unwrap();
    assert_eq!(said, format!("{} is not the file Argus checked", offer.wasm.url));
    assert_eq!(http.asked.lock().unwrap().len(), 1, "the queries are not fetched after a refusal");
    assert!(!dir.path().join("grammars").exists());
}

/// EDITOR-GRAM-11, and the open question of the editor plan: do the npm
/// parsers, built for `web-tree-sitter`, load in native tree-sitter? Needs the
/// network, so `cargo test -p argus-desktop-core -- --ignored
/// the_pins_are_the_files_npm_serves`.
#[test]
#[ignore = "downloads every pinned grammar"]
fn the_pins_are_the_files_npm_serves() {
    let net = Net::new();
    let mut failed = Vec::new();
    for known in crate::code::grammar::known::KNOWN {
        let path = format!("x.{}", known.exts.first().copied().unwrap_or("none"));
        let path = if known.exts.is_empty() { known.files[0].to_owned() } else { path };
        let offer = offer_for(&path, &net).unwrap();
        assert_eq!(offer.name, known.name);
        let grammar = match download(&offer, &net) {
            Ok(grammar) => grammar,
            Err(said) => {
                failed.push(format!("{}: {said}", known.name));
                continue;
            }
        };
        match load(&grammar).and_then(|loaded| loaded.parser().map(|mut p| (loaded, p.parse("", None).is_some()))) {
            Ok((loaded, parsed)) => assert!(parsed && !loaded.query.capture_names().is_empty(), "{}", known.name),
            Err(said) => failed.push(format!("{}: {said}", known.name)),
        }
    }
    assert!(failed.is_empty(), "{failed:#?}");
}

#[test]
#[ignore = "downloads the Rust grammar"]
fn an_installed_grammar_colours_the_next_window_with_the_daemons_kinds() {
    let net = Net::new();
    let (_dir, loader) = loader();
    let offer = offer_for("lib.rs", &net).unwrap();
    let loaded = install(&offer, &loader, &net).unwrap();

    let source = "fn main() { let s = \"é\"; }";
    let mut parser = loaded.parser().unwrap();
    let tree = parser.parse(source, None).unwrap();
    let mut cursor = tree_sitter::QueryCursor::new();
    let mut kinds = Vec::new();
    let mut captures = cursor.captures(&loaded.query, tree.root_node(), source.as_bytes());
    while let Some((found, _)) = tree_sitter::StreamingIterator::next(&mut captures) {
        for capture in found.captures {
            let name = loaded.query.capture_names()[capture.index as usize];
            if let Some(kind) = argus_protocol::review::captures::kind_of(name) {
                kinds.push((&source[capture.node.byte_range()], kind));
            }
        }
    }
    assert!(kinds.iter().any(|(text, kind)| *text == "fn" && *kind == argus_protocol::HighlightKind::Keyword), "{kinds:?}");

    // The next window: nothing in memory, the folder has it.
    let fresh = Loader::new(Store::new(loader.store().dir()));
    assert!(fresh.installed_for("other.rs").unwrap().is_some());
}
