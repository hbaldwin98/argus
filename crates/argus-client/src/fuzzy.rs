//! Fuzzy filtering for the pickers.
//!
//! Wraps `nucleo-matcher` — the matcher behind Helix, and fzf's algorithm —
//! rather than shelling out to `fzf`, which is an interactive program that
//! wants a terminal of its own and cannot be embedded in a view. Keeping it
//! in-process also matters more here than usual: the daemon is started
//! `DETACHED_PROCESS` on Windows and owns no console, so every console
//! child it spawns opens a visible window (see `git::list_worktrees`).

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

pub struct Fuzzy {
    matcher: Matcher,
}

impl Fuzzy {
    pub fn new() -> Self {
        Fuzzy {
            matcher: Matcher::new(Config::DEFAULT),
        }
    }

    /// Path-aware scoring, which bonuses a match starting the last segment
    /// — the difference between "app" finding `app.rs` and `apples.rs`.
    pub fn paths() -> Self {
        Fuzzy {
            matcher: Matcher::new(Config::DEFAULT.match_paths()),
        }
    }

    /// Indices into `items` that match `query`, best score first. An empty
    /// query keeps everything in its original order — the list is usually
    /// already in a meaningful one, and reordering it on no input would be
    /// noise.
    pub fn filter(&mut self, query: &str, items: &[String]) -> Vec<usize> {
        if query.trim().is_empty() {
            return (0..items.len()).collect();
        }
        // Smart case: a lowercase query ignores case, a query with any
        // uppercase in it means the user typed that on purpose.
        let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
        let mut buf = Vec::new();
        let mut scored: Vec<(usize, u32)> = items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| {
                let score = pattern.score(Utf32Str::new(item, &mut buf), &mut self.matcher)?;
                Some((i, score))
            })
            .collect();
        // Ties break on the original order, so equal candidates don't
        // shuffle under the cursor as the query grows.
        scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        scored.into_iter().map(|(i, _)| i).collect()
    }
}

impl Default for Fuzzy {
    fn default() -> Self {
        Fuzzy::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn matched(f: &mut Fuzzy, query: &str, list: &[String]) -> Vec<String> {
        f.filter(query, list)
            .into_iter()
            .map(|i| list[i].clone())
            .collect()
    }

    #[test]
    fn an_empty_query_keeps_every_item_in_its_original_order() {
        let list = items(&["main", "feature/x", "release"]);
        let mut f = Fuzzy::new();
        assert_eq!(matched(&mut f, "", &list), list);
        assert_eq!(matched(&mut f, "   ", &list), list);
    }

    #[test]
    fn letters_match_out_of_order_positions_but_in_sequence() {
        let list = items(&["feature/login", "main", "hotfix"]);
        let mut f = Fuzzy::new();
        assert_eq!(matched(&mut f, "flog", &list), vec!["feature/login"]);
    }

    #[test]
    fn a_query_that_matches_nothing_returns_nothing() {
        let list = items(&["main", "develop"]);
        let mut f = Fuzzy::new();
        assert!(matched(&mut f, "zzzz", &list).is_empty());
    }

    #[test]
    fn a_closer_match_outranks_a_looser_one() {
        let list = items(&["a-long-name-with-app-buried", "app"]);
        let mut f = Fuzzy::new();
        assert_eq!(matched(&mut f, "app", &list)[0], "app");
    }

    #[test]
    fn a_path_can_be_matched_across_its_separators() {
        // Typing the shape of a path — a bit of each segment — is the whole
        // point of the file picker.
        let list = items(&[
            "crates/argus-client/src/app.rs",
            "crates/argusd/src/state.rs",
        ]);
        let mut f = Fuzzy::paths();
        assert_eq!(
            matched(&mut f, "casapp", &list),
            vec!["crates/argus-client/src/app.rs"]
        );
    }

    #[test]
    fn the_file_name_is_enough_to_find_a_deep_path() {
        let list = items(&["a/b/c/d/e/needle.rs", "a/haystack.rs"]);
        let mut f = Fuzzy::paths();
        assert_eq!(
            matched(&mut f, "needle", &list),
            vec!["a/b/c/d/e/needle.rs"]
        );
    }

    #[test]
    fn a_lowercase_query_ignores_case_but_an_uppercase_one_does_not() {
        let list = items(&["README.md", "readme-draft.txt"]);
        let mut f = Fuzzy::new();
        assert_eq!(
            matched(&mut f, "readme", &list).len(),
            2,
            "smart case is lenient"
        );
        assert_eq!(
            matched(&mut f, "README", &list),
            vec!["README.md"],
            "typing capitals means you meant them"
        );
    }

    #[test]
    fn equal_scores_keep_the_order_they_came_in() {
        // Otherwise candidates shuffle under the cursor as the query grows.
        let list = items(&["x/a", "y/a", "z/a"]);
        let mut f = Fuzzy::new();
        let first = matched(&mut f, "a", &list);
        assert_eq!(first, matched(&mut f, "a", &list));
    }

    #[test]
    fn filtering_an_empty_list_is_not_an_error() {
        let mut f = Fuzzy::new();
        assert!(f.filter("anything", &[]).is_empty());
    }

    #[test]
    fn indices_point_back_at_the_original_list() {
        // Callers map the choice back to a branch or a path, so the indices
        // have to survive the reordering.
        let list = items(&["zero", "one", "two"]);
        let mut f = Fuzzy::new();
        let hits = f.filter("two", &list);
        assert_eq!(hits, vec![2]);
        assert_eq!(list[hits[0]], "two");
    }
}
