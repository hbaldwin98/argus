//! Which paths a typed query matches, best first: the review's jump field.
//!
//! The matcher is `nucleo-matcher`, the terminal client's own (`fuzzy` in
//! `argus-client`), scored for paths so a match starting a file's name
//! outranks one buried in a directory, and so a query finds the same files
//! in either client.

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// Indices into `paths` that `query` matches, best first. An empty query
/// keeps them all in their own order, which is the diff's.
pub fn paths<'a>(query: &str, paths: impl IntoIterator<Item = &'a str>) -> Vec<usize> {
    let paths: Vec<&str> = paths.into_iter().collect();
    if query.trim().is_empty() {
        return (0..paths.len()).collect();
    }
    // Smart case: a capital in the query was typed on purpose.
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
    let mut buf = Vec::new();
    let mut scored: Vec<(usize, u32)> = paths
        .iter()
        .enumerate()
        .filter_map(|(i, path)| Some((i, pattern.score(Utf32Str::new(path, &mut buf), &mut matcher)?)))
        .collect();
    // Ties keep the diff's order, so equal matches do not shuffle as the
    // query grows.
    scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    scored.into_iter().map(|(i, _)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_starting_a_file_name_comes_first() {
        let files = ["docs/prereview.md", "src/review.rs", "README.md"];
        assert_eq!(paths("rev", files), [1, 0]);
    }

    #[test]
    fn an_empty_query_keeps_every_path_in_order() {
        assert_eq!(paths(" ", ["b", "a"]), [0, 1]);
    }

    #[test]
    fn letters_need_not_be_adjacent() {
        assert_eq!(paths("rvw", ["src/review.rs", "src/history.rs"]), [0]);
    }
}
