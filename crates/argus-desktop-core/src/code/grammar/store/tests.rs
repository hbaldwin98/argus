use super::*;

fn kotlin() -> Installed {
    Installed {
        name: "kotlin".into(),
        label: "kotlin".into(),
        source: "tree-sitter-kotlin@1.0.0".into(),
        symbol: "kotlin".into(),
        wasm: vec![0, 97, 115, 109, 1, 0, 0, 0],
        query: "(identifier) @variable\n\"fun\" @keyword".into(),
    }
}

#[test]
fn a_kept_grammar_is_read_back_by_the_next_window() {
    let dir = tempfile::tempdir().unwrap();
    Store::new(dir.path()).keep(&kotlin()).unwrap();
    // A new store has no memory of it: this is what the next window sees.
    assert_eq!(Store::new(dir.path()).installed("kotlin"), Some(kotlin()));
}

#[test]
fn a_grammar_that_was_never_kept_is_none() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(Store::new(dir.path()).installed("kotlin"), None);
    assert_eq!(Store::new(dir.path().join("not-made-yet")).installed("kotlin"), None);
}

#[test]
fn a_grammar_stopped_half_way_is_not_read() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    store.keep(&kotlin()).unwrap();
    std::fs::remove_file(dir.path().join("kotlin.json")).unwrap();
    assert_eq!(Store::new(dir.path()).installed("kotlin"), None, "the metadata is what says it is whole");
    std::fs::write(dir.path().join("kotlin.json"), "{").unwrap();
    assert_eq!(Store::new(dir.path()).installed("kotlin"), None, "and it has to be readable");
}

#[test]
fn a_grammar_whose_write_failed_still_serves_the_window() {
    let dir = tempfile::tempdir().unwrap();
    // A file where the folder should be: nothing can be written beneath it.
    let blocked = dir.path().join("grammars");
    std::fs::write(&blocked, "in the way").unwrap();
    let store = Store::new(&blocked);
    assert!(store.keep(&kotlin()).is_err());
    assert_eq!(store.installed("kotlin"), Some(kotlin()));
    assert_eq!(Store::new(&blocked).installed("kotlin"), None, "but not the next one");
}

#[test]
fn a_name_that_is_not_a_file_name_is_never_read_or_written() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().join("grammars"));
    for name in ["", "../kotlin", "a/b", "a\\b", "Kotlin", "k.t", "con:", &"a".repeat(41)] {
        let mut bad = kotlin();
        bad.name = name.to_owned();
        assert!(store.keep(&bad).is_err(), "{name:?}");
        assert_eq!(store.installed(name), None, "{name:?}");
    }
    assert!(!dir.path().join("grammars").exists(), "nothing was made for a refused name");
}

#[test]
fn a_file_is_kept_under_the_name_it_is_looked_up_by() {
    // The page keeps a search's grammar under an extension and looks a
    // Dockerfile up by nothing; one key serves both here.
    assert_eq!(key_for("src/lib.rs").as_deref(), Some("rust"));
    assert_eq!(key_for("Gemfile").as_deref(), Some("ruby"));
    assert_eq!(key_for("a/Dockerfile").as_deref(), Some("dockerfile"));
    assert_eq!(key_for("Makefile").as_deref(), Some("make"));
    assert_eq!(key_for("App.kt").as_deref(), Some("kotlin"));
    assert_eq!(key_for("ci.yml"), key_for("ci.yaml"));
    assert_eq!(key_for("README"), None);
}

#[test]
fn every_key_is_a_name_the_store_will_keep() {
    for path in ["a.rs", "Dockerfile", "Makefile", "a.kt", "a.abcdefghijkl", "x.c_sharp", "a.cs"] {
        if let Some(key) = key_for(path) {
            assert!(safe(&key), "{path} -> {key}");
        }
    }
    for k in crate::code::grammar::known::KNOWN {
        assert!(safe(k.name), "{}", k.name);
    }
}

#[test]
fn a_symbol_that_differs_from_the_name_is_kept_and_an_old_meta_falls_back_to_the_name() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    store.keep(&Installed { symbol: "kotlin_ng".into(), ..kotlin() }).unwrap();
    assert_eq!(Store::new(dir.path()).installed("kotlin").unwrap().symbol, "kotlin_ng");
    std::fs::write(dir.path().join("kotlin.json"), br#"{"label":"kotlin","source":"s"}"#).unwrap();
    assert_eq!(Store::new(dir.path()).installed("kotlin").unwrap().symbol, "kotlin");
}
