//! The go-to-file field the review and the Code stage share: what is typed,
//! which match is lit, and the file Enter opens. Matching is `fuzzy`'s path
//! scoring, so it finds what the terminal's file picker would.

use argus_protocol::fuzzy::Fuzzy;

use crate::connection::page::ReviewJumpGesture;

#[derive(Debug)]
pub struct FileJump {
    query: String,
    /// The lit match, an index into what the query finds.
    at: usize,
    /// How many matches the page lists.
    shown: usize,
}

/// What a gesture in the field came to.
#[derive(Debug, PartialEq, Eq)]
pub enum Jumped {
    Kept,
    Closed,
    /// The file Enter opens, as an index into the paths given.
    Open(usize),
}

/// The paths the query matches, best first, as indices into them, and which
/// of those is lit, an index into `found`, when it is one of them.
pub struct Matches {
    pub found: Vec<usize>,
    pub lit: Option<usize>,
}

impl FileJump {
    /// A field that lists at most `shown` matches. Stepping and Enter stay
    /// within them, so the lit row is always one on the page.
    pub fn new(shown: usize) -> FileJump {
        FileJump { query: String::new(), at: 0, shown }
    }

    pub fn gesture(&mut self, gesture: ReviewJumpGesture, paths: &[String]) -> Jumped {
        match gesture {
            ReviewJumpGesture::Query { text } => {
                self.query = text;
                self.at = 0;
                Jumped::Kept
            }
            ReviewJumpGesture::Step { by } => {
                let last = self.found(paths).len().saturating_sub(1);
                self.at = self.at.saturating_add_signed(by).min(last);
                Jumped::Kept
            }
            ReviewJumpGesture::Close => Jumped::Closed,
            ReviewJumpGesture::Choose => match self.matches(paths) {
                Matches { found, lit: Some(lit) } => Jumped::Open(found[lit]),
                Matches { lit: None, .. } => Jumped::Closed,
            },
        }
    }

    /// What is lit is what Enter opens: a match the paths no longer hold
    /// lights nothing, as Enter then opens nothing.
    pub fn matches(&self, paths: &[String]) -> Matches {
        let found = self.found(paths);
        let lit = found.get(self.at).map(|_| self.at);
        Matches { found, lit }
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    /// The paths under it changed (the other side): the best match is lit again.
    pub fn rewind(&mut self) {
        self.at = 0;
    }

    fn found(&self, paths: &[String]) -> Vec<usize> {
        let mut found = Fuzzy::paths().filter(&self.query, paths);
        found.truncate(self.shown);
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(names: &[&str]) -> Vec<String> {
        names.iter().map(|&n| n.to_owned()).collect()
    }

    fn reviewish() -> Vec<String> {
        paths(&["docs/prereview.md", "src/review.rs", "README.md"])
    }

    fn query(text: &str) -> ReviewJumpGesture {
        ReviewJumpGesture::Query { text: text.into() }
    }

    #[test]
    fn every_path_matches_before_a_letter() {
        let m = FileJump::new(usize::MAX).matches(&reviewish());
        assert_eq!((m.found, m.lit), (vec![0, 1, 2], Some(0)));
    }

    #[test]
    fn typing_lights_the_best_match_again() {
        let mut j = FileJump::new(usize::MAX);
        j.gesture(ReviewJumpGesture::Step { by: 2 }, &reviewish());
        assert_eq!(j.gesture(query("rev"), &reviewish()), Jumped::Kept);
        let m = j.matches(&reviewish());
        assert_eq!((m.found, m.lit), (vec![1, 0], Some(0)));
    }

    #[test]
    fn a_step_is_held_to_the_last_match() {
        let mut j = FileJump::new(usize::MAX);
        j.gesture(ReviewJumpGesture::Step { by: 9 }, &reviewish());
        assert_eq!(j.matches(&reviewish()).lit, Some(2));
        j.gesture(ReviewJumpGesture::Step { by: -9 }, &reviewish());
        assert_eq!(j.matches(&reviewish()).lit, Some(0));
    }

    #[test]
    fn enter_opens_the_lit_match() {
        let mut j = FileJump::new(usize::MAX);
        j.gesture(query("rev"), &reviewish());
        j.gesture(ReviewJumpGesture::Step { by: 1 }, &reviewish());
        assert_eq!(j.gesture(ReviewJumpGesture::Choose, &reviewish()), Jumped::Open(0));
    }

    #[test]
    fn enter_with_no_match_closes_and_opens_nothing() {
        let mut j = FileJump::new(usize::MAX);
        j.gesture(query("zzz"), &reviewish());
        assert_eq!(j.matches(&reviewish()).lit, None);
        assert_eq!(j.gesture(ReviewJumpGesture::Choose, &reviewish()), Jumped::Closed);
    }

    #[test]
    fn what_is_lit_is_what_enter_opens_after_the_paths_shrink() {
        let mut j = FileJump::new(usize::MAX);
        j.gesture(ReviewJumpGesture::Step { by: 2 }, &reviewish());
        let fewer = paths(&["src/review.rs"]);
        assert_eq!(j.matches(&fewer).lit, None);
        assert_eq!(j.gesture(ReviewJumpGesture::Choose, &fewer), Jumped::Closed);
        j.rewind();
        assert_eq!(j.matches(&fewer).lit, Some(0));
        assert_eq!(j.gesture(ReviewJumpGesture::Choose, &fewer), Jumped::Open(0));
    }

    #[test]
    fn a_step_is_held_to_the_last_match_shown() {
        let five = paths(&["a/1.rs", "a/2.rs", "a/3.rs", "a/4.rs", "a/5.rs"]);
        let mut j = FileJump::new(2);
        j.gesture(ReviewJumpGesture::Step { by: 9 }, &five);
        let m = j.matches(&five);
        assert_eq!((m.found.len(), m.lit), (2, Some(1)));
        assert_eq!(j.gesture(ReviewJumpGesture::Choose, &five), Jumped::Open(m.found[1]));
    }
}
