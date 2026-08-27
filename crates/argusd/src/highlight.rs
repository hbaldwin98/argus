//! Syntax highlighting for review diffs. The daemon parses because the daemon
//! owns the blobs: the client is sent hunks, never whole files, and a parser
//! handed a bare hunk would be reading a fragment torn out of its syntax.
//! What crosses the wire is what a token *is*, never what colour it should be,
//! so the client's theme keeps the palette and stays a replaceable renderer.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use argus_protocol::{HighlightKind, HighlightSpan};
use tree_sitter::Language;
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

/// Anything larger is shipped unhighlighted. Parsing is linear in the source,
/// but a generated or minified file is megabytes on a handful of lines, and
/// this runs while a review request is waiting on it.
const MAX_HIGHLIGHT_BYTES: usize = 512 * 1024;

/// The capture names we ask grammars for, paired with what the client is told.
/// Tree-sitter resolves these as dotted prefixes, so `keyword` also claims
/// `keyword.control.return` without our listing every dialect's spelling.
///
/// What is deliberately absent is `variable`, which is most identifiers in most
/// files. Colouring those turns a diff into a wall of colour and buries the one
/// signal review exists for, which is which lines changed.
const CAPTURES: &[(&str, HighlightKind)] = &[
    ("attribute", HighlightKind::Property),
    ("boolean", HighlightKind::Constant),
    ("comment", HighlightKind::Comment),
    ("constant", HighlightKind::Constant),
    ("constructor", HighlightKind::Type),
    ("escape", HighlightKind::Str),
    ("function", HighlightKind::Function),
    ("keyword", HighlightKind::Keyword),
    ("label", HighlightKind::Constant),
    ("module", HighlightKind::Type),
    ("namespace", HighlightKind::Type),
    ("number", HighlightKind::Number),
    ("operator", HighlightKind::Operator),
    ("property", HighlightKind::Property),
    ("punctuation", HighlightKind::Punctuation),
    ("string", HighlightKind::Str),
    ("tag", HighlightKind::Type),
    ("type", HighlightKind::Type),
];

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Grammar {
    Rust,
    TypeScript,
    Tsx,
    Python,
    CSharp,
    Css,
    Yaml,
    Toml,
    Json,
    Markdown,
}

/// Extension alone. Content sniffing and shebang lines would buy a handful of
/// extensionless files at the cost of reading every blob twice.
fn grammar_for(path: &str) -> Option<Grammar> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let (_, ext) = name.rsplit_once('.')?;
    Some(match ext.to_ascii_lowercase().as_str() {
        "rs" => Grammar::Rust,
        "ts" | "mts" | "cts" => Grammar::TypeScript,
        // The TSX grammar is a superset, and plain TypeScript rejects JSX, so
        // JavaScript in any spelling is better served by TSX than by nothing.
        "tsx" | "js" | "jsx" | "mjs" | "cjs" => Grammar::Tsx,
        "py" | "pyi" => Grammar::Python,
        "cs" | "csx" => Grammar::CSharp,
        "css" => Grammar::Css,
        "yaml" | "yml" => Grammar::Yaml,
        "toml" => Grammar::Toml,
        "json" | "jsonc" => Grammar::Json,
        "md" | "markdown" => Grammar::Markdown,
        _ => return None,
    })
}

/// TypeScript's own query is only its additions to JavaScript's — types,
/// generics, decorators. Without the base query underneath it, `const` and
/// `return` and every string literal come back unhighlighted, so both
/// TypeScript grammars are configured with the two concatenated. Specific
/// rules lead, which is the order the upstream tooling uses. JSX lives in a
/// third query, and only the TSX grammar can parse what it matches.
fn typescript_query(jsx: bool) -> String {
    let mut query = format!(
        "{}\n{}",
        tree_sitter_typescript::HIGHLIGHTS_QUERY,
        tree_sitter_javascript::HIGHLIGHT_QUERY
    );
    if jsx {
        query.push('\n');
        query.push_str(tree_sitter_javascript::JSX_HIGHLIGHT_QUERY);
    }
    query
}

fn build(grammar: Grammar) -> Option<HighlightConfiguration> {
    let (language, name, highlights): (Language, &str, String) = match grammar {
        Grammar::Rust => (
            tree_sitter_rust::LANGUAGE.into(),
            "rust",
            tree_sitter_rust::HIGHLIGHTS_QUERY.to_string(),
        ),
        Grammar::TypeScript => (
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            "typescript",
            typescript_query(false),
        ),
        Grammar::Tsx => (
            tree_sitter_typescript::LANGUAGE_TSX.into(),
            "tsx",
            typescript_query(true),
        ),
        Grammar::Python => (
            tree_sitter_python::LANGUAGE.into(),
            "python",
            tree_sitter_python::HIGHLIGHTS_QUERY.to_string(),
        ),
        Grammar::CSharp => (
            tree_sitter_c_sharp::LANGUAGE.into(),
            "c-sharp",
            tree_sitter_c_sharp::HIGHLIGHTS_QUERY.to_string(),
        ),
        Grammar::Css => (
            tree_sitter_css::LANGUAGE.into(),
            "css",
            tree_sitter_css::HIGHLIGHTS_QUERY.to_string(),
        ),
        Grammar::Yaml => (
            tree_sitter_yaml::LANGUAGE.into(),
            "yaml",
            tree_sitter_yaml::HIGHLIGHTS_QUERY.to_string(),
        ),
        Grammar::Toml => (
            tree_sitter_toml_ng::LANGUAGE.into(),
            "toml",
            tree_sitter_toml_ng::HIGHLIGHTS_QUERY.to_string(),
        ),
        Grammar::Json => (
            tree_sitter_json::LANGUAGE.into(),
            "json",
            tree_sitter_json::HIGHLIGHTS_QUERY.to_string(),
        ),
        // Block structure only. Markdown splits its inline syntax into a second
        // grammar reached through injections, which buys emphasis and link text
        // for a second parse of every file; headings and fences are the part
        // that helps you find your place in a diff.
        Grammar::Markdown => (
            tree_sitter_md::LANGUAGE.into(),
            "markdown",
            tree_sitter_md::HIGHLIGHT_QUERY_BLOCK.to_string(),
        ),
    };

    let mut config = match HighlightConfiguration::new(language, name, &highlights, "", "") {
        Ok(config) => config,
        // A grammar whose query will not compile against its own parser is a
        // packaging fault, not a per-file one: say so once and go without.
        Err(err) => {
            tracing::warn!(grammar = name, %err, "highlight query failed to compile");
            return None;
        }
    };
    let names: Vec<&str> = CAPTURES.iter().map(|(name, _)| *name).collect();
    config.configure(&names);
    Some(config)
}

/// Configurations are built once and kept for the daemon's life. Compiling a
/// grammar's query costs more than parsing the file that needs it, and there
/// are only ever ten of them.
fn config(grammar: Grammar) -> Option<&'static HighlightConfiguration> {
    static CACHE: OnceLock<Mutex<HashMap<Grammar, Option<&'static HighlightConfiguration>>>> =
        OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache.lock().ok()?;
    *cache
        .entry(grammar)
        .or_insert_with(|| build(grammar).map(|config| &*Box::leak(Box::new(config))))
}

/// Per-line spans for one file's full text, indexed from line zero. `None` when
/// the path has no grammar, the file is too large, or the parse failed — each of
/// which means plain text rather than an error, because highlighting is
/// decoration and a review has to render without it.
pub fn line_spans(path: &str, source: &str) -> Option<Vec<Vec<HighlightSpan>>> {
    if source.len() > MAX_HIGHLIGHT_BYTES {
        return None;
    }
    let config = config(grammar_for(path)?)?;

    // Byte offsets into `source`. A line's usable width stops before its
    // terminator, matching the text the diff itself carries.
    let bytes = source.as_bytes();
    let mut starts = vec![0usize];
    let mut widths = Vec::new();
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'\n' {
            let start = *starts.last().unwrap();
            let end = if i > start && bytes[i - 1] == b'\r' {
                i - 1
            } else {
                i
            };
            widths.push(end - start);
            starts.push(i + 1);
        }
    }
    widths.push(source.len() - starts.last().unwrap());

    let mut spans: Vec<Vec<HighlightSpan>> = vec![Vec::new(); starts.len()];
    let mut highlighter = Highlighter::new();
    let events = highlighter.highlight(config, bytes, None, |_| None).ok()?;

    let mut stack: Vec<HighlightKind> = Vec::new();
    for event in events {
        match event.ok()? {
            HighlightEvent::HighlightStart(h) => stack.push(CAPTURES.get(h.0)?.1),
            HighlightEvent::HighlightEnd => {
                stack.pop();
            }
            HighlightEvent::Source { start, end } => {
                // Only the innermost highlight applies. Tree-sitter nests these
                // properly, so taking the top of the stack keeps spans disjoint.
                let Some(kind) = stack.last().copied() else {
                    continue;
                };
                push_span(&mut spans, &starts, &widths, start, end, kind);
            }
        }
    }
    Some(spans)
}

/// Splits one source range across the lines it covers. A multi-line token — a
/// block comment, a raw string — becomes one span per line, because the client
/// draws lines and never sees the file.
fn push_span(
    spans: &mut [Vec<HighlightSpan>],
    starts: &[usize],
    widths: &[usize],
    start: usize,
    end: usize,
    kind: HighlightKind,
) {
    let first = match starts.binary_search(&start) {
        Ok(i) => i,
        Err(i) => i.saturating_sub(1),
    };
    for i in first..starts.len() {
        let line_start = starts[i];
        if line_start >= end {
            break;
        }
        let line_end = line_start + widths[i];
        let from = start.max(line_start);
        let to = end.min(line_end);
        if to > from {
            spans[i].push(HighlightSpan {
                start: (from - line_start) as u32,
                end: (to - line_start) as u32,
                kind,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The spans covering one line, as `(text, kind)` pairs. Slicing the source
    /// back out with the offsets is what proves the offsets are right.
    fn spans_on<'a>(source: &'a str, path: &str, line: usize) -> Vec<(&'a str, HighlightKind)> {
        let all = line_spans(path, source).expect("grammar and parse");
        let start: usize = source.split_inclusive('\n').take(line).map(str::len).sum();
        all[line]
            .iter()
            .map(|s| {
                (
                    &source[start + s.start as usize..start + s.end as usize],
                    s.kind,
                )
            })
            .collect()
    }

    #[test]
    fn an_unknown_extension_has_no_grammar() {
        assert!(line_spans("notes.xyz", "whatever").is_none());
        assert!(line_spans("Makefile", "all:").is_none());
    }

    #[test]
    fn a_file_over_the_cap_is_left_plain() {
        let huge = "// x\n".repeat(MAX_HIGHLIGHT_BYTES / 5 + 1);
        assert!(huge.len() > MAX_HIGHLIGHT_BYTES);
        assert!(line_spans("big.rs", &huge).is_none());
    }

    #[test]
    fn rust_keywords_and_strings_are_found_where_they_sit() {
        let src = "fn main() {\n    let s = \"hi\";\n}\n";
        let line = spans_on(src, "main.rs", 1);
        assert!(
            line.iter()
                .any(|(t, k)| *t == "let" && *k == HighlightKind::Keyword),
            "expected a let keyword, got {line:?}"
        );
        assert!(
            line.iter()
                .any(|(t, k)| t.contains("hi") && *k == HighlightKind::Str),
            "expected a string, got {line:?}"
        );
    }

    #[test]
    fn identifiers_are_left_alone() {
        let src = "let some_local_name = 1;\n";
        let line = spans_on(src, "a.rs", 0);
        assert!(
            !line.iter().any(|(t, _)| *t == "some_local_name"),
            "plain identifiers should stay unhighlighted, got {line:?}"
        );
    }

    #[test]
    fn a_block_comment_is_split_at_every_line_it_crosses() {
        let src = "/* one\n   two\n   three */\n";
        for line in 0..3 {
            let spans = spans_on(src, "a.rs", line);
            assert!(!spans.is_empty(), "line {line} should be covered");
            assert!(
                spans.iter().all(|(_, k)| *k == HighlightKind::Comment),
                "line {line} should be all comment, got {spans:?}"
            );
        }
    }

    #[test]
    fn spans_never_overlap_and_stay_in_order() {
        // Enough shapes on one line to nest: a string inside a call inside a
        // macro, with a comment after it.
        let src = "fn f() { println!(\"{}\", g(1)); } // done\n";
        let all = line_spans("a.rs", src).expect("parse");
        for (i, line) in all.iter().enumerate() {
            let mut last = 0;
            for span in line {
                assert!(span.start >= last, "line {i} spans out of order: {line:?}");
                assert!(
                    span.end > span.start,
                    "line {i} has an empty span: {span:?}"
                );
                last = span.end;
            }
        }
        assert!(!all[0].is_empty(), "the line should have highlights at all");
    }

    #[test]
    fn crlf_offsets_ignore_the_carriage_return() {
        let src = "let a = 1;\r\nlet b = 2;\r\n";
        let line = spans_on(src, "a.rs", 1);
        assert!(
            line.iter()
                .any(|(t, k)| *t == "let" && *k == HighlightKind::Keyword),
            "expected a let keyword on the second line, got {line:?}"
        );
        // Nothing may reach past the text the diff would actually carry.
        let all = line_spans("a.rs", src).unwrap();
        assert!(all[1].iter().all(|s| s.end <= "let b = 2;".len() as u32));
    }

    /// One landmark token per grammar. This is the test that catches a grammar
    /// wired up with a query that does not cover it: TypeScript ships only its
    /// own additions, and without JavaScript's query underneath it every file
    /// came back plain while still parsing perfectly well.
    #[test]
    fn every_grammar_finds_its_own_landmark() {
        let cases: &[(&str, &str, &str, HighlightKind)] = &[
            ("a.rs", "pub fn f() {}", "fn", HighlightKind::Keyword),
            (
                "a.ts",
                "export const x: number = 1;",
                "const",
                HighlightKind::Keyword,
            ),
            // Proves the JSX query is layered in as well as JavaScript's.
            (
                "a.tsx",
                "const El = () => <div className={x} />;",
                "className",
                HighlightKind::Property,
            ),
            ("a.py", "def f(n): return n", "def", HighlightKind::Keyword),
            (
                "a.cs",
                "public class C { }",
                "class",
                HighlightKind::Keyword,
            ),
            (
                "a.css",
                "body { color: red; }",
                "color",
                HighlightKind::Property,
            ),
            ("a.yaml", "key: value", "key", HighlightKind::Property),
            ("a.toml", "[table]", "table", HighlightKind::Type),
            ("a.json", "{\"a\": 1}", "1", HighlightKind::Number),
            ("a.md", "# Heading", "#", HighlightKind::Punctuation),
        ];
        for (path, src, text, kind) in cases {
            let found = spans_on(src, path, 0);
            assert!(
                found.iter().any(|(t, k)| t == text && k == kind),
                "{path}: expected {text:?} as {kind:?}, got {found:?}"
            );
        }
    }
}
