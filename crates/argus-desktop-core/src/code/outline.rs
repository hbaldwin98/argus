//! What the outline column says: the badge of a definition's kind, the sentence for a file that
//! defines nothing the outline knows of, and the heading over the changes (`code/stage.ts`,
//! `outline`). Which definition is under the cursor and which changed is the page's, sent as it
//! is (`CodeOutlineRow`).

use crate::code::stage::CodePage;

pub const HEADING: &str = "Outline";
pub const CLOSE: &str = "Close the outline  O";
pub const NONE: &str = "Nothing defined that the outline knows of.";
pub const CHANGES: &str = "Changes in this file";

/// The kind's first letter in capitals: `fn` is `F`, `iface` is `I`.
pub fn badge(kind: &str) -> String {
    kind.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default()
}

/// What an outline of no definitions says: nothing, while no file is read.
pub fn empty(page: &CodePage) -> &'static str {
    if page.path.is_some() { NONE } else { "" }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CODE-45: the badge is the kind's first letter, upper-cased.
    #[test]
    fn a_badge_is_the_kinds_first_letter_in_capitals() {
        for (kind, badge_of) in [("fn", "F"), ("type", "T"), ("iface", "I"), ("mod", "M"), ("macro", "M"), ("const", "C"), ("def", "D"), ("", "")] {
            assert_eq!(badge(kind), badge_of, "{kind}");
        }
    }

    #[test]
    fn an_empty_outline_says_so_only_for_a_file_that_is_read() {
        assert_eq!(empty(&CodePage { path: Some("a.rs".into()), ..CodePage::default() }), NONE);
        assert_eq!(empty(&CodePage::default()), "");
    }
}
