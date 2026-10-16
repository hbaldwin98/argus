//! Where text occurs across a checkout's working tree, for the Code stage's
//! search. In process, over the files `browse` lists, so `.gitignore` is
//! honoured and no `rg` has to be installed — or spawn a console window per
//! search on Windows, where the daemon owns none.

use std::path::Path;

use argus_protocol::{CodeMatch, MAX_CODE_MATCHES};

/// Larger files are skipped: they are generated, and a match in one is
/// noise in the list.
const MAX_SEARCHED_BYTES: usize = 1024 * 1024;

/// Every line `query` occurs on, file by file in path order, and whether
/// more matched than are returned. Literal; matching case only when the
/// query has a capital, as ripgrep's smart case does. Binary files are
/// skipped.
pub fn search(root: &Path, query: &str) -> (Vec<CodeMatch>, bool) {
    if query.trim().is_empty() {
        return (Vec::new(), false);
    }
    // ASCII folding keeps byte offsets in the folded line the offsets of the
    // line itself, which full Unicode folding does not.
    let exact = query.chars().any(char::is_uppercase);
    let fold = |s: &str| if exact { s.to_owned() } else { s.to_ascii_lowercase() };
    let needle = fold(query);
    let mut matches = Vec::new();
    for path in crate::browse::files(root) {
        let Ok(bytes) = std::fs::read(root.join(&path)) else {
            continue;
        };
        if bytes.len() > MAX_SEARCHED_BYTES || bytes[..bytes.len().min(8000)].contains(&0) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        for (i, line) in text.lines().enumerate() {
            let Some(start) = fold(line).find(&needle) else {
                continue;
            };
            if matches.len() == MAX_CODE_MATCHES {
                return (matches, true);
            }
            matches.push(CodeMatch {
                path: path.clone(),
                line: i as u32 + 1,
                text: line.to_owned(),
                start: start as u32,
                end: (start + needle.len()) as u32,
            });
        }
    }
    (matches, false)
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

    fn found(root: &Path, query: &str) -> Vec<(String, u32, String)> {
        let (matches, _) = search(root, query);
        matches
            .into_iter()
            .map(|m| (m.path, m.line, m.text[m.start as usize..m.end as usize].to_owned()))
            .collect()
    }

    #[test]
    fn a_lowercase_query_matches_any_case_and_names_what_it_matched() {
        let dir = checkout(&[("a.rs", "let Brief = 1;\nnothing\n"), ("b/c.rs", "brief\n")]);
        assert_eq!(
            found(dir.path(), "brief"),
            [("a.rs".into(), 1, "Brief".into()), ("b/c.rs".into(), 1, "brief".into())]
        );
    }

    #[test]
    fn a_query_with_a_capital_matches_only_that_case() {
        let dir = checkout(&[("a.rs", "brief\nBrief\n")]);
        assert_eq!(found(dir.path(), "Brief"), [("a.rs".into(), 2, "Brief".into())]);
    }

    #[test]
    fn ignored_and_binary_files_are_not_searched() {
        let dir = checkout(&[(".gitignore", "target/\n"), ("target/out.rs", "brief\n"), ("a.bin", "brief\0")]);
        assert!(found(dir.path(), "brief").is_empty());
    }

    #[test]
    fn too_many_matches_stop_at_the_limit_and_say_so() {
        let body = "x\n".repeat(MAX_CODE_MATCHES + 5);
        let dir = checkout(&[("a.txt", body.as_str())]);
        let (matches, truncated) = search(dir.path(), "x");
        assert_eq!((matches.len(), truncated), (MAX_CODE_MATCHES, true));
    }

    #[test]
    fn an_empty_query_finds_nothing() {
        let dir = checkout(&[("a.rs", "x\n")]);
        assert_eq!(search(dir.path(), "  "), (Vec::new(), false));
    }
}
