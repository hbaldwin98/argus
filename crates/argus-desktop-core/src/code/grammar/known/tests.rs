use std::collections::BTreeSet;

use super::*;

fn hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn pins() -> impl Iterator<Item = &'static Pinned> {
    KNOWN.iter().flat_map(|k| std::iter::once(&k.wasm).chain(k.queries.iter()))
}

#[test]
fn every_pin_names_a_hash_and_a_file_on_jsdelivr() {
    // EDITOR-GRAM-1: each file is pinned to its SHA-256 and size and fetched
    // from jsDelivr; an unhashed or foreign url would be installed unchecked.
    for pin in pins() {
        assert!(hex64(pin.sha256), "{} has no lower-case SHA-256", pin.file);
        assert!(pin.size > 0, "{} has no size", pin.file);
        let url = pin.url();
        assert!(url.starts_with("https://cdn.jsdelivr.net/npm/"), "{url}");
        assert!(!url.contains(".."), "{url}");
        assert!(pin.package.rsplit_once('@').is_some_and(|(name, version)| !name.is_empty() && !version.is_empty()), "{}", pin.package);
    }
}

#[test]
fn a_known_grammar_is_a_parser_and_queries() {
    assert_eq!(KNOWN.len(), 23, "EDITOR-GRAM-1 lists twenty-three");
    let names: BTreeSet<_> = KNOWN.iter().map(|k| k.name).collect();
    assert_eq!(names.len(), KNOWN.len(), "a name is one grammar");
    for k in KNOWN {
        assert!(k.wasm.file.ends_with(".wasm"), "{}", k.name);
        assert!(!k.queries.is_empty(), "{} would colour nothing", k.name);
        assert!(k.queries.iter().all(|q| q.file.ends_with(".scm")), "{}", k.name);
        assert!(k.exts.iter().all(|e| *e == e.to_lowercase() && !e.starts_with('.')), "{}", k.name);
        assert!(k.exts.len() + k.files.len() > 0, "{} is found by nothing", k.name);
    }
}

#[test]
fn the_languages_that_extend_another_list_both_queries() {
    let queries = |name: &str| {
        KNOWN.iter().find(|k| k.name == name).unwrap().queries.iter().map(|q| q.package).collect::<Vec<_>>()
    };
    assert_eq!(queries("typescript"), ["tree-sitter-javascript@0.25.0", "tree-sitter-typescript@0.23.2"]);
    assert_eq!(queries("tsx").len(), 3, "JavaScript's, its JSX and TypeScript's");
    assert_eq!(queries("cpp").len(), 2, "C's and C++'s");
}

#[test]
fn the_pins_are_the_ones_the_page_installs() {
    // The page's list is the one that was hashed against npm; a pin changed
    // on one side only would install a different file in each window.
    let page = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../argus-desktop/page/src/code/grammar/known.ts"),
    )
    .unwrap();
    let theirs: BTreeSet<&str> = page
        .split("sha256: '")
        .skip(1)
        .map(|rest| rest.split('\'').next().unwrap())
        .collect();
    let ours: BTreeSet<&str> = pins().map(|p| p.sha256).collect();
    assert_eq!(ours, theirs);
}

#[test]
fn a_file_is_found_by_whole_name_then_by_lower_cased_extension() {
    let name = |path: &str| known_for(path).map(|k| k.name);
    assert_eq!(name("src/a.RS"), Some("rust"));
    assert_eq!(name("x/y/script.PYI"), Some("python"));
    assert_eq!(name("Gemfile"), Some("ruby"));
    assert_eq!(name(".bashrc"), Some("bash"), "a dotfile is found by its whole name");
    assert_eq!(name("gemfile"), None, "a whole name is case-sensitive");
    assert_eq!(name("a.ts"), Some("typescript"));
    assert_eq!(name("a.tsx"), Some("tsx"));
    assert_eq!(name("Cargo.lock"), Some("toml"));
}

#[test]
fn what_is_not_pinned_is_left_to_search() {
    let name = |path: &str| known_for(path).map(|k| k.name);
    assert_eq!(name("notes.md"), None);
    assert_eq!(name("a.yaml"), None);
    assert_eq!(name("Makefile"), None);
    assert_eq!(name(".rs"), None, "a leading dot is no extension");
    assert_eq!(name("dir.rs/file"), None, "the file's name, not a directory's");
    assert_eq!(name(""), None);
}
