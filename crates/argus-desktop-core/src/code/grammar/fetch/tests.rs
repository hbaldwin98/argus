use super::*;
use crate::code::grammar::net::Table;

const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

fn search_url(language: &str) -> String {
    format!("https://registry.npmjs.org/-/v1/search?text=tree-sitter-{language}&size=10")
}

fn listing_url(package: &str) -> String {
    format!("https://data.jsdelivr.com/v1/packages/npm/{package}?structure=flat")
}

fn found(packages: &[(&str, &str, Option<&str>)]) -> String {
    let objects: Vec<String> = packages
        .iter()
        .map(|(name, version, publisher)| match publisher {
            Some(user) => format!(r#"{{"package":{{"name":"{name}","version":"{version}","publisher":{{"username":"{user}"}}}}}}"#),
            None => format!(r#"{{"package":{{"name":"{name}","version":"{version}"}}}}"#),
        })
        .collect();
    format!(r#"{{"objects":[{}]}}"#, objects.join(","))
}

fn files(names: &[(&str, u64)]) -> String {
    let files: Vec<String> = names.iter().map(|(n, s)| format!(r#"{{"name":"{n}","size":{s}}}"#)).collect();
    format!(r#"{{"files":[{}]}}"#, files.join(","))
}

fn kotlin_listing() -> String {
    files(&[("/package.json", 10), ("/tree-sitter-kotlin.wasm", 5000), ("/queries/highlights.scm", 300)])
}

#[test]
fn the_language_is_what_npm_calls_it_by_the_extension() {
    // EDITOR-GRAM-3.
    let language = |path: &str| language_of(path);
    assert_eq!(language("a/b/Main.KT").as_deref(), Some("kotlin"));
    assert_eq!(language("ci.yml").as_deref(), Some("yaml"));
    assert_eq!(language("notes.md").as_deref(), Some("markdown"));
    assert_eq!(language("x.gql").as_deref(), Some("graphql"));
    assert_eq!(language("x.fsx").as_deref(), Some("fsharp"));
    assert_eq!(language("Dockerfile").as_deref(), Some("dockerfile"));
    assert_eq!(language("dir/Makefile").as_deref(), Some("make"));
    assert_eq!(language("build.dockerfile").as_deref(), Some("dockerfile"));
}

#[test]
fn any_other_extension_that_reads_as_a_name_is_its_own_language() {
    assert_eq!(language_of("a.swift").as_deref(), Some("swift"));
    assert_eq!(language_of("a.abcdefghijkl").as_deref(), Some("abcdefghijkl"), "twelve letters");
    assert_eq!(language_of("a.abcdefghijklm"), None, "thirteen is too long to be a package's");
    assert_eq!(language_of("a.1x"), None, "a name starts with a letter");
    assert_eq!(language_of("a.c++"), None);
    assert_eq!(language_of("a.x-y"), None);
}

#[test]
fn a_file_with_no_extension_of_its_own_has_no_language() {
    assert_eq!(language_of("README"), None);
    assert_eq!(language_of(".gitignore"), None, "a leading dot is no extension");
    assert_eq!(language_of("dir.d/file"), None, "the file's name, not a directory's");
    assert_eq!(language_of(""), None);
}

#[test]
fn a_pinned_grammar_is_offered_without_asking_anyone() {
    let http = Table::default();
    let offer = offer_for("src/main.rs", &http).unwrap();
    assert!(offer.pinned);
    assert_eq!((offer.name.as_str(), offer.label.as_str()), ("rust", "Rust"));
    assert_eq!(offer.source, "tree-sitter-rust@0.24.0");
    assert_eq!(offer.publisher, None);
    assert!(offer.wasm.sha256.is_some() && offer.queries.iter().all(|q| q.sha256.is_some()));
    assert_eq!(offer.bytes(), 1102547 + 3527, "the parser and the query");
    assert!(http.asked.lock().unwrap().is_empty());
}

#[test]
fn a_file_with_no_pinned_grammar_is_searched_for_and_not_trusted() {
    let http = Table::default()
        .with(&search_url("kotlin"), found(&[("tree-sitter-kotlin", "1.2.3", Some("fwcd"))]))
        .with(&listing_url("tree-sitter-kotlin@1.2.3"), kotlin_listing());
    let offer = offer_for("App.kt", &http).unwrap();
    assert!(!offer.pinned);
    assert_eq!((offer.name.as_str(), offer.label.as_str()), ("kotlin", "kotlin"), "EDITOR-GRAM-13");
    assert_eq!(offer.source, "tree-sitter-kotlin@1.2.3");
    assert_eq!(offer.publisher.as_deref(), Some("fwcd"));
    assert_eq!(offer.wasm.url, "https://cdn.jsdelivr.net/npm/tree-sitter-kotlin@1.2.3/tree-sitter-kotlin.wasm");
    assert_eq!(offer.queries[0].url, "https://cdn.jsdelivr.net/npm/tree-sitter-kotlin@1.2.3/queries/highlights.scm");
    assert_eq!(offer.bytes(), 5300);
    assert!(offer.wasm.sha256.is_none() && offer.queries[0].sha256.is_none());
}

#[test]
fn a_package_that_only_mentions_the_language_is_not_the_grammar() {
    let http = Table::default()
        .with(
            &search_url("kotlin"),
            found(&[
                ("kotlin-highlighter", "1.0.0", None),
                ("tree-sitter-kotlin-extra", "1.0.0", None),
                ("x-tree-sitter-kotlin", "1.0.0", None),
                ("@Bad/tree-sitter-kotlin", "1.0.0", None),
            ]),
        )
        .with(&listing_url("kotlin-highlighter@1.0.0"), kotlin_listing());
    assert_eq!(offer_for("a.kt", &http), None);
    assert!(
        http.asked.lock().unwrap().iter().all(|u| !u.contains("npm/")),
        "no listing is read for a package that is not named for the grammar"
    );
}

#[test]
fn a_scoped_grammar_collectives_package_counts() {
    let http = Table::default()
        .with(&search_url("zzz"), found(&[("@tree-sitter-grammars/tree-sitter-zzz", "0.4.1", None)]))
        .with(
            &listing_url("@tree-sitter-grammars/tree-sitter-zzz@0.4.1"),
            files(&[("/tree-sitter-zzz.wasm", 9), ("/queries/highlights.scm", 1)]),
        );
    assert_eq!(offer_for("a.zzz", &http).unwrap().source, "@tree-sitter-grammars/tree-sitter-zzz@0.4.1");
}

#[test]
fn the_first_package_with_a_parser_and_a_query_wins() {
    let http = Table::default()
        .with(&search_url("kotlin"), found(&[("tree-sitter-kotlin", "1.0.0", None), ("@a/tree-sitter-kotlin", "2.0.0", None)]))
        .with(&listing_url("tree-sitter-kotlin@1.0.0"), files(&[("/tree-sitter-kotlin.wasm", 5)]))
        .with(&listing_url("@a/tree-sitter-kotlin@2.0.0"), kotlin_listing());
    assert_eq!(offer_for("a.kt", &http).unwrap().source, "@a/tree-sitter-kotlin@2.0.0", "the first has no query");
}

#[test]
fn a_package_with_several_parsers_is_not_guessed_at() {
    let several = files(&[("/schema.wasm", 3), ("/other.wasm", 4), ("/queries/highlights.scm", 1)]);
    let http = Table::default()
        .with(&search_url("kotlin"), found(&[("tree-sitter-kotlin", "1.0.0", None)]))
        .with(&listing_url("tree-sitter-kotlin@1.0.0"), several);
    assert_eq!(offer_for("a.kt", &http), None);

    // Unless one is named for the language, or only one is at the top.
    let named = files(&[("/schema.wasm", 3), ("/tree-sitter-kotlin.wasm", 4), ("/queries/highlights.scm", 1)]);
    let http = Table::default()
        .with(&search_url("kotlin"), found(&[("tree-sitter-kotlin", "1.0.0", None)]))
        .with(&listing_url("tree-sitter-kotlin@1.0.0"), named);
    assert!(offer_for("a.kt", &http).unwrap().wasm.url.ends_with("/tree-sitter-kotlin.wasm"));

    let only = files(&[("/parser.wasm", 3), ("/sub/other.wasm", 4), ("/queries/highlights.scm", 1)]);
    let http = Table::default()
        .with(&search_url("kotlin"), found(&[("tree-sitter-kotlin", "1.0.0", None)]))
        .with(&listing_url("tree-sitter-kotlin@1.0.0"), only);
    assert!(offer_for("a.kt", &http).unwrap().wasm.url.ends_with("/parser.wasm"), "a nested parser is not a top-level one");
}

#[test]
fn offline_or_with_npm_away_there_is_nothing_to_offer() {
    assert_eq!(offer_for("a.kt", &Table::default()), None);
    let http = Table::default().with(&search_url("kotlin"), "not json");
    assert_eq!(offer_for("a.kt", &http), None);
    let http = Table::default().with(&search_url("kotlin"), found(&[("tree-sitter-kotlin", "1.0.0", None)]));
    assert_eq!(offer_for("a.kt", &http), None, "a listing that will not answer");
    assert_eq!(offer_for("README", &Table::default()), None, "no language to search for");
}

#[test]
fn a_hash_is_lower_case_hex_of_the_bytes() {
    assert_eq!(sha256_hex(b"abc"), ABC);
    assert_eq!(sha256_hex(b"").len(), 64);
}

fn offer(wasm_hash: Option<&str>) -> Offer {
    Offer {
        name: "x".into(),
        label: "X".into(),
        source: "tree-sitter-x@1.0.0".into(),
        symbol: "x".into(),
        wasm: Remote { url: "https://cdn.jsdelivr.net/npm/x/x.wasm".into(), size: 3, sha256: wasm_hash.map(str::to_owned) },
        queries: vec![
            Remote { url: "https://cdn.jsdelivr.net/npm/x/a.scm".into(), size: 1, sha256: None },
            Remote { url: "https://cdn.jsdelivr.net/npm/x/b.scm".into(), size: 1, sha256: None },
        ],
        pinned: wasm_hash.is_some(),
        publisher: None,
    }
}

fn files_of_x() -> Table {
    Table::default()
        .with("https://cdn.jsdelivr.net/npm/x/x.wasm", "abc")
        .with("https://cdn.jsdelivr.net/npm/x/a.scm", "(a) @keyword")
        .with("https://cdn.jsdelivr.net/npm/x/b.scm", "(b) @string")
}

#[test]
fn the_files_are_downloaded_and_the_queries_joined_by_newlines() {
    let got = download(&offer(Some(ABC)), &files_of_x()).unwrap();
    assert_eq!(got.wasm, b"abc");
    assert_eq!(got.query, "(a) @keyword\n(b) @string");
    assert_eq!((got.name.as_str(), got.label.as_str(), got.source.as_str()), ("x", "X", "tree-sitter-x@1.0.0"));
}

#[test]
fn a_pinned_file_that_differs_is_refused_and_a_found_one_is_not_checked() {
    let wrong = "0".repeat(64);
    let said = download(&offer(Some(&wrong)), &files_of_x()).unwrap_err();
    assert_eq!(said, "https://cdn.jsdelivr.net/npm/x/x.wasm is not the file Argus checked");
    assert!(download(&offer(None), &files_of_x()).is_ok());
}

#[test]
fn a_refused_download_says_what_answered() {
    let http = Table::default().with("https://cdn.jsdelivr.net/npm/x/x.wasm", "abc");
    assert_eq!(download(&offer(None), &http).unwrap_err(), "https://cdn.jsdelivr.net/npm/x/a.scm answered 404");
}

#[test]
fn the_symbol_is_what_the_parser_file_exports_not_the_language() {
    assert_eq!(symbol_of("tree-sitter-rust.wasm"), "rust");
    assert_eq!(symbol_of("tree-sitter-c_sharp.wasm"), "c_sharp");
    assert_eq!(symbol_of("/tree-sitter-c-sharp.wasm"), "c_sharp");
    assert_eq!(symbol_of("/parser.wasm"), "parser");
}

#[test]
fn a_searched_package_whose_only_parser_has_another_name_keeps_its_symbol() {
    // EDITOR-GRAM-4/8: the page finds the export itself; here it follows the file.
    let only = files(&[("/tree-sitter-kotlin-ng.wasm", 3), ("/queries/highlights.scm", 1)]);
    let http = Table::default()
        .with(&search_url("kotlin"), found(&[("tree-sitter-kotlin", "1.0.0", None)]))
        .with(&listing_url("tree-sitter-kotlin@1.0.0"), only);
    let offer = offer_for("a.kt", &http).unwrap();
    assert_eq!((offer.name.as_str(), offer.symbol.as_str()), ("kotlin", "kotlin_ng"));
}

#[test]
fn a_pinned_offer_carries_the_symbol_of_its_parser_file() {
    assert_eq!(offer_for("A.cs", &Table::default()).unwrap().symbol, "c_sharp");
}
