use super::*;

fn broken(name: &str) -> Installed {
    Installed {
        name: name.into(),
        label: name.into(),
        source: format!("tree-sitter-{name}@1.0.0"),
        symbol: name.into(),
        wasm: b"this is not a WebAssembly module".to_vec(),
        query: "(identifier) @variable".into(),
    }
}

#[test]
fn a_parser_that_is_not_webassembly_does_not_load() {
    // EDITOR-GRAM-8: one that will not load is never kept.
    let said = load(&broken("kotlin")).err().expect("garbage is refused");
    assert!(!said.is_empty());
}

#[test]
fn nothing_is_installed_for_a_file_nothing_was_installed_for() {
    let dir = tempfile::tempdir().unwrap();
    let loader = Loader::new(Store::new(dir.path()));
    assert!(loader.installed_for("App.kt").unwrap().is_none());
    assert!(loader.installed_for("README").unwrap().is_none(), "no language, no grammar");
    assert!(loader.loaded_for("App.kt").is_none());
}

#[test]
fn an_installed_grammar_that_will_not_load_says_so_and_is_not_held() {
    // EDITOR-GRAM-6: "Could not load the grammar: <error>".
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    store.keep(&broken("kotlin")).unwrap();
    let loader = Loader::new(Store::new(dir.path()));
    assert!(loader.installed_for("App.kt").is_err());
    assert!(loader.loaded_for("App.kt").is_none(), "a failure is not remembered as a grammar");
    assert!(loader.installed_for("Other.kt").is_err(), "and it is tried again");
}

#[test]
fn a_dockerfile_grammar_is_found_by_the_next_dockerfile() {
    // EDITOR-GRAM-12: the store key and the lookup key are one.
    let dir = tempfile::tempdir().unwrap();
    Store::new(dir.path()).keep(&broken("dockerfile")).unwrap();
    let loader = Loader::new(Store::new(dir.path()));
    assert!(loader.installed_for("build/Dockerfile").is_err(), "found, so it was tried and did not load");
    let empty = Loader::new(Store::new(tempfile::tempdir().unwrap().path()));
    assert!(empty.installed_for("build/Dockerfile").unwrap().is_none());
}
