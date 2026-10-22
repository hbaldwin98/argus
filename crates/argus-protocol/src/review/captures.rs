//! What a tree-sitter capture name means as a `HighlightKind`, the one table
//! the daemon's read view and the window's editor both colour by, so a token
//! is the same colour in a diff and under the cursor.

use super::HighlightKind;

/// The capture names a grammar's highlight query is asked for, each with the
/// kind it stands for. A name claims its dotted children too, so `keyword`
/// also takes `keyword.control.return` without listing every dialect's
/// spelling; see [`kind_of`].
///
/// What is deliberately absent is `variable`, which is most identifiers in
/// most files. Colouring those turns a diff into a wall of colour and buries
/// the one signal review exists for, which is which lines changed. Sorted, so
/// a reader finds a name and a test can say it appears once.
pub const CAPTURES: &[(&str, HighlightKind)] = &[
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

/// The kind of a capture, read by its first word, the part before the first
/// dot: how the page colours the editor, and what tree-sitter's own prefix
/// matching comes to for the daemon, which asks for the names above.
pub fn kind_of(capture: &str) -> Option<HighlightKind> {
    let word = capture.split('.').next().unwrap_or(capture);
    CAPTURES.iter().find(|(name, _)| *name == word).map(|(_, kind)| *kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dotted_capture_takes_its_first_words_kind() {
        assert_eq!(kind_of("keyword"), Some(HighlightKind::Keyword));
        assert_eq!(kind_of("keyword.control.return"), Some(HighlightKind::Keyword));
        assert_eq!(kind_of("string.escape"), Some(HighlightKind::Str));
        assert_eq!(kind_of("function.method.call"), Some(HighlightKind::Function));
    }

    #[test]
    fn plain_variables_and_unknown_words_stay_uncoloured() {
        assert_eq!(kind_of("variable"), None);
        assert_eq!(kind_of("variable.builtin"), None);
        assert_eq!(kind_of("markup.heading"), None);
        assert_eq!(kind_of(""), None);
    }

    #[test]
    fn only_a_whole_first_word_counts() {
        assert_eq!(kind_of("keywords"), None, "a longer word is another word");
        assert_eq!(kind_of("typeof"), None);
        assert_eq!(kind_of(".keyword"), None, "an empty first word is not `keyword`");
    }

    #[test]
    fn the_table_is_sorted_and_names_each_word_once() {
        let names: Vec<&str> = CAPTURES.iter().map(|(name, _)| *name).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(names, sorted);
    }

    mod properties {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            // The page reads a capture by its first word (EDITOR-COLOUR-1) and
            // the daemon gets the same through tree-sitter's prefix match, so
            // whatever follows the first dot must never change the kind.
            #[test]
            fn what_follows_the_first_dot_never_changes_the_kind(
                index in 0..CAPTURES.len(),
                tail in "[a-z_.]{0,16}",
            ) {
                let (name, kind) = CAPTURES[index];
                prop_assert_eq!(kind_of(&format!("{name}.{tail}")), Some(kind));
            }
        }
    }
}
