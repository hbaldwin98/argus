//! Where text occurs across a checkout's working tree, for the Code stage's
//! search: ripgrep's rules — literal or a regular expression, smart case,
//! whole words, include globs, `.gitignore` kept — run in process. The
//! crates are ripgrep's own (`regex`, `ignore`), so no `rg` has to be
//! installed, nor spawn a console window per search on Windows, where the
//! daemon owns none.

use std::path::Path;

use anyhow::Context;
use argus_protocol::{CodeMatch, SearchOptions, MAX_CODE_MATCHES};
use regex::{Regex, RegexBuilder};

/// Larger files are skipped: they are generated, and a match in one is
/// noise in the list.
const MAX_SEARCHED_BYTES: usize = 1024 * 1024;

/// The query as `options` read it. Smart case is ripgrep's: case matters
/// only when the query has a capital, unless `case` says it always does.
fn pattern(query: &str, options: &SearchOptions) -> anyhow::Result<Regex> {
    let body = if options.regex { query.to_owned() } else { regex::escape(query) };
    let body = if options.word { format!(r"\b(?:{body})\b") } else { body };
    let insensitive = !options.case && !query.chars().any(char::is_uppercase);
    RegexBuilder::new(&body)
        .case_insensitive(insensitive)
        .size_limit(1 << 20)
        .build()
        .with_context(|| format!("“{query}” is not a regular expression this can read"))
}

/// The files `options.glob` names, as a gitignore-style list read the way
/// ripgrep's `-g` reads it: a glob includes, `!glob` excludes.
fn files(root: &Path, glob: &str) -> anyhow::Result<Vec<String>> {
    let mut walk = ignore::WalkBuilder::new(root);
    walk.hidden(false).git_ignore(true).git_global(true).git_exclude(true).filter_entry(|e| e.file_name() != ".git");
    let globs: Vec<&str> = glob.split(',').map(str::trim).filter(|g| !g.is_empty()).collect();
    if !globs.is_empty() {
        let mut overrides = ignore::overrides::OverrideBuilder::new(root);
        for glob in globs {
            overrides.add(glob).with_context(|| format!("“{glob}” is not a glob"))?;
        }
        walk.overrides(overrides.build().context("the globs could not be read")?);
    }
    let mut out: Vec<String> = walk
        .build()
        .flatten()
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
        .filter_map(|e| e.path().strip_prefix(root).ok().map(|p| p.to_string_lossy().replace('\\', "/")))
        .collect();
    out.sort();
    Ok(out)
}

/// Every line `query` occurs on, file by file in path order, with every
/// match on it, and whether more matched than are returned. Binary files
/// and ones over 1 MiB are skipped. A query that is not a regular
/// expression, or a glob that is not one, is an error rather than nothing
/// found.
pub fn search(root: &Path, query: &str, options: &SearchOptions) -> anyhow::Result<(Vec<CodeMatch>, bool)> {
    if query.trim().is_empty() {
        return Ok((Vec::new(), false));
    }
    let pattern = pattern(query, options)?;
    let mut matches = Vec::new();
    for path in files(root, &options.glob)? {
        let Ok(bytes) = std::fs::read(root.join(&path)) else {
            continue;
        };
        if bytes.len() > MAX_SEARCHED_BYTES || bytes[..bytes.len().min(8000)].contains(&0) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        for (i, line) in text.lines().enumerate() {
            let ranges: Vec<(u32, u32)> = pattern
                .find_iter(line)
                .filter(|m| !m.is_empty())
                .map(|m| (m.start() as u32, m.end() as u32))
                .collect();
            if ranges.is_empty() {
                continue;
            }
            if matches.len() == MAX_CODE_MATCHES {
                return Ok((matches, true));
            }
            matches.push(CodeMatch { path: path.clone(), line: i as u32 + 1, text: line.to_owned(), ranges });
        }
    }
    Ok((matches, false))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checkout(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        git2::Repository::init(dir.path()).unwrap();
        for (name, body) in files {
            let path = dir.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body).unwrap();
        }
        dir
    }

    fn found(root: &Path, query: &str, options: SearchOptions) -> Vec<(String, u32, Vec<String>)> {
        let (matches, _) = search(root, query, &options).unwrap();
        matches
            .into_iter()
            .map(|m| {
                let hits = m.ranges.iter().map(|&(a, b)| m.text[a as usize..b as usize].to_owned()).collect();
                (m.path, m.line, hits)
            })
            .collect()
    }

    fn plain() -> SearchOptions {
        SearchOptions::default()
    }

    #[test]
    fn a_lowercase_query_matches_any_case_and_every_match_on_a_line() {
        let dir = checkout(&[("a.rs", "let Brief = brief;\nnothing\n"), ("b/c.rs", "brief\n")]);
        assert_eq!(
            found(dir.path(), "brief", plain()),
            [("a.rs".into(), 1, vec!["Brief".into(), "brief".into()]), ("b/c.rs".into(), 1, vec!["brief".into()])]
        );
    }

    #[test]
    fn a_query_with_a_capital_or_case_on_matches_only_that_case() {
        let dir = checkout(&[("a.rs", "brief\nBrief\n")]);
        assert_eq!(found(dir.path(), "Brief", plain()), [("a.rs".into(), 2, vec!["Brief".into()])]);
        let case = SearchOptions { case: true, ..plain() };
        assert_eq!(found(dir.path(), "brief", case), [("a.rs".into(), 1, vec!["brief".into()])]);
    }

    #[test]
    fn literal_text_is_not_read_as_a_pattern_unless_regex_is_on() {
        let dir = checkout(&[("a.rs", "a.b\naxb\n")]);
        assert_eq!(found(dir.path(), "a.b", plain()), [("a.rs".into(), 1, vec!["a.b".into()])]);
        let regex = SearchOptions { regex: true, ..plain() };
        assert_eq!(found(dir.path(), "a.b", regex).len(), 2);
    }

    #[test]
    fn whole_word_leaves_out_a_match_inside_a_longer_word() {
        let dir = checkout(&[("a.rs", "brief\nbriefs\n")]);
        let word = SearchOptions { word: true, ..plain() };
        assert_eq!(found(dir.path(), "brief", word), [("a.rs".into(), 1, vec!["brief".into()])]);
    }

    #[test]
    fn globs_name_the_files_searched_and_a_bang_leaves_some_out() {
        let dir = checkout(&[("src/a.rs", "x\n"), ("src/b.ts", "x\n"), ("docs/c.rs", "x\n")]);
        let rs = SearchOptions { glob: "src/**/*.rs".into(), ..plain() };
        assert_eq!(found(dir.path(), "x", rs).into_iter().map(|f| f.0).collect::<Vec<_>>(), ["src/a.rs"]);
        let not_docs = SearchOptions { glob: "!docs/**".into(), ..plain() };
        assert_eq!(found(dir.path(), "x", not_docs).len(), 2);
    }

    #[test]
    fn a_pattern_that_is_not_a_regex_is_an_error_not_nothing_found() {
        let dir = checkout(&[("a.rs", "x\n")]);
        let regex = SearchOptions { regex: true, ..plain() };
        assert!(search(dir.path(), "(unclosed", &regex).is_err());
    }

    #[test]
    fn ignored_and_binary_files_are_not_searched() {
        let dir = checkout(&[(".gitignore", "target/\n"), ("target/out.rs", "brief\n"), ("a.bin", "brief\0")]);
        assert!(found(dir.path(), "brief", plain()).is_empty());
    }

    #[test]
    fn too_many_matches_stop_at_the_limit_and_say_so() {
        let body = "x\n".repeat(MAX_CODE_MATCHES + 5);
        let dir = checkout(&[("a.txt", body.as_str())]);
        let (matches, truncated) = search(dir.path(), "x", &plain()).unwrap();
        assert_eq!((matches.len(), truncated), (MAX_CODE_MATCHES, true));
    }

    #[test]
    fn an_empty_query_finds_nothing() {
        let dir = checkout(&[("a.rs", "x\n")]);
        assert_eq!(search(dir.path(), "  ", &plain()).unwrap(), (Vec::new(), false));
    }
}
