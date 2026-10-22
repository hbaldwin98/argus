//! What a file defines — its functions, types, methods and modules — as the
//! tag queries its grammar ships find them. The outline of a file the Code
//! stage shows.
//!
//! Like highlighting, this is decoration: a file with no grammar, no tag
//! query, too much text or a failed parse has no outline, never an error.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use argus_protocol::CodeSymbol;
use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

use crate::syntax::grammar::Grammar;

/// The same bound highlighting keeps, for the same reason: a generated file
/// is megabytes, and the person is waiting on the read.
const MAX_OUTLINE_BYTES: usize = 512 * 1024;

/// The tag query a grammar ships, if it ships one. TypeScript's is only its
/// additions to JavaScript's, as its highlight query is, so both grammars
/// take the two together when they compile together — JavaScript's names
/// JSX, which plain TypeScript cannot parse, so it falls back to its own.
fn sources(grammar: Grammar) -> Vec<String> {
    let typescript = || {
        vec![
            format!("{}\n{}", tree_sitter_typescript::TAGS_QUERY, tree_sitter_javascript::TAGS_QUERY),
            tree_sitter_typescript::TAGS_QUERY.to_string(),
        ]
    };
    match grammar {
        Grammar::Rust => vec![tree_sitter_rust::TAGS_QUERY.to_string()],
        Grammar::TypeScript | Grammar::Tsx => typescript(),
        Grammar::Python => vec![tree_sitter_python::TAGS_QUERY.to_string()],
        Grammar::CSharp => vec![tree_sitter_c_sharp::TAGS_QUERY.to_string()],
        Grammar::Css | Grammar::Yaml | Grammar::Toml | Grammar::Json | Grammar::Markdown => Vec::new(),
    }
}

/// Compiled once per grammar and kept: compiling a query costs more than
/// running it over the file that asked.
fn query(grammar: Grammar) -> Option<&'static Query> {
    static CACHE: OnceLock<Mutex<HashMap<Grammar, Option<&'static Query>>>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(Default::default).lock().ok()?;
    *cache.entry(grammar).or_insert_with(|| {
        let language = grammar.language();
        let compiled = sources(grammar).into_iter().find_map(|source| Query::new(&language, &source).ok());
        if compiled.is_none() && !sources(grammar).is_empty() {
            tracing::warn!(grammar = grammar.name(), "tag query failed to compile");
        }
        compiled.map(|query| &*Box::leak(Box::new(query)))
    })
}

/// What `source`, read as `path`, defines, in the order it defines it.
pub fn of(path: &str, source: &str) -> Vec<CodeSymbol> {
    if source.len() > MAX_OUTLINE_BYTES {
        return Vec::new();
    }
    let Some(grammar) = Grammar::for_path(path) else {
        return Vec::new();
    };
    let Some(query) = query(grammar) else {
        return Vec::new();
    };
    let mut parser = Parser::new();
    if parser.set_language(&grammar.language()).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };
    let names = query.capture_names();
    let mut symbols = Vec::new();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), source.as_bytes());
    while let Some(found) = matches.next() {
        // A tag pattern pairs a `@name` with what kind of thing it names;
        // the `@reference.*` patterns name uses, which an outline leaves out.
        let kind = found
            .captures
            .iter()
            .find_map(|c| names[c.index as usize].strip_prefix("definition."));
        let name = found.captures.iter().find(|c| names[c.index as usize] == "name");
        let (Some(kind), Some(name)) = (kind, name) else {
            continue;
        };
        let Ok(text) = name.node.utf8_text(source.as_bytes()) else {
            continue;
        };
        symbols.push(CodeSymbol {
            name: text.to_owned(),
            kind: kind.to_owned(),
            line: name.node.start_position().row as u32 + 1,
        });
    }
    symbols.sort_by_key(|s| s.line);
    symbols.dedup_by(|a, b| a.line == b.line && a.name == b.name);
    symbols
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outline(path: &str, source: &str) -> Vec<(String, String, u32)> {
        of(path, source).into_iter().map(|s| (s.name, s.kind, s.line)).collect()
    }

    #[test]
    fn a_rust_file_lists_what_it_defines_in_order() {
        let source = "struct Brief;\n\nimpl Brief {\n    fn read(&self) {}\n}\n\nfn main() {}\n";
        let found = outline("a.rs", source);
        assert!(found.contains(&("Brief".into(), "class".into(), 1)), "{found:?}");
        assert!(found.contains(&("read".into(), "method".into(), 4)), "{found:?}");
        assert!(found.contains(&("main".into(), "function".into(), 7)), "{found:?}");
        assert!(found.windows(2).all(|w| w[0].2 <= w[1].2), "in file order: {found:?}");
    }

    #[test]
    fn every_grammar_with_a_tag_query_compiles_it() {
        // A query that fails to compile would silently give every file of
        // that language an empty outline.
        for grammar in [Grammar::Rust, Grammar::TypeScript, Grammar::Tsx, Grammar::Python, Grammar::CSharp] {
            assert!(query(grammar).is_some(), "{grammar:?}");
        }
    }

    #[test]
    fn typescript_and_javascript_list_their_functions_and_classes() {
        let found = outline("a.ts", "interface Shape {}\nclass Box {}\nfunction draw() {}\n");
        assert!(found.iter().any(|s| s.0 == "Shape" && s.1 == "interface"), "{found:?}");
        assert!(found.iter().any(|s| s.0 == "draw" && s.1 == "function"), "{found:?}");
        let found = outline("a.js", "class Box {}\nfunction draw() {}\n");
        assert!(found.iter().any(|s| s.0 == "Box" && s.1 == "class"), "{found:?}");
    }

    #[test]
    fn a_file_without_a_tag_query_or_a_grammar_has_no_outline() {
        assert!(outline("a.toml", "[package]\nname = \"x\"\n").is_empty());
        assert!(outline("Makefile", "all:\n").is_empty());
    }
}
