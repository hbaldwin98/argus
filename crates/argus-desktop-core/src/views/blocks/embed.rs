//! Whether a saved view embedded in another is drawn in place, or what is said instead.
//!
//! The page writes this in `views/saved/logic.ts` with the depth cap copied; here the cap is the
//! protocol's, so the two cannot drift. A cycle is found by the chain of views being drawn, never by
//! the depth, so a wrong cap could cost a wrong line and never a loop.

use argus_protocol::MAX_EMBED_DEPTH;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Draw,
    /// The view is already being drawn: it embeds itself, through however many others.
    Cycle,
    /// Embeds were followed `MAX_EMBED_DEPTH` deep already.
    Deep,
    /// The window holds no view by that id; an embed may name one saved later.
    Missing,
}

/// Whether the saved view `target` is drawn inline. `chain` holds the saved views being drawn,
/// outermost first, and `depth` how many embeds were followed to get here.
pub fn verdict(target: &str, chain: &[String], depth: usize, held: impl Fn(&str) -> bool) -> Verdict {
    if chain.iter().any(|id| id == target) {
        Verdict::Cycle
    } else if depth >= MAX_EMBED_DEPTH {
        Verdict::Deep
    } else if held(target) {
        Verdict::Draw
    } else {
        Verdict::Missing
    }
}

/// The line drawn in place of an embed that is not drawn; none for one that is.
pub fn words(verdict: Verdict) -> Option<&'static str> {
    match verdict {
        Verdict::Draw => None,
        Verdict::Cycle => Some("view embeds itself"),
        Verdict::Deep => Some("embedded too deep"),
        Verdict::Missing => Some("view not found"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|id| (*id).to_owned()).collect()
    }

    #[test]
    fn a_view_being_drawn_is_a_cycle_whatever_the_depth() {
        assert_eq!(verdict("a", &chain(&["a"]), 0, |_| true), Verdict::Cycle);
        assert_eq!(verdict("a", &chain(&["a", "b"]), MAX_EMBED_DEPTH + 5, |_| true), Verdict::Cycle, "a cycle is named before depth is counted");
    }

    #[test]
    fn embeds_are_followed_to_the_protocols_depth_and_no_further() {
        assert_eq!(verdict("a", &[], MAX_EMBED_DEPTH - 1, |_| true), Verdict::Draw);
        assert_eq!(verdict("a", &[], MAX_EMBED_DEPTH, |_| true), Verdict::Deep);
    }

    #[test]
    fn a_view_the_window_does_not_hold_is_missing_not_an_error() {
        assert_eq!(verdict("later", &[], 0, |id| id == "now"), Verdict::Missing);
        assert_eq!(verdict("now", &[], 0, |id| id == "now"), Verdict::Draw);
    }

    #[test]
    fn each_refusal_has_its_own_line_and_a_drawn_embed_has_none() {
        let lines: Vec<_> = [Verdict::Cycle, Verdict::Deep, Verdict::Missing].map(|v| words(v).unwrap()).to_vec();
        assert_eq!(lines, ["view embeds itself", "embedded too deep", "view not found"]);
        assert_eq!(words(Verdict::Draw), None);
    }
}
